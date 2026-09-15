//! Domain types for window management — zero external dependencies.

/// Metadata about a top-level OS window.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WindowInfo {
    /// Platform window handle (HWND on Windows, XID on Linux, etc.).
    pub hwnd: usize,
    pub title: String,
    pub pid: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
}

/// Filter criteria for finding a specific window.
#[derive(Debug, Default)]
pub struct WindowFilter {
    /// Case-insensitive substring match against window title.
    pub title: Option<String>,
    /// Exact match on process ID.
    pub pid: Option<u32>,
}

/// Errors that can arise during window operations.
#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("window not found")]
    NotFound,
    #[error("enumeration failed: {0}")]
    EnumerationFailed(String),
    #[error("operation failed: {0}")]
    Failed(String),
}
