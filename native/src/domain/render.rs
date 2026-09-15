//! Domain types for rendering — zero external dependencies.

/// A frame to be rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// The result of a render operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutput {
    pub width: u32,
    pub height: u32,
    pub draw_calls: u32,
}

/// Errors that can arise during rendering.
#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    #[error("invalid frame: {0}")]
    InvalidFrame(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
}
