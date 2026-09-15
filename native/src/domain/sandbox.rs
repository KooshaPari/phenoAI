//! Domain types for sandbox execution — zero external dependencies.

/// An opaque handle to a sandboxed process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxHandle {
    pub id: String,
}

/// The runtime status of a sandboxed process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxStatus {
    pub running: bool,
    pub exit_code: Option<i32>,
}

/// A specification for launching a sandboxed process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxSpec {
    pub command: String,
    pub args: Vec<String>,
}

/// Errors that can arise during sandbox operations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("spawn failed: {0}")]
    SpawnFailed(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("kill failed: {0}")]
    KillFailed(String),
    #[error("status check failed: {0}")]
    StatusFailed(String),
}
