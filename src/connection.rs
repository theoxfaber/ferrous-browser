use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;
use futures_util::StreamExt;

use crate::cdp::{CDPClient, CDPMessage};
use crate::error::Result;

/// Manages the WebSocket connection and message routing.
pub struct Connection {
    cdp: Arc<CDPClient>,
    ws_stream: Arc<RwLock<Option<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>>,
}

impl Connection {
    /// Create a new connection
    pub fn new(
        cdp: Arc<CDPClient>,
        ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    ) -> Self {
        Connection {
            cdp,
            ws_stream: Arc::new(RwLock::new(Some(ws_stream))),
        }
    }

    /// Start the connection loop.
    pub async fn run(self) -> Result<()> {
        let mut ws_guard = self.ws_stream.write().await;
        if let Some(ws) = ws_guard.take() {
            let (sink, mut stream) = ws.split();
            drop(ws_guard);
            self.cdp.set_sink(sink).await;
            loop {
                match stream.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            if let Ok(msg) = CDPMessage::from_json(value) {
                                let _ = self.cdp.handle_message(msg).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return Ok(()),
                    Some(Err(_)) => return Ok(()),
                    Some(Ok(_)) => {}
                }
            }
        } else {
            Err(crate::error::BrowserError::websocket(
                "Connection::run",
                "WebSocket stream not available",
            ))
        }
    }
}
