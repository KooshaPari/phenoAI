//! IPC channel between overlay and host process.
//!
//! Uses tokio mpsc channels for bidirectional message passing between the
//! overlay UI layer and the host kmobile daemon.

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub kind: String,
    pub payload: serde_json::Value,
}

impl IpcMessage {
    pub fn new(kind: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            kind: kind.into(),
            payload,
        }
    }

    /// Convenience: create a message with a string payload.
    pub fn text(kind: impl Into<String>, text: impl Into<String>) -> Self {
        Self::new(kind, serde_json::Value::String(text.into()))
    }
}

/// Error type for IPC operations.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("channel closed")]
    ChannelClosed,
    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// One end of the bidirectional IPC channel — used by the overlay.
pub struct OverlaySide {
    pub tx: mpsc::Sender<IpcMessage>,
    pub rx: mpsc::Receiver<IpcMessage>,
}

/// One end of the bidirectional IPC channel — used by the host daemon.
pub struct HostSide {
    pub tx: mpsc::Sender<IpcMessage>,
    pub rx: mpsc::Receiver<IpcMessage>,
}

/// Create a linked overlay↔host channel pair with the given buffer capacity.
pub fn channel(buffer: usize) -> (OverlaySide, HostSide) {
    let (overlay_tx, host_rx) = mpsc::channel(buffer);
    let (host_tx, overlay_rx) = mpsc::channel(buffer);
    (
        OverlaySide {
            tx: overlay_tx,
            rx: overlay_rx,
        },
        HostSide {
            tx: host_tx,
            rx: host_rx,
        },
    )
}

impl OverlaySide {
    /// Send a message to the host.
    pub async fn send(&self, msg: IpcMessage) -> Result<(), IpcError> {
        self.tx.send(msg).await.map_err(|_| IpcError::ChannelClosed)
    }

    /// Receive the next message from the host (blocks until one is available).
    pub async fn recv(&mut self) -> Option<IpcMessage> {
        self.rx.recv().await
    }
}

impl HostSide {
    /// Send a message to the overlay.
    pub async fn send(&self, msg: IpcMessage) -> Result<(), IpcError> {
        self.tx.send(msg).await.map_err(|_| IpcError::ChannelClosed)
    }

    /// Receive the next message from the overlay.
    pub async fn recv(&mut self) -> Option<IpcMessage> {
        self.rx.recv().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn overlay_to_host_roundtrip() {
        let (overlay, mut host) = channel(8);
        overlay
            .send(IpcMessage::text("ping", "hello"))
            .await
            .unwrap();
        let msg = host.recv().await.unwrap();
        assert_eq!(msg.kind, "ping");
        assert_eq!(msg.payload, serde_json::Value::String("hello".to_string()));
    }

    #[tokio::test]
    async fn host_to_overlay_roundtrip() {
        let (mut overlay, host) = channel(8);
        host.send(IpcMessage::new("ack", serde_json::json!({"status": "ok"})))
            .await
            .unwrap();
        let msg = overlay.recv().await.unwrap();
        assert_eq!(msg.kind, "ack");
        assert_eq!(msg.payload["status"], "ok");
    }

    #[tokio::test]
    async fn closed_channel_returns_error() {
        let (overlay, host) = channel(1);
        drop(host);
        let result = overlay.send(IpcMessage::text("test", "payload")).await;
        assert!(matches!(result, Err(IpcError::ChannelClosed)));
    }
}
