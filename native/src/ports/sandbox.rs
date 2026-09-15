//! Sandbox port — abstract boundary for sandboxed execution.
//!
//! Provides an async trait [`Sandbox`] with an in-memory test adapter.
//! The production wire adapter lives in [`crate::adapters::sandbox`].

use crate::domain::sandbox::{SandboxError, SandboxHandle, SandboxSpec, SandboxStatus};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Sandbox port — the abstract boundary for the execution-sandbox side
/// of the PlayCua hex refactor.
#[async_trait]
pub trait Sandbox: Send + Sync {
    /// Spawn a sandboxed process from the given spec.
    async fn spawn(&self, spec: &SandboxSpec) -> Result<SandboxHandle, SandboxError>;
    /// Query the runtime status of a sandboxed process.
    async fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError>;
    /// Terminate a sandboxed process.
    async fn kill(&self, handle: &SandboxHandle) -> Result<(), SandboxError>;
}

/// In-memory adapter for testing — tracks processes in a shared map.
pub struct InMemorySandboxAdapter {
    processes: Arc<Mutex<HashMap<String, SandboxStatus>>>,
}

impl InMemorySandboxAdapter {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySandboxAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Sandbox for InMemorySandboxAdapter {
    async fn spawn(&self, spec: &SandboxSpec) -> Result<SandboxHandle, SandboxError> {
        let id = format!("mock-{}-{}", spec.command, spec.args.join("-"));
        let mut processes = self.processes.lock().await;
        processes.insert(
            id.clone(),
            SandboxStatus {
                running: true,
                exit_code: None,
            },
        );
        Ok(SandboxHandle { id })
    }

    async fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError> {
        let processes = self.processes.lock().await;
        processes
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| SandboxError::NotFound(handle.id.clone()))
    }

    async fn kill(&self, handle: &SandboxHandle) -> Result<(), SandboxError> {
        let mut processes = self.processes.lock().await;
        let status = processes
            .get_mut(&handle.id)
            .ok_or_else(|| SandboxError::NotFound(handle.id.clone()))?;
        status.running = false;
        status.exit_code = Some(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The in-memory adapter must spawn a process and return a handle.
    #[tokio::test]
    async fn in_memory_sandbox_spawn_round_trip() {
        let adapter = InMemorySandboxAdapter::new();
        let spec = SandboxSpec {
            command: "echo".into(),
            args: vec!["hello".into()],
        };
        let handle = adapter.spawn(&spec).await.expect("spawn must succeed");
        assert!(handle.id.contains("echo"));
        let status = adapter.status(&handle).await.expect("status must succeed");
        assert!(status.running);
        assert!(status.exit_code.is_none());
    }

    /// The in-memory adapter must update status after kill.
    #[tokio::test]
    async fn in_memory_sandbox_kill_updates_status() {
        let adapter = InMemorySandboxAdapter::new();
        let spec = SandboxSpec {
            command: "sleep".into(),
            args: vec!["1".into()],
        };
        let handle = adapter.spawn(&spec).await.unwrap();
        adapter.kill(&handle).await.expect("kill must succeed");
        let status = adapter.status(&handle).await.expect("status must succeed");
        assert!(!status.running);
        assert_eq!(status.exit_code, Some(0));
    }

    /// The in-memory adapter must return NotFound for a bogus handle.
    #[tokio::test]
    async fn in_memory_sandbox_not_found_for_bogus_handle() {
        let adapter = InMemorySandboxAdapter::new();
        let bogus = SandboxHandle {
            id: "nonexistent".into(),
        };
        let result = adapter.status(&bogus).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SandboxError::NotFound(_)));
    }

    /// `Box<dyn Sandbox>` storage must compile (object safety).
    #[tokio::test]
    async fn sandbox_is_object_safe() {
        let s: Box<dyn Sandbox> = Box::new(InMemorySandboxAdapter::new());
        let spec = SandboxSpec {
            command: "true".into(),
            args: vec![],
        };
        let handle = s.spawn(&spec).await.expect("boxed spawn must succeed");
        let status = s.status(&handle).await.expect("boxed status must succeed");
        assert!(status.running);
    }
}
