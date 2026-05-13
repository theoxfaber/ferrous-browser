use futures_util::SinkExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::{timeout, Duration};
use tokio_tungstenite::tungstenite::Message;
use tracing::Instrument;

use crate::error::{BrowserError, Result};

/// Represents a CDP command request
#[derive(Debug, Clone)]
pub struct CDPRequest {
    /// Unique request ID
    pub id: u32,
    /// CDP method name
    pub method: String,
    /// Optional parameters for the method
    pub params: Option<Value>,
    /// Optional session ID for targeting specific pages
    pub session_id: Option<String>,
}

impl CDPRequest {
    /// Create a new CDP request
    pub fn new(id: u32, method: String, params: Option<Value>) -> Self {
        Self {
            id,
            method,
            params,
            session_id: None,
        }
    }

    /// Create a CDP request with session ID
    pub fn with_session(
        id: u32,
        method: String,
        params: Option<Value>,
        session_id: String,
    ) -> Self {
        Self {
            id,
            method,
            params,
            session_id: Some(session_id),
        }
    }

    /// Convert to JSON value for sending
    pub fn to_json(&self) -> Value {
        let mut obj = json!({
            "id": self.id,
            "method": self.method,
        });

        if let Some(session_id) = &self.session_id {
            obj["sessionId"] = json!(session_id);
        }

        if let Some(params) = &self.params {
            obj["params"] = params.clone();
        }

        obj
    }
}

/// Represents a CDP event or response
#[derive(Debug, Clone)]
pub struct CDPMessage {
    /// Response ID (if this is a response)
    pub id: Option<u32>,
    /// Event method name (if this is an event)
    pub method: Option<String>,
    /// Event parameters
    pub params: Option<Value>,
    /// Command result (if successful)
    pub result: Option<Value>,
    /// Error object (if failed)
    pub error: Option<Value>,
    /// Session ID — identifies which page/target this message belongs to.
    /// This is the critical field for multi-page session isolation.
    pub session_id: Option<String>,
}

impl CDPMessage {
    /// Parse a CDP message from JSON value
    pub fn from_json(value: Value) -> Result<Self> {
        Ok(CDPMessage {
            id: value.get("id").and_then(|v| v.as_u64()).map(|v| v as u32),
            method: value
                .get("method")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            params: value.get("params").cloned(),
            result: value.get("result").cloned(),
            error: value.get("error").cloned(),
            // Chrome always includes sessionId in session-scoped messages
            session_id: value
                .get("sessionId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }
}

/// Type for WebSocket sink
pub type WebSocketSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

/// Core CDP client that manages WebSocket connection and message routing.
///
/// **Concurrency model:**
/// - Outgoing writes go through an unbounded mpsc channel feeding a dedicated
///   writer task (`spawn_writer_task`). Many callers can send concurrently
///   without contending on a lock.
/// - Incoming reads are dispatched by `Connection::run` calling
///   `handle_message`, which only briefly holds a sync mutex on
///   `pending_responses`, with no awaits while the lock is held.
pub struct CDPClient {
    ws_url: String,
    message_id_counter: Arc<AtomicU32>,
    /// Pending command responses, keyed by request id. Guarded by a *sync*
    /// mutex; we never `.await` while holding it, so a `std::sync::Mutex`
    /// is correct and skips `tokio::sync::RwLock`'s wait-queue overhead.
    pending_responses: Arc<StdMutex<HashMap<u32, oneshot::Sender<Value>>>>,
    /// Broadcast channel carrying ALL CDP events (`method.is_some()`).
    /// Subscribers filter by method name and session_id themselves.
    event_broadcast: broadcast::Sender<CDPMessage>,
    /// Sender side of the writer-task mailbox. `None` until `set_writer` is
    /// called from `Browser::connect_internal`.
    ws_tx: Arc<StdMutex<Option<mpsc::UnboundedSender<Message>>>>,
    /// Latched "is the CDP connection dead?" signal. Transitions false→true
    /// exactly once when `fail_all_pending` is called from
    /// `Connection::run`'s termination. Long-running wait loops in
    /// `Page::goto` watch this so they can return a disconnect error
    /// promptly instead of hanging on a broadcast channel that never sees
    /// new events but also never closes (CDPClient is kept alive by all
    /// Pages holding Arc clones, even after the underlying WebSocket is
    /// gone — so `RecvError::Closed` on its own is not a reliable signal).
    disconnected_tx: watch::Sender<bool>,
}

impl CDPClient {
    /// Create a new CDP client
    pub fn new(ws_url: String) -> Self {
        let (event_broadcast, _) = broadcast::channel(1024);
        let (disconnected_tx, _) = watch::channel(false);
        Self {
            ws_url,
            message_id_counter: Arc::new(AtomicU32::new(1)),
            pending_responses: Arc::new(StdMutex::new(HashMap::new())),
            event_broadcast,
            ws_tx: Arc::new(StdMutex::new(None)),
            disconnected_tx,
        }
    }

    /// Subscribe to the disconnect signal. Returns a `watch::Receiver<bool>`
    /// that starts at `false` and transitions to `true` exactly once when
    /// the CDP connection terminates (see `fail_all_pending`).
    pub fn disconnected(&self) -> watch::Receiver<bool> {
        self.disconnected_tx.subscribe()
    }

    /// Install the writer-task mailbox. Called once by
    /// `Browser::connect_internal` right after the writer task is spawned.
    pub fn set_writer(&self, tx: mpsc::UnboundedSender<Message>) {
        *self.ws_tx.lock().expect("ws_tx mutex poisoned") = Some(tx);
    }

    /// Generate the next message ID
    pub fn next_id(&self) -> u32 {
        self.message_id_counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Connect to the Chrome DevTools Protocol WebSocket
    pub async fn connect(
        &self,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    > {
        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.ws_url)
            .await
            .map_err(|e| BrowserError::connection_failed(&self.ws_url, e.to_string()))?;

        Ok(ws_stream)
    }

    /// Send raw message through the writer task. Synchronous and (modulo one
    /// brief sync-mutex acquire) lock-free, because the mpsc channel fan-in
    /// is the contention point now, not a per-call lock.
    pub fn send_raw(&self, msg: String) -> Result<()> {
        let tx_guard = self.ws_tx.lock().expect("ws_tx mutex poisoned");
        let tx = tx_guard.as_ref().ok_or_else(|| {
            BrowserError::websocket("send_raw", "WebSocket writer not initialised")
        })?;
        tx.send(Message::Text(msg))
            .map_err(|_| BrowserError::websocket("send_raw", "WebSocket writer task ended"))
    }

    /// Subscribe to all CDP events (unfiltered broadcast receiver).
    ///
    /// Callers are responsible for filtering by `msg.method` and
    /// `msg.session_id` as needed.
    ///
    /// **IMPORTANT:** Subscribe *before* sending the CDP command that
    /// triggers the event to avoid the race where Chrome replies before the
    /// receiver is registered.
    pub fn subscribe_events(&self) -> broadcast::Receiver<CDPMessage> {
        self.event_broadcast.subscribe()
    }

    /// Send a command and wait for response with timeout.
    ///
    /// The response handler is registered **before** the message is sent so
    /// that fast Chrome replies are never dropped.
    #[tracing::instrument(level = "info", skip(self, params), fields(method = %method, id))]
    pub async fn send_command(&self, method: String, params: Option<Value>) -> Result<Value> {
        let id = self.next_id();
        tracing::Span::current().record("id", id);
        let request = CDPRequest::new(id, method.clone(), params);

        // ── Register handler BEFORE sending ──────────────────────────────────
        let (tx, rx) = oneshot::channel();
        self.register_response_handler(id, tx);
        let json_str = tracing::info_span!("serialize").in_scope(|| request.to_json().to_string());
        let bytes = json_str.len();
        tracing::info_span!("ws_send", bytes).in_scope(|| self.send_raw(json_str))?;
        // ─────────────────────────────────────────────────────────────────────

        const TIMEOUT_SECS: u64 = 30;
        let wait = async {
            match timeout(Duration::from_secs(TIMEOUT_SECS), rx).await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(_)) => Err(BrowserError::command_failed(
                    &method,
                    "response channel closed unexpectedly",
                )),
                Err(_) => {
                    self.pending_responses
                        .lock()
                        .expect("pending_responses mutex poisoned")
                        .remove(&id);
                    Err(BrowserError::timeout(
                        format!("waiting for response to '{method}'"),
                        TIMEOUT_SECS,
                    ))
                }
            }
        };
        wait.instrument(tracing::info_span!("await_response")).await
    }

    /// Send a command to a specific page session.
    ///
    /// The response handler is registered **before** the message is sent.
    #[tracing::instrument(level = "info", skip(self, params), fields(method = %method, id, session_id = %session_id))]
    pub async fn send_command_with_session(
        &self,
        session_id: &str,
        method: String,
        params: Option<Value>,
    ) -> Result<Value> {
        let id = self.next_id();
        tracing::Span::current().record("id", id);
        let request = CDPRequest::with_session(id, method.clone(), params, session_id.to_string());

        // ── Register handler BEFORE sending ──────────────────────────────────
        let (tx, rx) = oneshot::channel();
        self.register_response_handler(id, tx);
        let json_str = tracing::info_span!("serialize").in_scope(|| request.to_json().to_string());
        let bytes = json_str.len();
        tracing::info_span!("ws_send", bytes).in_scope(|| self.send_raw(json_str))?;
        // ─────────────────────────────────────────────────────────────────────

        const TIMEOUT_SECS: u64 = 30;
        let wait = async {
            match timeout(Duration::from_secs(TIMEOUT_SECS), rx).await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(_)) => Err(BrowserError::command_failed(
                    &method,
                    "response channel closed unexpectedly",
                )),
                Err(_) => {
                    self.pending_responses
                        .lock()
                        .expect("pending_responses mutex poisoned")
                        .remove(&id);
                    Err(BrowserError::timeout(
                        format!("waiting for response to '{method}'"),
                        TIMEOUT_SECS,
                    ))
                }
            }
        };
        wait.instrument(tracing::info_span!("await_response")).await
    }

    /// Register a pending response handler. Synchronous: the sync mutex
    /// only protects the HashMap insert.
    pub fn register_response_handler(&self, id: u32, tx: oneshot::Sender<Value>) {
        self.pending_responses
            .lock()
            .expect("pending_responses mutex poisoned")
            .insert(id, tx);
    }

    /// Drop every pending response sender. Any `send_command` currently
    /// awaiting one of these will see its oneshot close immediately and
    /// return `BrowserError::command_failed("…", "response channel closed…")`,
    /// instead of waiting out the 30-second timeout. Call this when the
    /// underlying WebSocket dies.
    pub fn fail_all_pending(&self, reason: &str) {
        let mut pending = self
            .pending_responses
            .lock()
            .expect("pending_responses mutex poisoned");
        let count = pending.len();
        pending.clear(); // dropping the senders signals the receivers
        drop(pending);
        // Latch the disconnect signal so long-running wait loops watching
        // `disconnected()` can return promptly. We intentionally do this
        // after the pending-responses drop so any task observing both
        // sees a consistent "everything is dead" state.
        let _ = self.disconnected_tx.send(true);
        if count > 0 {
            tracing::warn!(
                pending_count = count,
                reason = reason,
                "WebSocket terminated; failing in-flight CDP requests"
            );
        }
    }

    /// Handle an incoming CDP message; called by `Connection::run`.
    /// Synchronous: no `.await` happens while the pending-responses mutex
    /// is held.
    #[tracing::instrument(level = "debug", skip_all, fields(method = ?msg.method, id = ?msg.id))]
    pub fn handle_message(&self, msg: CDPMessage) -> Result<()> {
        if let Some(id) = msg.id {
            // It's a response to one of our commands
            let tx = self
                .pending_responses
                .lock()
                .expect("pending_responses mutex poisoned")
                .remove(&id);
            if let Some(tx) = tx {
                if let Some(error) = msg.error {
                    let _ = tx.send(json!({ "error": error }));
                } else if let Some(result) = msg.result {
                    let _ = tx.send(result);
                } else {
                    let _ = tx.send(json!({}));
                }
            }
        } else if msg.method.is_some() {
            // It's an event — broadcast to all subscribers.
            // Subscribers filter by method + session_id.
            let _ = self.event_broadcast.send(msg);
        }
        Ok(())
    }
}

/// Spawn the dedicated writer task that drains the mpsc and writes to the
/// WebSocket sink. The task ends when the channel closes (all senders
/// dropped) or when a write fails. On a write failure it also fails every
/// in-flight CDP request via `fail_all_pending` so callers see an immediate
/// error instead of the 30 s timeout.
pub fn spawn_writer_task(
    mut sink: WebSocketSink,
    mut rx: mpsc::UnboundedReceiver<Message>,
    cdp: Arc<CDPClient>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = sink.send(msg).await {
                tracing::error!(error = %e, "WebSocket write error; terminating writer");
                cdp.fail_all_pending(&format!("write error: {e}"));
                return;
            }
        }
        tracing::debug!("WebSocket writer task exiting (channel closed)");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdp_request_creation() {
        let req = CDPRequest::new(
            1,
            "Page.navigate".to_string(),
            Some(json!({"url": "https://example.com"})),
        );
        assert_eq!(req.id, 1);
        assert_eq!(req.method, "Page.navigate");
        assert_eq!(req.params.as_ref().unwrap()["url"], "https://example.com");
    }

    #[test]
    fn test_cdp_request_to_json() {
        let req = CDPRequest::new(
            1,
            "Page.navigate".to_string(),
            Some(json!({"url": "https://example.com"})),
        );
        let json = req.to_json();
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "Page.navigate");
        assert_eq!(json["params"]["url"], "https://example.com");
    }

    #[test]
    fn test_cdp_message_from_json() {
        let json_val = json!({
            "id": 1,
            "result": {"url": "https://example.com"},
            "sessionId": "SES001"
        });
        let msg = CDPMessage::from_json(json_val).unwrap();
        assert_eq!(msg.id, Some(1));
        assert_eq!(msg.result.as_ref().unwrap()["url"], "https://example.com");
        assert_eq!(msg.session_id.as_deref(), Some("SES001"));
    }

    #[test]
    fn test_cdp_message_session_id_parsed() {
        let event = json!({
            "method": "Page.loadEventFired",
            "params": {},
            "sessionId": "ABC123"
        });
        let msg = CDPMessage::from_json(event).unwrap();
        assert_eq!(msg.method.as_deref(), Some("Page.loadEventFired"));
        assert_eq!(msg.session_id.as_deref(), Some("ABC123"));
    }

    #[test]
    fn test_cdp_request_with_session() {
        let req = CDPRequest::with_session(
            2,
            "Runtime.evaluate".to_string(),
            Some(json!({"expression": "1+1"})),
            "SES001".to_string(),
        );
        let json = req.to_json();
        assert_eq!(json["sessionId"], "SES001");
        assert_eq!(json["method"], "Runtime.evaluate");
    }
}
