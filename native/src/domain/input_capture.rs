//! Domain types for input capture — zero external dependencies.

/// A captured keyboard event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedKey {
    pub key: String,
    pub action: KeyAction,
}

/// The lifecycle of a captured key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Release,
}

/// A captured mouse event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedMouse {
    pub x: i32,
    pub y: i32,
    pub button: Option<MouseButton>,
    pub action: Option<MouseAction>,
}

/// A mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// The lifecycle of a captured mouse button event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Click,
    Down,
    Up,
}

/// A single input event surfaced by the capture port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapturedEvent {
    Key(CapturedKey),
    Mouse(CapturedMouse),
}

/// Errors that can arise during input capture.
#[derive(Debug, thiserror::Error)]
pub enum InputCaptureError {
    #[error("capture failed: {0}")]
    CaptureFailed(String),
    #[error("device not available: {0}")]
    DeviceUnavailable(String),
    #[error("transport closed: {0}")]
    TransportClosed(String),
}
