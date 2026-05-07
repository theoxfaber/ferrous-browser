use futures_util::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

use crate::cdp::{CDPClient, CDPMessage};
use crate::error::Result;

use futures_util::stream::SplitStream;

/// Type alias for the underlying WebSocket stream to reduce type complexity.
type WsStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Manages the WebSocket connection and message routing.
pub struct Connection {
    cdp: Arc<CDPClient>,
    stream: Arc<RwLock<Option<SplitStream<WsStream>>>>,
}

impl Connection {
    /// Create a new connection
    pub fn new(
        cdp: Arc<CDPClient>,
        stream: SplitStream<WsStream>,
    ) -> Self {
        Connection {
            cdp,
            stream: Arc::new(RwLock::new(Some(stream))),
        }
    }

    /// Start the connection loop.
    pub async fn run(self) -> Result<()> {
        let mut stream_guard = self.stream.write().await;
        if let Some(mut stream) = stream_guard.take() {
            drop(stream_guard);
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
