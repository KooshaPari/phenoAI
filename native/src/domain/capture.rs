//! Domain types for screen capture — zero external dependencies.

/// A captured frame: raw PNG bytes plus dimensions.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Base64-encoded PNG bytes (matches existing IPC contract).
    pub data: String,
    pub width: u32,
    pub height: u32,
}

/// Identifies a physical monitor by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor(pub u32);

/// Opaque handle to an OS window (HWND on Windows, XID on Linux, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowHandle(pub usize);

/// Errors that can arise during screen capture.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("window not found: {0}")]
    WindowNotFound(String),
    #[error("capture failed: {0}")]
    CaptureFailed(String),
    #[error("encode failed: {0}")]
    EncodeFailed(String),
}
