//! Domain types for process management — zero external dependencies.

/// A request to launch a process.
#[derive(Debug, Clone)]
pub struct ProcessHandle {
    pub path: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

impl ProcessHandle {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            args: Vec::new(),
            cwd: None,
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }
}

/// The runtime status of a managed process.
#[derive(Debug, Clone)]
pub struct ProcessStatus {
    pub running: bool,
    pub exit_code: Option<i32>,
}

/// Errors that can arise during process operations.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("process not found: pid={0}")]
    NotFound(u32),
    #[error("launch failed: {0}")]
    LaunchFailed(String),
    #[error("kill failed: {0}")]
    KillFailed(String),
    #[error("status check failed: {0}")]
    StatusFailed(String),
}
