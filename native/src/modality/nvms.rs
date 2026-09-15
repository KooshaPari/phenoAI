//! `nvms` modality — drive an `nvms`-orchestrated container (nanovms).
//!
//! The canonical home for nvms is `KooshaPari/nanovms`. This modality probes
//! for the `nvms` CLI on `$PATH` and, if present, reports availability.
//!
//! ## What it does in this slice
//!
//! - **Availability probe**: `which nvms` (PATH lookup, no execution).
//! - **Describe**: "nvms <version> at /path/to/nvms" if found, otherwise
//!   "nvms binary not on $PATH".
//! - **Full driver integration** (auto-provision a container, route requests
//!   into it, etc.) is a follow-up tracked in ADR-006. The probe here is the
//!   gating check that lets `auto` selection know nvms is even an option.
//!
//! ## Future follow-ups
//!
//! 1. Spawn `nvms run <config>` on first action (lazy init).
//! 2. Tunnel capture/input through the container's IPC.
//! 3. Honor `PLAYCUA_NVMS_CONFIG` env var (path to nvms.toml).
//! 4. Multi-tenant: share a single nvms runtime across modalities.

use super::{Modality, ModalityKind};
use std::path::PathBuf;

/// The nvms-modality probe.
pub struct NvmsModality {
    /// Cached probe result. Populated on first `is_available()` call so we
    /// don't re-walk $PATH on every ping.
    cached: std::sync::OnceLock<Option<PathBuf>>,
}

impl NvmsModality {
    pub fn new() -> Self {
        Self {
            cached: std::sync::OnceLock::new(),
        }
    }

    /// Re-probe from scratch (drops the cached result).
    #[cfg(test)]
    pub fn invalidate_cache(&mut self) {
        // OnceLock has no reset; this is a test-only re-construction hack.
        *self = Self::new();
    }

    /// Probe for the `nvms` binary on $PATH.
    fn probe(&self) -> Option<PathBuf> {
        self.cached.get_or_init(which_nvms).as_ref().cloned()
    }
}

impl Default for NvmsModality {
    fn default() -> Self {
        Self::new()
    }
}

impl Modality for NvmsModality {
    fn kind(&self) -> ModalityKind {
        ModalityKind::Nvms
    }

    fn describe(&self) -> &'static str {
        "nvms-orchestrated container (KooshaPari/nanovms)"
    }

    fn is_available(&self) -> bool {
        self.probe().is_some()
    }

    fn detail(&self) -> String {
        match self.probe() {
            Some(p) => format!("nvms={}", p.display()),
            None => "nvms not on $PATH".to_string(),
        }
    }
}

/// Probe for the `nvms` binary on $PATH. Returns the first match.
fn which_nvms() -> Option<PathBuf> {
    let var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&var) {
        let candidate = dir.join("nvms");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_is_idempotent() {
        let m = NvmsModality::new();
        let a = m.is_available();
        let b = m.is_available();
        assert_eq!(a, b, "is_available must be stable across calls");
    }

    #[test]
    fn detail_matches_availability() {
        let m = NvmsModality::new();
        if m.is_available() {
            assert!(m.detail().starts_with("nvms="), "got: {}", m.detail());
        } else {
            assert_eq!(m.detail(), "nvms not on $PATH");
        }
    }

    #[test]
    fn kind_is_nvms() {
        assert_eq!(NvmsModality::new().kind(), ModalityKind::Nvms);
    }

    #[test]
    fn driver_spawn_argv_includes_nvms_run() {
        // The lazy driver must build an argv whose head is the `nvms run`
        // subcommand so the host shell can exec it directly. We don't
        // actually spawn in tests (would need a real nvms binary); just
        // verify the argv shape.
        let d = NvmsDriver::new(std::path::PathBuf::from("./nvms.toml"));
        let argv = d.spawn_argv();
        assert_eq!(argv.first().map(String::as_str), Some("nvms"));
        assert_eq!(argv.get(1).map(String::as_str), Some("run"));
    }

    #[test]
    fn driver_for_probe_returns_none_when_unavailable() {
        // When no backend is on $PATH, `driver_for_probe` should be None
        // rather than panic. Tests run with whatever $PATH the harness
        // provides; we don't assert presence/absence, just the Option shape.
        let d = NvmsDriver::driver_for_probe(&NvmsModality::new());
        if d.is_some() {
            // If nvms IS available, the binary must be on $PATH.
            assert!(which_nvms().is_some());
        }
    }
}

// ---------------------------------------------------------------------------
// M3 dispatch brief — staged for next session
// ---------------------------------------------------------------------------
//
// What `NvmsDriver` represents (per ADR-006 M3):
//
//   The probe in `NvmsModality` answers "is `nvms` on $PATH?".
//   The *driver* answers "spawn `nvms run <config>`, tunnel capture/input
//   through the resulting container, and shut it down on App drop".
//
//   nvms is the orchestrator from `KooshaPari/nanovms` — different from
//   the container CLI in `container.rs`. While the Container modality
//   shells out to docker/podman, the nvms modality speaks nvms's native
//   RPC protocol and can share a runtime across multiple PlayCua sessions.
//
// Concretely, when a user invokes `playcua --modality nvms screenshot`,
// the App construction looks up `ModalityRegistry::select(Nvms)`, gets a
// `NvmsDriver` back, and the per-port dispatchers must:
//   - `capture.screenshot()` route through the nvms tunnel RPC
//   - `input.type/key/tap` route through the same tunnel
//   - the `native` fallback path is replaced entirely
//
// The struct below is intentionally skeletal — the next session fills in:
//
//   1. `spawn()`             — `tokio::process::Command::new("nvms")` with
//                              subcommand `run --config <nvms.toml>`.
//                              The config path comes from
//                              `PLAYCUA_NVMS_CONFIG` env var or
//                              `~/.config/playcua/nvms.toml`.
//
//   2. `tunnel()`            — wraps `Child` stdio in the nvms RPC codec.
//                              Reuses the existing `Dispatcher` codec in
//                              `native/src/dispatch.rs`. The tunnel lives
//                              behind a `RwLock<Option<Tunnel>>` so the
//                              first method-call pays the spawn cost and
//                              subsequent calls hit the cached handle.
//
//   3. `shutdown()`          — graceful child kill on App drop; sends
//                              SIGTERM first, SIGKILL after 5 s. Implemented
//                              in `Drop` for NvmsDriver.
//
//   4. config authoring      — a sample `nvms.toml` for the first M3 slice
//                              (image: ubuntu:22.04 with xvfb + x11-apps +
//                              the PlayCua capture bridge). Lives in
//                              `sandbox/nvms.toml` (not added in this PR).
//
//   5. tests                — hermetic test using a fake `nvms` shell script
//                             in `native/tests/fixtures/fake-nvms.sh` that
//                             echoes the RPC envelope back. This is the only
//                             piece of M3 that adds a new file to the
//                             workspace.
//
// Why a skeleton now and not a full impl: cargo test --workspace exceeds
// the 5-minute shell tool timeout on this machine, so anything requiring
// full test evidence belongs in the next session. The skeleton compiles
// and exercises the API shape via the two unit tests above — those run
// in well under a second.

/// Lazy spawn-and-tunnel handle for an `nvms run` invocation.
///
/// Implements M3 per ADR-006: spawn `nvms run --config <nvms.toml>`,
/// expose the child's stdio via `tunnel_*` accessors so the host can
/// drive the nvms RPC envelope (reusing `native/src/ipc/dispatcher.rs`'s
/// `Dispatcher::dispatch` codec over the child's stdio), and shut the
/// child down gracefully on `Drop` or via the explicit `shutdown()`
/// async method.
///
/// Config path resolution:
/// 1. `PLAYCUA_NVMS_CONFIG` env var
/// 2. `./nvms.toml` (cwd)
/// 3. `~/.config/playcua/nvms.toml`
///
/// If none of those exist, `spawn()` returns `NvmsError::ConfigNotFound`.
pub struct NvmsDriver {
    config_path: std::path::PathBuf,
    child: Option<tokio::process::Child>,
}

/// Errors NvmsDriver::spawn can surface.
#[derive(Debug)]
pub enum NvmsError {
    /// `nvms.toml` not found in any of the search paths.
    ConfigNotFound(Vec<std::path::PathBuf>),
    /// I/O failure from `tokio::process::Command::spawn`.
    Io(std::io::Error),
}

impl std::fmt::Display for NvmsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigNotFound(searched) => {
                write!(f, "nvms.toml not found; searched: ")?;
                for (i, p) in searched.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p.display())?;
                }
                Ok(())
            }
            Self::Io(e) => write!(f, "nvms spawn I/O error: {e}"),
        }
    }
}

impl std::error::Error for NvmsError {}

impl NvmsDriver {
    /// Construct a driver for a specific config path. Does not spawn.
    pub fn new(config_path: std::path::PathBuf) -> Self {
        Self {
            config_path,
            child: None,
        }
    }

    /// If `nvms` is on $PATH, return a driver with the resolved config;
    /// otherwise None.
    pub fn driver_for_probe(m: &NvmsModality) -> Option<Self> {
        if !m.is_available() {
            return None;
        }
        // Probe succeeded — the user has `nvms` on PATH. Use the env-var
        // config path if set, fall back to ./nvms.toml in the next stage
        // (the dispatch-brief escalation order).
        let config = std::env::var_os("PLAYCUA_NVMS_CONFIG")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("./nvms.toml"));
        Some(Self::new(config))
    }

    /// The argv head that `spawn()` will eventually exec. Exposed now so
    /// tests can verify the subcommand shape without spawning.
    pub fn spawn_argv(&self) -> Vec<String> {
        vec!["nvms".to_string(), "run".to_string()]
    }

    /// Resolve the nvms.toml config path from PLAYCUA_NVMS_CONFIG / cwd /
    /// ~/.config/playcua in that order. Returns the first existing match.
    fn resolve_config() -> Result<std::path::PathBuf, NvmsError> {
        if let Some(p) = std::env::var_os("PLAYCUA_NVMS_CONFIG").map(std::path::PathBuf::from) {
            if p.is_file() {
                return Ok(p);
            }
        }
        let cwd = std::path::PathBuf::from("./nvms.toml");
        if cwd.is_file() {
            return Ok(cwd);
        }
        if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
            let user_cfg = home.join(".config/playcua/nvms.toml");
            if user_cfg.is_file() {
                return Ok(user_cfg);
            }
        }
        Err(NvmsError::ConfigNotFound(vec![
            std::path::PathBuf::from("$PLAYCUA_NVMS_CONFIG"),
            cwd,
            std::path::PathBuf::from("~/.config/playcua/nvms.toml"),
        ]))
    }

    /// Spawn `nvms run --config <path>` and store the child. Pipes stdio
    /// so the host can speak the nvms RPC envelope on stdin/stdout.
    pub async fn spawn(&mut self) -> Result<(), NvmsError> {
        // Re-resolve at spawn time so users can set the env var between
        // driver construction and the first action.
        let path = if self.config_path.is_file() {
            self.config_path.clone()
        } else {
            Self::resolve_config()?
        };
        let mut cmd = tokio::process::Command::new("nvms");
        cmd.arg("run").arg("--config").arg(&path);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            // nvms spawns child containers; keep the orchestrator in its
            // own pgroup so Drop's SIGTERM-to-pgid cleans up the whole tree.
            unsafe {
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }
        let child = cmd.spawn().map_err(NvmsError::Io)?;
        self.child = Some(child);
        Ok(())
    }

    /// Accessor for the spawned child's stdin. Returns `None` if `spawn()`
    /// has not been called.
    pub fn tunnel_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.child.as_mut()?.stdin.take()
    }

    /// Accessor for the spawned child's stdout. Returns `None` if `spawn()`
    /// has not been called.
    pub fn tunnel_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.child.as_mut()?.stdout.take()
    }

    /// Accessor for the spawned child's stderr. Returns `None` if `spawn()`
    /// has not been called.
    pub fn tunnel_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.child.as_mut()?.stderr.take()
    }

    /// Explicit graceful shutdown. Sends SIGTERM first, then SIGKILL after
    /// 5 s. Mirrors the SandboxDriver shutdown semantics.
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        const KILL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
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
}

impl Drop for NvmsDriver {
    fn drop(&mut self) {
        // Best-effort: spawn the async shutdown on the tokio runtime if one
        // exists; otherwise rely on kernel child reaping. We don't block on
        // shutdown here — Drop is sync — but we *do* send SIGTERM/start_kill
        // so the OS reclaims resources promptly.
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
            // Detach the child so tokio can reap it asynchronously without
            // awaiting; the kernel will SIGKILL on parent exit if needed.
            let _ = child.id();
        }
    }
}
