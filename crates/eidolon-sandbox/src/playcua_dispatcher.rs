//! Eidolon ↔ PlayCua dispatcher.
//!
//! Per findings/2026-06-17-eidolon-absorption.md (user-confirmed: Eidolon
//! is the canonical absorb target; PlayCua stays as the desktop/sandbox
//! backend), this module provides the Eidolon-native `PlayCuaDispatcher`
//! that wraps PlayCua's `port-input`, `port-renderer`, `port-window-mgr`
//! ports behind Eidolon's `SandboxAutomator` trait.
//!
//! Architecture:
//!
//!   Eidolon `SandboxAutomator`  ← `PlayCuaDispatcher`  ←  PlayCua `port-*` ports
//!
//! The dispatcher is intentionally trait-shaped so the Eidolon crate does
//! not take a hard dependency on the PlayCua workspace. A consumer crate
//! (e.g. `eidolon-playcua-bridge`) injects a concrete `PlayCuaTransport`
//! at composition time.
//!
//! This file ships:
//!
//!   - `PlayCuaPort` — minimal trait surface covering the 3 PlayCua
//!     ports the dispatcher calls. A real implementation lives in a
//!     separate bridge crate and implements this trait against the
//!     PlayCua rmcp server.
//!   - `PlayCuaDispatcher` — `SandboxAutomator` impl that delegates
//!     start/stop/exec/resource_usage to PlayCua.
//!   - `NullPlayCuaPort` — no-op transport for tests and dev.

use async_trait::async_trait;
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, instrument, warn};

/// Minimal port surface that the dispatcher needs from PlayCua.
///
/// Real implementations wrap PlayCua's `port-input` (mouse/keyboard),
/// `port-renderer` (screen capture), and `port-window-mgr` (window
/// lifecycle). This trait is intentionally narrow so test doubles are
/// trivial.
#[async_trait]
pub trait PlayCuaPort: Send + Sync {
    /// Start a sandbox session. Returns the sandbox id assigned by PlayCua.
    async fn start(&self, image: &str, cpu: u32, mem_mb: u32) -> anyhow::Result<String>;

    /// Stop a running sandbox.
    async fn stop(&self, sandbox_id: &str) -> anyhow::Result<()>;

    /// Execute a command inside the sandbox. Returns the captured stdout.
    async fn exec(&self, sandbox_id: &str, cmd: &str) -> anyhow::Result<String>;

    /// Snapshot current resource usage.
    async fn resource_usage(&self, sandbox_id: &str) -> anyhow::Result<ResourceUsage>;

    /// Resolve sandbox metadata (image, limits, runtime, etc).
    async fn metadata(&self, sandbox_id: &str) -> anyhow::Result<SandboxMetadata>;

    /// Whether the PlayCua transport is reachable. Used by the dispatcher
    /// to choose between the real port and the null port.
    async fn is_available(&self) -> bool {
        true
    }
}

/// No-op transport for tests and offline development.
pub struct NullPlayCuaPort;

#[async_trait]
impl PlayCuaPort for NullPlayCuaPort {
    async fn start(&self, _image: &str, _cpu: u32, _mem_mb: u32) -> anyhow::Result<String> {
        warn!("NullPlayCuaPort.start — no real PlayCua transport wired");
        Ok(format!("null-{}", uuid::Uuid::new_v4()))
    }

    async fn stop(&self, sandbox_id: &str) -> anyhow::Result<()> {
        warn!(sandbox_id, "NullPlayCuaPort.stop — no-op");
        Ok(())
    }

    async fn exec(&self, sandbox_id: &str, cmd: &str) -> anyhow::Result<String> {
        warn!(sandbox_id, cmd, "NullPlayCuaPort.exec — no-op");
        Ok(String::new())
    }

    async fn resource_usage(&self, _sandbox_id: &str) -> anyhow::Result<ResourceUsage> {
        Ok(ResourceUsage {
            cpu_percent: 0.0,
            memory_mb: 0,
            disk_mb: None,
        })
    }

    async fn metadata(&self, sandbox_id: &str) -> anyhow::Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: sandbox_id.to_string(),
            image: "null:latest".to_string(),
            cpu_limit: 0,
            memory_limit_mb: 0,
            disk_limit_mb: None,
        })
    }

    async fn is_available(&self) -> bool {
        false
    }
}

/// Configuration for the dispatcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayCuaConfig {
    /// Default image when start is called without one.
    pub default_image: String,
    /// Default CPU limit.
    pub default_cpu: u32,
    /// Default memory limit (MB).
    pub default_memory_mb: u32,
    /// Timeout for start/stop/exec operations.
    pub op_timeout: Duration,
}

impl Default for PlayCuaConfig {
    fn default() -> Self {
        Self {
            default_image: "playcua/base:latest".to_string(),
            default_cpu: 2,
            default_memory_mb: 512,
            op_timeout: Duration::from_secs(30),
        }
    }
}

/// Eidolon `SandboxAutomator` that dispatches to PlayCua.
pub struct PlayCuaDispatcher<T: PlayCuaPort + 'static> {
    port: T,
    config: PlayCuaConfig,
    sandbox_id: tokio::sync::RwLock<Option<String>>,
}

impl<T: PlayCuaPort + 'static> PlayCuaDispatcher<T> {
    pub fn new(port: T, config: PlayCuaConfig) -> Self {
        Self {
            port,
            config,
            sandbox_id: tokio::sync::RwLock::new(None),
        }
    }

    pub fn with_default_config(port: T) -> Self {
        Self::new(port, PlayCuaConfig::default())
    }

    /// Returns the live sandbox id, if any.
    pub async fn current_sandbox_id(&self) -> Option<String> {
        self.sandbox_id.read().await.clone()
    }
}

/// Wraps a future with the dispatcher's `op_timeout`, converting a
/// `tokio::time::error::Elapsed` into a `PhenoError::Timeout`.
macro_rules! with_timeout {
    ($timeout:expr, $fut:expr) => {
        tokio::time::timeout($timeout, $fut)
            .await
            .map_err(|_| eidolon_core::error::PhenoError::Timeout)?
            .map_err(|e| eidolon_core::error::PhenoError::Internal(format!("{:#}", e)))
    };
}

#[async_trait::async_trait]
impl<T: PlayCuaPort + 'static> SandboxAutomator for PlayCuaDispatcher<T> {
    #[instrument(skip(self), fields(sandbox_id = tracing::field::Empty))]
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        let guard = self.sandbox_id.read().await;
        match guard.as_ref() {
            Some(id) => {
                tracing::Span::current().record("sandbox_id", id.as_str());
                with_timeout!(self.config.op_timeout, self.port.metadata(id))
            }
            None => Err(eidolon_core::error::PhenoError::Internal(
                "sandbox not started".into(),
            )),
        }
    }

    #[instrument(skip(self))]
    async fn start(&self) -> Result<()> {
        let id = with_timeout!(
            self.config.op_timeout,
            self.port.start(
                &self.config.default_image,
                self.config.default_cpu,
                self.config.default_memory_mb,
            )
        )?;
        debug!(sandbox_id = %id, "sandbox started");
        *self.sandbox_id.write().await = Some(id);
        Ok(())
    }

    #[instrument(skip(self), fields(sandbox_id = tracing::field::Empty))]
    async fn stop(&self) -> Result<()> {
        let guard = self.sandbox_id.read().await;
        match guard.as_ref() {
            Some(id) => {
                tracing::Span::current().record("sandbox_id", id.as_str());
                with_timeout!(self.config.op_timeout, self.port.stop(id))
            }
            None => Ok(()),
        }
    }

    #[instrument(skip(self), fields(sandbox_id = tracing::field::Empty))]
    async fn exec(&self, cmd: &str) -> Result<String> {
        let guard = self.sandbox_id.read().await;
        match guard.as_ref() {
            Some(id) => {
                tracing::Span::current().record("sandbox_id", id.as_str());
                with_timeout!(self.config.op_timeout, self.port.exec(id, cmd))
            }
            None => Err(eidolon_core::error::PhenoError::Internal(
                "sandbox not started".into(),
            )),
        }
    }

    #[instrument(skip(self), fields(sandbox_id = tracing::field::Empty))]
    async fn resource_usage(&self) -> Result<ResourceUsage> {
        let guard = self.sandbox_id.read().await;
        match guard.as_ref() {
            Some(id) => {
                tracing::Span::current().record("sandbox_id", id.as_str());
                with_timeout!(self.config.op_timeout, self.port.resource_usage(id))
            }
            None => Ok(ResourceUsage {
                cpu_percent: 0.0,
                memory_mb: 0,
                disk_mb: None,
            }),
        }
    }

    #[instrument(skip(self, event), fields(event_type = %event.event_type))]
    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        debug!(event_id = %event.id, platform = %event.platform, "dispatcher recorded event");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock port that records calls for assertions.
    struct MockPort {
        started: tokio::sync::Mutex<Vec<(String, u32, u32)>>,
        stopped: tokio::sync::Mutex<Vec<String>>,
        exec_calls: tokio::sync::Mutex<Vec<(String, String)>>,
    }

    impl MockPort {
        fn new() -> Self {
            Self {
                started: tokio::sync::Mutex::new(Vec::new()),
                stopped: tokio::sync::Mutex::new(Vec::new()),
                exec_calls: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl PlayCuaPort for MockPort {
        async fn start(&self, image: &str, cpu: u32, mem_mb: u32) -> anyhow::Result<String> {
            self.started
                .lock()
                .await
                .push((image.to_string(), cpu, mem_mb));
            Ok(format!("mock-{}", self.started.lock().await.len()))
        }

        async fn stop(&self, sandbox_id: &str) -> anyhow::Result<()> {
            self.stopped.lock().await.push(sandbox_id.to_string());
            Ok(())
        }

        async fn exec(&self, sandbox_id: &str, cmd: &str) -> anyhow::Result<String> {
            self.exec_calls
                .lock()
                .await
                .push((sandbox_id.to_string(), cmd.to_string()));
            Ok(format!("out:{}", cmd))
        }

        async fn resource_usage(&self, _sandbox_id: &str) -> anyhow::Result<ResourceUsage> {
            Ok(ResourceUsage {
                cpu_percent: 12.5,
                memory_mb: 256,
                disk_mb: Some(1024),
            })
        }

        async fn metadata(&self, sandbox_id: &str) -> anyhow::Result<SandboxMetadata> {
            Ok(SandboxMetadata {
                id: sandbox_id.to_string(),
                image: "mock:latest".to_string(),
                cpu_limit: 2,
                memory_limit_mb: 512,
                disk_limit_mb: Some(5120),
            })
        }
    }

    #[tokio::test]
    async fn dispatcher_start_exec_stop_lifecycle() {
        let port = MockPort::new();
        let d = PlayCuaDispatcher::with_default_config(port);
        d.start().await.unwrap();
        let id = d.current_sandbox_id().await;
        assert!(id.is_some());
        let out = d.exec("echo hi").await.unwrap();
        assert_eq!(out, "out:echo hi");
        d.stop().await.unwrap();
    }

    #[tokio::test]
    async fn dispatcher_exec_before_start_errors() {
        let port = MockPort::new();
        let d = PlayCuaDispatcher::with_default_config(port);
        let err = d.exec("echo").await.unwrap_err();
        assert!(format!("{:?}", err).contains("sandbox not started"));
    }

    #[tokio::test]
    async fn dispatcher_metadata_after_start() {
        let port = MockPort::new();
        let d = PlayCuaDispatcher::with_default_config(port);
        d.start().await.unwrap();
        let md = d.get_metadata().await.unwrap();
        assert!(md.id.starts_with("mock-"));
        assert_eq!(md.image, "mock:latest");
    }

    #[tokio::test]
    async fn dispatcher_resource_usage_after_start() {
        let port = MockPort::new();
        let d = PlayCuaDispatcher::with_default_config(port);
        d.start().await.unwrap();
        let u = d.resource_usage().await.unwrap();
        assert_eq!(u.cpu_percent, 12.5);
        assert_eq!(u.memory_mb, 256);
    }

    #[tokio::test]
    async fn null_port_is_not_available() {
        let d = PlayCuaDispatcher::with_default_config(NullPlayCuaPort);
        assert!(!d.port.is_available().await);
    }

    #[tokio::test]
    async fn null_port_exec_returns_empty() {
        let d = PlayCuaDispatcher::with_default_config(NullPlayCuaPort);
        d.start().await.unwrap();
        let out = d.exec("echo").await.unwrap();
        assert_eq!(out, "");
    }

    #[tokio::test]
    async fn config_default_values() {
        let c = PlayCuaConfig::default();
        assert_eq!(c.default_cpu, 2);
        assert_eq!(c.default_memory_mb, 512);
        assert!(c.default_image.contains("playcua"));
    }
}
