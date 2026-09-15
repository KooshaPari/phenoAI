//! Ports layer — abstract interfaces (traits) between domain and adapters.
//!
//! Every adapter must implement exactly one port trait. The dispatcher and app
//! wiring only depend on these traits, never on concrete adapter types.

use crate::domain::{
    analysis::{AnalysisError, DiffResult, HashResult},
    capture::{CaptureError, Frame},
    input::{InputError, Key, KeyAction, MouseEvent},
    process::{ProcessError, ProcessHandle, ProcessStatus},
    window::{WindowError, WindowFilter, WindowInfo},
};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncWrite};

pub type BoxAsyncReader = Box<dyn AsyncRead + Send + Unpin>;
pub type BoxAsyncWriter = Box<dyn AsyncWrite + Send + Unpin>;

#[derive(Debug, Clone, Default)]
pub struct SpawnSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("spawn failed: {0}")]
    SpawnFailed(String),
    #[error("kill failed: {0}")]
    KillFailed(String),
    #[error("process not found: id={0}")]
    NotFound(u64),
}

pub struct ChildHandle {
    pub id: u64,
    pub stdin: BoxAsyncWriter,
    pub stdout: BoxAsyncReader,
    pub stderr: BoxAsyncReader,
}

/// Raw frame data returned by a window capture backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Minimal metadata for a capturable native window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowDescriptor {
    pub id: u64,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

/// Port for screen capture operations.
#[async_trait]
pub trait CapturePort: Send + Sync {
    /// Capture an entire display (monitor) by index.
    async fn capture_display(&self, monitor: u32) -> Result<Frame, CaptureError>;
    /// Capture a single window, optionally filtered by title substring.
    async fn capture_window(&self, title: Option<&str>) -> Result<Frame, CaptureError>;
}

/// Port for enumerating and capturing native windows as raw pixels.
pub trait WindowCapturer: Send + Sync {
    fn capture(&self, window_id: u64) -> Result<CapturedFrame, CaptureError>;
    fn list_windows(&self) -> Result<Vec<WindowDescriptor>, CaptureError>;
}

/// Port for keyboard and mouse input injection.
#[async_trait]
pub trait InputPort: Send + Sync {
    /// Press, hold, or release a keyboard key.
    async fn key_event(&self, key: Key, action: KeyAction) -> Result<(), InputError>;
    /// Type a string of text.
    async fn type_text(&self, text: &str) -> Result<(), InputError>;
    /// Perform a mouse event (move, click, or scroll).
    async fn mouse_event(&self, event: MouseEvent) -> Result<(), InputError>;
}

/// Port for window enumeration and focus.
#[async_trait]
pub trait WindowPort: Send + Sync {
    /// List all top-level windows visible to the OS.
    async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError>;
    /// Find the first window matching `filter`, or `None` if not found.
    async fn find_window(&self, filter: WindowFilter) -> Result<Option<WindowInfo>, WindowError>;
    /// Bring a window to the foreground by its platform handle.
    async fn focus_window(&self, hwnd: usize) -> Result<(), WindowError>;
}

/// Port for async child-process spawning with piped stdio.
#[async_trait]
pub trait ProcessSpawner: Send + Sync {
    /// Spawn a new process and return its handle plus piped stdio streams.
    async fn spawn(&self, spec: SpawnSpec) -> Result<ChildHandle, SpawnError>;
    /// Terminate a process by its runtime identifier.
    async fn kill(&self, id: u64) -> Result<(), SpawnError>;
}

/// Port for process lifecycle management.
#[async_trait]
pub trait ProcessPort: Send + Sync {
    /// Spawn a new process. Returns its PID.
    async fn launch(&self, handle: ProcessHandle) -> Result<u32, ProcessError>;
    /// Terminate a process by PID.
    async fn kill(&self, pid: u32) -> Result<(), ProcessError>;
    /// Query whether a process is still running and its exit code if done.
    async fn status(&self, pid: u32) -> Result<ProcessStatus, ProcessError>;
}

/// Port for image analysis operations.
#[async_trait]
pub trait AnalysisPort: Send + Sync {
    /// Compute the fraction of pixels that differ between two PNG images.
    async fn diff(&self, a: &[u8], b: &[u8], threshold: f32) -> Result<DiffResult, AnalysisError>;
    /// Compute a BLAKE3 hash of the normalized pixel data of a PNG image.
    async fn hash(&self, data: &[u8]) -> Result<HashResult, AnalysisError>;
}

// Hex-refactor ports (L4 #61)

pub mod input;
pub mod renderer;
pub mod sandbox;
