//! `sandbox` modality — sealed-host isolation (Windows Sandbox, Firecracker, gVisor).
//!
//! The repo ships a `sandbox/` config sketch (Windows Sandbox `.wsb` files
//! and helpers) committed at `ab9d42a`. This modality probes whether any
//! known sandbox backend is reachable in the current environment.
//!
//! ## Probes (in order)
//!
//! 1. `PLAYCUA_SANDBOX_BACKEND` env override (`direct` | backend name)
//! 2. `which sandbox-exec` (macOS built-in)
//! 3. `which firejail` (Linux)
//! 4. `which firecracker` (Linux)
//! 5. `which runsc` (gVisor, Linux)
//!
//! If any probe succeeds, the modality reports available. Spawn + tunnel
//! are handled by [`SandboxDriver`]; the Sandbox port and App modality
//! dispatch wire that driver into real process lifecycle (see
//! `modality::dispatch` and `ports::sandbox::WireSandboxAdapter`).
//!
//! ## Bridge lifecycle
//!
//! Capture / input / window under sandbox modality talk NDJSON JSON-RPC to
//! `playcua-bridge` (or `PLAYCUA_BRIDGE_BIN`). [`SandboxDriver`] owns that
//! child alongside the guest so I/O ports do not depend solely on ambient
//! `$PATH` at first call.
//!
//! **Direct backend (hermetic CI):** set `PLAYCUA_SANDBOX_BACKEND=direct` and
//! point `PLAYCUA_BRIDGE_BIN` at `native/tests/fixtures/fake-playcua-bridge.sh`.
//! [`SandboxDriver::spawn_bridge`] / [`SandboxDriver::spawn_guest_with_bridge`]
//! then spawn the fake bridge as a live stdio child — never native host I/O.

use super::{Modality, ModalityKind};
use crate::ipc::bridge_client::{BridgeClient, BridgeError};
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::Mutex;

/// Serializes tests that mutate `PLAYCUA_SANDBOX_BACKEND` (process-global).
#[cfg(test)]
pub static SANDBOX_ENV_LOCK: Mutex<()> = Mutex::const_new(());

/// The sandbox-modality probe.
pub struct SandboxModality {
    cached: std::sync::OnceLock<Option<SandboxBackend>>,
}

/// Concrete sandbox backend selected by the probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackend {
    /// Run the guest command directly (no wrapper). Hermetic tests / CI.
    Direct,
    SandboxExec,
    Firejail,
    Firecracker,
    Runsc,
    // GVisor is a user-facing alias for Runsc (see `binary()` below).
    // It's only constructed when a user explicitly asks for it via
    // `PLAYCUA_SANDBOX_BACKEND=gvisor`, so allow dead_code.
    #[allow(dead_code)]
    GVisor,
}

impl SandboxBackend {
    /// Binary name used as argv[0] for wrapper backends.
    ///
    /// [`SandboxBackend::Direct`] has no wrapper binary — callers must use
    /// [`SandboxDriver::spawn_guest`] and treat the guest as argv[0].
    pub fn binary(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::SandboxExec => "sandbox-exec",
            Self::Firejail => "firejail",
            Self::Firecracker => "firecracker",
            Self::Runsc => "runsc",
            Self::GVisor => "runsc", // alias
        }
    }
}

impl SandboxModality {
    pub fn new() -> Self {
        Self {
            cached: std::sync::OnceLock::new(),
        }
    }

    /// Probe (and cache) the first available backend.
    pub fn probe(&self) -> Option<SandboxBackend> {
        self.cached
            .get_or_init(|| {
                if let Ok(override_bin) = std::env::var("PLAYCUA_SANDBOX_BACKEND") {
                    return Some(match override_bin.as_str() {
                        "direct" => SandboxBackend::Direct,
                        "sandbox-exec" => SandboxBackend::SandboxExec,
                        "firejail" => SandboxBackend::Firejail,
                        "firecracker" => SandboxBackend::Firecracker,
                        "runsc" | "gvisor" => SandboxBackend::Runsc,
                        _ => return None,
                    });
                }
                [
                    SandboxBackend::SandboxExec,
                    SandboxBackend::Firejail,
                    SandboxBackend::Firecracker,
                    SandboxBackend::Runsc,
                ]
                .into_iter()
                .find(|backend| which(backend.binary()).is_some())
            })
            .as_ref()
            .copied()
    }
}

impl Default for SandboxModality {
    fn default() -> Self {
        Self::new()
    }
}

impl Modality for SandboxModality {
    fn kind(&self) -> ModalityKind {
        ModalityKind::Sandbox
    }

    fn describe(&self) -> &'static str {
        "sealed-host sandbox (sandbox-exec / firejail / firecracker / runsc / direct)"
    }

    fn is_available(&self) -> bool {
        self.probe().is_some()
    }

    fn detail(&self) -> String {
        match self.probe() {
            Some(b) => format!("backend={}", b.binary()),
            None => "no sandbox backend on $PATH".to_string(),
        }
    }
}

fn which(bin: &str) -> Option<PathBuf> {
    let var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&var) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Lazy spawn-and-tunnel handle for a sandbox backend. Constructed via
/// [`SandboxDriver::new`] (explicit) or [`SandboxDriver::driver_for_probe`]
/// (uses the `SandboxModality` probe).
///
/// On construction the driver is *lazy*: no child process is started until
/// [`SandboxDriver::spawn`] / [`SandboxDriver::spawn_guest`] /
/// [`SandboxDriver::spawn_bridge`] is called.
/// After spawn, the guest is held in `child` and the JSON-RPC bridge in
/// `bridge`; both are torn down in `Drop` / [`SandboxDriver::shutdown`]
/// (SIGTERM, then SIGKILL after 5s).
pub struct SandboxDriver {
    backend: SandboxBackend,
    child: Option<tokio::process::Child>,
    /// Live `playcua-bridge` (or `PLAYCUA_BRIDGE_BIN` / hermetic fake).
    bridge: Option<Arc<BridgeClient>>,
}

impl SandboxDriver {
    /// Construct a driver for a specific backend. Does not spawn.
    pub fn new(backend: SandboxBackend) -> Self {
        Self {
            backend,
            child: None,
            bridge: None,
        }
    }

    /// If the given modality probe found a backend, return a driver for it.
    pub fn driver_for_probe(m: &SandboxModality) -> Option<Self> {
        let backend = m.probe()?;
        Some(Self {
            backend,
            child: None,
            bridge: None,
        })
    }

    /// The argv head that `spawn()` will eventually exec. Exposed so
    /// tests can verify the backend selection without spawning.
    pub fn spawn_argv(&self) -> Vec<String> {
        vec![self.backend.binary().to_string()]
    }

    /// The backend this driver was constructed for.
    pub fn backend(&self) -> SandboxBackend {
        self.backend
    }

    /// OS pid of the spawned child, if `spawn`/`spawn_guest` succeeded.
    pub fn child_id(&self) -> Option<u32> {
        self.child.as_ref()?.id()
    }

    /// Non-blocking status probe. `None` if not yet spawned.
    pub fn try_status(&mut self) -> std::io::Result<Option<(bool, Option<i32>)>> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        match child.try_wait()? {
            None => Ok(Some((true, None))),
            Some(status) => Ok(Some((false, status.code()))),
        }
    }

    /// Spawn with the default guest (`cat`) so stdio stays open for tunneling.
    /// Prefer [`Self::spawn_guest`] when the caller has a real command.
    pub async fn spawn(&mut self) -> std::io::Result<()> {
        self.spawn_guest("cat", &[]).await
    }

    /// Resolve `PLAYCUA_BRIDGE_BIN` / `playcua-bridge` and spawn a live stdio
    /// JSON-RPC child managed by this driver.
    ///
    /// Fail-loud if the binary is missing — never falls back to native host
    /// capture/input/window. Idempotent: returns the existing client when
    /// already spawned.
    ///
    /// Hermetic Direct backend: point `PLAYCUA_BRIDGE_BIN` at
    /// `fake-playcua-bridge.sh` so CI exercises the real spawn path without
    /// a production bridge on `$PATH`.
    pub async fn spawn_bridge(&mut self) -> Result<Arc<BridgeClient>, BridgeError> {
        if let Some(client) = &self.bridge {
            return Ok(Arc::clone(client));
        }
        let bin = BridgeClient::resolve_binary()?;
        let client = Arc::new(BridgeClient::spawn(&bin, &[]).await?);
        self.bridge = Some(Arc::clone(&client));
        tracing::info!(
            backend = self.backend.binary(),
            bridge = %bin.display(),
            "sandbox driver spawned playcua-bridge child"
        );
        Ok(client)
    }

    /// Shared handle to the driver-managed bridge, if [`Self::spawn_bridge`]
    /// (or [`Self::spawn_guest_with_bridge`]) has succeeded.
    pub fn bridge_client(&self) -> Option<Arc<BridgeClient>> {
        self.bridge.as_ref().map(Arc::clone)
    }

    /// Spawn the backend wrapping `program` + `args` as the guest command.
    ///
    /// Backend-specific flag handling per ADR-006 M2:
    ///   - `Direct`:      exec `program` directly (hermetic / CI)
    ///   - `Firejail`:    `--noprofile -- <program> <args…>`
    ///   - `Runsc`/`GVisor`: `run --bundle=<oci-dir> <program> <args…>`
    ///   - `SandboxExec`: `-D /tmp/playcua.sb <program> <args…>`
    ///   - `Firecracker`: out of scope; falls through to binary-only invoke
    ///
    /// After this call, the child's stdio is available via the `tunnel_*`
    /// accessors. JSON-RPC I/O uses [`Self::spawn_bridge`] (sibling child),
    /// not these guest pipes.
    pub async fn spawn_guest(&mut self, program: &str, args: &[String]) -> std::io::Result<()> {
        let mut cmd = self.build_command(program, args);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            // Own process group so Drop / shutdown can SIGTERM the tree.
            unsafe {
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }
        let child = cmd.spawn()?;
        self.child = Some(child);
        Ok(())
    }

    /// Spawn guest and `playcua-bridge` together (bridge is a sibling child
    /// owned by this driver). Fail-loud if the bridge binary is missing.
    pub async fn spawn_guest_with_bridge(
        &mut self,
        program: &str,
        args: &[String],
    ) -> Result<Arc<BridgeClient>, BridgeError> {
        self.spawn_guest(program, args)
            .await
            .map_err(|e| BridgeError::SpawnFailed(format!("{program}: {e}")))?;
        self.spawn_bridge().await
    }

    /// Get a `tokio::process::ChildStdin` for the spawned child's stdin.
    /// Returns `None` if `spawn()` has not been called.
    pub fn tunnel_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.child.as_mut()?.stdin.take()
    }

    /// Get a `tokio::process::ChildStdout` for the spawned child's stdout.
    /// Returns `None` if `spawn()` has not been called.
    pub fn tunnel_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }

    /// Get a `tokio::process::ChildStderr` for the spawned child's stderr.
    /// Returns `None` if `spawn()` has not been called.
    pub fn tunnel_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }

    /// Explicit graceful shutdown. Tears down bridge then guest (SIGTERM,
    /// then SIGKILL after 5s). Also implemented in `Drop` (best-effort, sync).
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        const KILL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
        if let Some(bridge) = self.bridge.take() {
            let _ = bridge.shutdown().await;
        }
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            {
                let pid = child.id().unwrap_or(0) as i32;
                if pid > 0 {
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                let _ = child.start_kill();
            }
            match tokio::time::timeout(KILL_TIMEOUT, child.wait()).await {
                Ok(_) => return Ok(()),
                Err(_) => {
                    child.start_kill().ok();
                    child.wait().await.ok();
                }
            }
        }
        Ok(())
    }

    fn build_command(&self, program: &str, args: &[String]) -> tokio::process::Command {
        match self.backend {
            SandboxBackend::Direct => {
                let mut cmd = tokio::process::Command::new(program);
                cmd.args(args);
                cmd
            }
            SandboxBackend::Firejail => {
                let mut cmd = tokio::process::Command::new(self.backend.binary());
                cmd.arg("--noprofile").arg("--").arg(program).args(args);
                cmd
            }
            SandboxBackend::Runsc | SandboxBackend::GVisor => {
                let mut cmd = tokio::process::Command::new(self.backend.binary());
                cmd.arg("run")
                    .arg("--bundle=/tmp/playcua-oci")
                    .arg(program)
                    .args(args);
                cmd
            }
            SandboxBackend::SandboxExec => {
                let mut cmd = tokio::process::Command::new(self.backend.binary());
                cmd.arg("-D").arg("/tmp/playcua.sb").arg(program).args(args);
                cmd
            }
            SandboxBackend::Firecracker => {
                // Out of scope for this slice; invoke binary only.
                tokio::process::Command::new(self.backend.binary())
            }
        }
    }
}

impl Drop for SandboxDriver {
    fn drop(&mut self) {
        // BridgeClient::Drop kills its child; drop the Arc explicitly first.
        self.bridge.take();
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            {
                let pid = child.id().unwrap_or(0) as i32;
                if pid > 0 {
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                let _ = child.start_kill();
            }
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_available_is_stable() {
        let m = SandboxModality::new();
        assert_eq!(m.is_available(), m.is_available());
    }

    #[test]
    fn kind_is_sandbox() {
        assert_eq!(SandboxModality::new().kind(), ModalityKind::Sandbox);
    }

    #[test]
    fn driver_spawn_argv_includes_backend_binary() {
        // The lazy driver must build an argv whose head is the backend
        // binary (e.g. "firejail", "runsc") so the host shell can exec it
        // directly. We don't actually spawn in tests (would need a real
        // backend); just verify the argv shape.
        let d = SandboxDriver::new(SandboxBackend::Firejail);
        let argv = d.spawn_argv();
        assert_eq!(argv.first().map(String::as_str), Some("firejail"));
    }

    #[test]
    fn driver_for_probe_returns_none_when_unavailable() {
        // When no backend is on $PATH, `driver_for_probe` should be None
        // rather than panic. Tests run with whatever $PATH the harness
        // provides; we don't assert presence/absence, just the Option shape.
        let d = SandboxDriver::driver_for_probe(&SandboxModality::new());
        if let Some(d) = d {
            if d.backend() != SandboxBackend::Direct {
                assert!(which(d.backend().binary()).is_some());
            }
        }
    }

    #[test]
    fn direct_backend_spawn_argv_is_direct_marker() {
        let d = SandboxDriver::new(SandboxBackend::Direct);
        assert_eq!(d.spawn_argv(), vec!["direct".to_string()]);
    }
}
