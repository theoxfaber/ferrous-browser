use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

use crate::cdp::{CDPClient, CDPMessage};
use crate::error::Result;

use futures_util::stream::SplitStream;

/// Type alias for the underlying WebSocket stream to reduce type complexity.
type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Manages the WebSocket connection and message routing.
pub struct Connection {
    cdp: Arc<CDPClient>,
    stream: Arc<RwLock<Option<SplitStream<WsStream>>>>,
}

impl Connection {
    /// Create a new connection
    pub fn new(cdp: Arc<CDPClient>, stream: SplitStream<WsStream>) -> Self {
        Connection {
            cdp,
            stream: Arc::new(RwLock::new(Some(stream))),
        }
    }

    /// Start the connection loop.
    ///
    /// Runs forever, dispatching incoming CDP messages to the `CDPClient`.
    /// When the WebSocket terminates (cleanly, with an error, or with EOF),
    /// every still-pending CDP request is failed immediately so callers don't
    /// hang waiting for a 30 second per-command timeout.
    pub async fn run(self) -> Result<()> {
        let mut stream_guard = self.stream.write().await;
        let Some(mut stream) = stream_guard.take() else {
            return Err(crate::error::BrowserError::websocket(
                "Connection::run",
                "WebSocket stream not available",
            ));
        };
        drop(stream_guard);

        let termination_reason: String = loop {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<Value>(&text) {
                        Ok(value) => match CDPMessage::from_json(value) {
                            Ok(msg) => {
                                if let Err(e) = self.cdp.handle_message(msg).await {
                                    tracing::warn!(error = %e, "handle_message failed");
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "malformed CDP message");
                            }
                        },
                        Err(e) => {
                            tracing::warn!(error = %e, "invalid JSON on CDP socket");
                        }
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    break format!("WebSocket closed by peer: {frame:?}");
                }
                None => {
                    break "WebSocket stream ended (no more frames)".to_string();
                }
                Some(Err(e)) => {
                    tracing::error!(error = %e, "WebSocket read error; tearing down");
                    break format!("WebSocket error: {e}");
                }
                Some(Ok(_)) => {
                    // Binary, Ping, Pong, and Frame messages aren't part of
                    // the CDP text protocol; ignore.
                }
            }
        };

        // Wake every in-flight `send_command` with a clean failure rather
        // than letting each one time out individually.
        self.cdp.fail_all_pending(&termination_reason).await;
        Ok(())
    }
}
