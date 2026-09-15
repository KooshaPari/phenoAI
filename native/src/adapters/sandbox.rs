//! Production wire adapter for the Sandbox port.
//!
//! Spawns via [`SandboxDriver`] after probing [`SandboxModality`]. Fail-loud
//! when no backend is available (no silent stub success).
//!
//! On each guest spawn the driver also ensures a live `playcua-bridge` child
//! (`PLAYCUA_BRIDGE_BIN` or `playcua-bridge` on `$PATH`) so capture/input/window
//! ports share a driver-managed JSON-RPC session — not ambient PATH alone.
//!
//! Hermetic / CI: set `PLAYCUA_SANDBOX_BACKEND=direct` and point
//! `PLAYCUA_BRIDGE_BIN` at `fake-playcua-bridge.sh`.

use crate::domain::sandbox::{SandboxError, SandboxHandle, SandboxSpec, SandboxStatus};
use crate::ipc::bridge_client::BridgeClient;
use crate::modality::sandbox::{SandboxDriver, SandboxModality};
use crate::ports::sandbox::Sandbox;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared slot filled by [`WireSandboxAdapter`] when the driver spawns the
/// bridge; consumed by [`crate::adapters::sandbox_bridge::SandboxBridgePorts`].
pub type SharedBridgeSlot = Arc<Mutex<Option<Arc<BridgeClient>>>>;

/// Production wire adapter — dispatches through [`SandboxDriver`].
pub struct WireSandboxAdapter {
    live: Arc<Mutex<HashMap<String, SandboxDriver>>>,
    /// Driver-spawned bridge shared with sandbox I/O ports.
    bridge: SharedBridgeSlot,
}

impl WireSandboxAdapter {
    pub fn new() -> Self {
        Self {
            live: Arc::new(Mutex::new(HashMap::new())),
            bridge: Arc::new(Mutex::new(None)),
        }
    }

    /// Shared bridge slot for [`SandboxBridgePorts::from_shared_slot`].
    pub fn bridge_slot(&self) -> SharedBridgeSlot {
        Arc::clone(&self.bridge)
    }

    /// Ensure a live bridge via [`SandboxDriver::spawn_bridge`], publishing
    /// it into the shared slot. Fail-loud if the binary is missing.
    pub async fn ensure_bridge(&self) -> Result<Arc<BridgeClient>, SandboxError> {
        {
            let guard = self.bridge.lock().await;
            if let Some(client) = guard.as_ref() {
                return Ok(Arc::clone(client));
            }
        }
        let modality = SandboxModality::new();
        let mut driver = SandboxDriver::driver_for_probe(&modality).ok_or_else(|| {
            SandboxError::SpawnFailed(
                "no sandbox backend on $PATH; install firejail/sandbox-exec/runsc \
                 or set PLAYCUA_SANDBOX_BACKEND=direct for hermetic spawn"
                    .into(),
            )
        })?;
        let client = driver.spawn_bridge().await.map_err(|e| {
            SandboxError::SpawnFailed(format!(
                "playcua-bridge required for sandbox I/O but missing: {e}"
            ))
        })?;
        // Keep the driver alive only long enough to hand off the Arc; the
        // BridgeClient owns the child process independently.
        let mut slot = self.bridge.lock().await;
        if let Some(existing) = slot.as_ref() {
            return Ok(Arc::clone(existing));
        }
        *slot = Some(Arc::clone(&client));
        Ok(client)
    }
}

impl Default for WireSandboxAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Sandbox for WireSandboxAdapter {
    async fn spawn(&self, spec: &SandboxSpec) -> Result<SandboxHandle, SandboxError> {
        let modality = SandboxModality::new();
        let mut driver = SandboxDriver::driver_for_probe(&modality).ok_or_else(|| {
            SandboxError::SpawnFailed(
                "no sandbox backend on $PATH; install firejail/sandbox-exec/runsc \
                 or set PLAYCUA_SANDBOX_BACKEND=direct for hermetic spawn"
                    .into(),
            )
        })?;
        driver
            .spawn_guest(&spec.command, &spec.args)
            .await
            .map_err(|e| SandboxError::SpawnFailed(format!("{}: {e}", spec.command)))?;
        // Sibling bridge process (PLAYCUA_BRIDGE_BIN / playcua-bridge) —
        // published into the shared slot for I/O ports. Kept off the guest
        // driver so kill(guest) does not tear down JSON-RPC.
        self.ensure_bridge().await?;
        let id = driver
            .child_id()
            .ok_or_else(|| SandboxError::SpawnFailed("child process missing pid".into()))?
            .to_string();
        self.live.lock().await.insert(id.clone(), driver);
        Ok(SandboxHandle { id })
    }

    async fn status(&self, handle: &SandboxHandle) -> Result<SandboxStatus, SandboxError> {
        let mut live = self.live.lock().await;
        let driver = live
            .get_mut(&handle.id)
            .ok_or_else(|| SandboxError::NotFound(handle.id.clone()))?;
        match driver.try_status() {
            Ok(Some((running, exit_code))) => Ok(SandboxStatus { running, exit_code }),
            Ok(None) => Err(SandboxError::StatusFailed(format!(
                "driver for {} has no child",
                handle.id
            ))),
            Err(e) => Err(SandboxError::StatusFailed(e.to_string())),
        }
    }

    async fn kill(&self, handle: &SandboxHandle) -> Result<(), SandboxError> {
        let mut live = self.live.lock().await;
        let mut driver = live
            .remove(&handle.id)
            .ok_or_else(|| SandboxError::NotFound(handle.id.clone()))?;
        driver
            .shutdown()
            .await
            .map_err(|e| SandboxError::KillFailed(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::bridge_client::BRIDGE_ENV_LOCK;
    use crate::modality::sandbox::SANDBOX_ENV_LOCK;

    /// Wire adapter fails loud when no backend is configured.
    #[tokio::test]
    async fn wire_sandbox_fails_loud_without_backend() {
        let _guard = SANDBOX_ENV_LOCK.lock().await;
        let _bguard = BRIDGE_ENV_LOCK.lock().await;
        let prev = std::env::var("PLAYCUA_SANDBOX_BACKEND").ok();
        std::env::set_var("PLAYCUA_SANDBOX_BACKEND", "not-a-real-backend");
        let adapter = WireSandboxAdapter::new();
        let spec = SandboxSpec {
            command: "echo".into(),
            args: vec!["test".into()],
        };
        let err = adapter.spawn(&spec).await.expect_err("must fail loud");
        assert!(
            matches!(err, SandboxError::SpawnFailed(_)),
            "expected SpawnFailed, got {err:?}"
        );
        match prev {
            Some(v) => std::env::set_var("PLAYCUA_SANDBOX_BACKEND", v),
            None => std::env::remove_var("PLAYCUA_SANDBOX_BACKEND"),
        }
    }

    /// Missing bridge binary fails loud on spawn (no native host leak).
    #[tokio::test]
    async fn wire_sandbox_fails_loud_without_bridge() {
        let _guard = SANDBOX_ENV_LOCK.lock().await;
        let _bguard = BRIDGE_ENV_LOCK.lock().await;
        let prev_backend = std::env::var("PLAYCUA_SANDBOX_BACKEND").ok();
        let prev_bridge = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
        std::env::set_var("PLAYCUA_SANDBOX_BACKEND", "direct");
        std::env::set_var("PLAYCUA_BRIDGE_BIN", "/nonexistent/playcua-bridge");
        let adapter = WireSandboxAdapter::new();
        let spec = SandboxSpec {
            command: "sleep".into(),
            args: vec!["30".into()],
        };
        let err = adapter.spawn(&spec).await.expect_err("must fail loud");
        let msg = err.to_string();
        assert!(
            msg.contains("bridge") || msg.contains("PLAYCUA_BRIDGE_BIN"),
            "unexpected: {msg}"
        );
        match prev_backend {
            Some(v) => std::env::set_var("PLAYCUA_SANDBOX_BACKEND", v),
            None => std::env::remove_var("PLAYCUA_SANDBOX_BACKEND"),
        }
        match prev_bridge {
            Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
            None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
        }
    }

    /// Wire adapter + Direct backend + fake bridge actually spawns and kills.
    #[tokio::test]
    async fn wire_sandbox_direct_spawn_and_kill() {
        let _guard = SANDBOX_ENV_LOCK.lock().await;
        let _bguard = BRIDGE_ENV_LOCK.lock().await;
        let prev_backend = std::env::var("PLAYCUA_SANDBOX_BACKEND").ok();
        let prev_bridge = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
        std::env::set_var("PLAYCUA_SANDBOX_BACKEND", "direct");

        let bin = {
            let fixture = if cfg!(windows) {
                "fake-playcua-bridge.cmd"
            } else {
                "fake-playcua-bridge.sh"
            };
            let mut candidates = vec![];
            if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
                candidates.push(
                    std::path::PathBuf::from(m)
                        .join("tests/fixtures")
                        .join(fixture),
                );
            }
            candidates.push(std::path::PathBuf::from("native/tests/fixtures").join(fixture));
            candidates.push(std::path::PathBuf::from("tests/fixtures").join(fixture));
            candidates
                .into_iter()
                .find(|p| p.is_file())
                .expect("platform fake-playcua-bridge fixture")
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin, perms).ok();
        }
        std::env::set_var("PLAYCUA_BRIDGE_BIN", &bin);

        let adapter = WireSandboxAdapter::new();
        #[cfg(unix)]
        let spec = SandboxSpec {
            command: "sleep".into(),
            args: vec!["30".into()],
        };
        #[cfg(windows)]
        let spec = SandboxSpec {
            command: "cmd".into(),
            args: vec!["/C".into(), "ping -n 30 127.0.0.1 >NUL".into()],
        };
        let handle = adapter
            .spawn(&spec)
            .await
            .expect("direct spawn must succeed");
        assert!(
            adapter.bridge_slot().lock().await.is_some(),
            "bridge must be published alongside guest"
        );
        let status = adapter.status(&handle).await.expect("status");
        assert!(status.running, "child should still be running");
        adapter.kill(&handle).await.expect("kill must succeed");
        let missing = adapter.status(&handle).await;
        assert!(matches!(missing, Err(SandboxError::NotFound(_))));
        match prev_backend {
            Some(v) => std::env::set_var("PLAYCUA_SANDBOX_BACKEND", v),
            None => std::env::remove_var("PLAYCUA_SANDBOX_BACKEND"),
        }
        match prev_bridge {
            Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
            None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
        }
    }
}
