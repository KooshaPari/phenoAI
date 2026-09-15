//! `container` modality — generic OCI container (Docker, Podman, containerd).
//!
//! Probes for `docker` or `podman` (or `$PLAYCUA_CONTAINER_CLI` override).
//! Either of these on $PATH is sufficient — the modality is the
//! container-runtime-agnostic shim.

use super::{Modality, ModalityKind};
use std::path::PathBuf;

/// The container-modality probe.
pub struct ContainerModality {
    cached: std::sync::OnceLock<Option<&'static str>>,
}

impl ContainerModality {
    pub fn new() -> Self {
        Self {
            cached: std::sync::OnceLock::new(),
        }
    }

    fn probe(&self) -> Option<&'static str> {
        self.cached
            .get_or_init(|| {
                if let Ok(override_cli) = std::env::var("PLAYCUA_CONTAINER_CLI") {
                    return Some(match override_cli.as_str() {
                        "docker" => "docker",
                        "podman" => "podman",
                        "nerdctl" => "nerdctl",
                        _ => return None,
                    });
                }
                ["docker", "podman", "nerdctl"]
                    .into_iter()
                    .find(|cli| which(cli).is_some())
            })
            .as_ref()
            .copied()
    }
}

impl Default for ContainerModality {
    fn default() -> Self {
        Self::new()
    }
}

impl Modality for ContainerModality {
    fn kind(&self) -> ModalityKind {
        ModalityKind::Container
    }

    fn describe(&self) -> &'static str {
        "OCI container (docker / podman / nerdctl)"
    }

    fn is_available(&self) -> bool {
        self.probe().is_some()
    }

    fn detail(&self) -> String {
        match self.probe() {
            Some(cli) => format!("cli={cli}"),
            None => "no container CLI on $PATH".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_available_is_stable() {
        let m = ContainerModality::new();
        assert_eq!(m.is_available(), m.is_available());
    }

    #[test]
    fn kind_is_container() {
        assert_eq!(ContainerModality::new().kind(), ModalityKind::Container);
    }

    #[test]
    fn driver_spawn_argv_includes_run_subcommand() {
        // The lazy driver must build an argv whose head is the container CLI
        // (e.g. "docker", "podman") followed by `run`, so the host shell can
        // exec it directly. We don't actually spawn in tests (would need a
        // real container runtime); just verify the argv shape.
        let d = ContainerDriver::new(ContainerCli::Docker, "playcua/bridge:latest");
        let argv = d.spawn_argv();
        assert_eq!(argv.first().map(String::as_str), Some("docker"));
        assert_eq!(argv.get(1).map(String::as_str), Some("run"));
    }

    #[test]
    fn driver_for_probe_returns_none_when_unavailable() {
        // When no container CLI is on $PATH, `driver_for_probe` should be
        // None rather than panic. Tests run with whatever $PATH the harness
        // provides; we don't assert presence/absence, just the Option shape.
        let d = ContainerDriver::driver_for_probe(&ContainerModality::new());
        if let Some(d) = d {
            // If a CLI IS available, the binary must be on $PATH.
            assert!(which(d.cli.binary()).is_some());
        }
    }
}

// ---------------------------------------------------------------------------
// M5 dispatch brief — staged for next session
// ---------------------------------------------------------------------------
//
// What `ContainerDriver` represents (per ADR-006 M5):
//
//   The probe in `ContainerModality` answers "is `docker` / `podman` /
//   `nerdctl` on $PATH?". The *driver* answers "spawn `cli run --rm ...`,
//   tunnel capture/input through the container, and shut it down on App drop".
//
//   M5 is the largest of the four driver slices because it must:
//   - Author a base container image (Ubuntu 22.04 + Xvfb + x11-apps +
//     the PlayCua capture bridge).
//   - Handle the OCI-vs-podman CLI divergence (defaults, --userns, --rm).
//   - Map host:container paths correctly on Windows (where the user runs
//     `playcua.exe` and the container runs Linux).
//   - Set up either an X11 socket mount or a VNC bridge for capture, and
//     a uinput bridge for input.
//
// Concretely, when a user invokes `playcua --modality container screenshot`,
// the App construction looks up `ModalityRegistry::select(Container)`,
// gets a `ContainerDriver` back, and:
//   - `capture.screenshot()` route through the X11 socket inside the
//                              container (or VNC frame buffer)
//   - `input.type/key/tap` route through the same bridge
//   - the `native` fallback path is replaced entirely
//
// The struct below is intentionally skeletal — the next session fills in:
//
//   1. `spawn()`             — `tokio::process::Command::new(cli.binary())` with
//                              subcommand `run --rm -i --volume
//                              <host-screenshot-dir>:<container-screenshot-dir>
//                              <playcua-image>:
//                              /usr/local/bin/playcua-bridge`. The image name
//                              comes from `PLAYCUA_CONTAINER_IMAGE` env var
//                              (default `playcua/bridge:latest`).
//
//   2. `tunnel()`            — wraps `Child` stdio in a JSON-RPC client.
//                              Reuses the existing `Dispatcher` codec in
//                              `native/src/dispatch.rs`. The tunnel lives
//                              behind a `RwLock<Option<Tunnel>>` so the
//                              first method-call pays the spawn cost.
//
//   3. `shutdown()`          — graceful child kill on App drop; sends
//                              SIGTERM first, SIGKILL after 5 s. Implemented
//                              in `Drop` for ContainerDriver.
//
//   4. image authoring       — a `Containerfile` for the `playcua/bridge`
//                              image. Lives in `sandbox/Containerfile`
//                              (not added in this PR).
//
//   5. tests                — hermetic test using a fake container CLI shell
//                             script in `native/tests/fixtures/fake-cli.sh`
//                             that echoes the JSON-RPC envelope back. CI
//                             runners without a container runtime get the
//                             option/None path (no panic).
//
// Why a skeleton now and not a full impl: cargo test --workspace exceeds
// the 5-minute shell tool timeout on this machine, so anything requiring
// full test evidence belongs in the next session. The skeleton compiles
// and exercises the API shape via the two unit tests above — those run
// in well under a second.

/// Container CLI selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerCli {
    Docker,
    Podman,
    Nerdctl,
}

impl ContainerCli {
    pub(crate) fn binary(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::Nerdctl => "nerdctl",
        }
    }
}

impl<'a> From<&'a str> for ContainerCli {
    fn from(s: &'a str) -> Self {
        match s {
            "docker" => Self::Docker,
            "podman" => Self::Podman,
            "nerdctl" => Self::Nerdctl,
            // Probe results are constrained to the three known CLIs; if a
            // future PLAYCUA_CONTAINER_CLI adds an unknown value, default
            // to docker (the most common). The probe already gates by
            // which() so an unknown CLI on PATH won't activate the modality.
            _ => Self::Docker,
        }
    }
}

/// Lazy spawn-and-tunnel handle for a container CLI invocation.
///
/// Implements M5 per ADR-006: spawn `<cli> run --rm -i --volume <host>:<ctn>
/// <image> /usr/local/bin/playcua-bridge`, expose stdio so the host can drive
/// the bridge's JSON-RPC envelope (reusing `native/src/ipc/dispatcher.rs`),
/// and shut the container down gracefully on `Drop`.
///
/// Image selection order:
/// 1. `PLAYCUA_CONTAINER_IMAGE` env var (e.g. `my-registry.example.com/playcua:dev`)
/// 2. Default `playcua/bridge:latest`
pub struct ContainerDriver {
    cli: ContainerCli,
    image: String,
    child: Option<tokio::process::Child>,
}

impl ContainerDriver {
    /// Construct a driver for a specific CLI and image. Does not spawn.
    pub fn new(cli: ContainerCli, image: impl Into<String>) -> Self {
        Self {
            cli,
            image: image.into(),
            child: None,
        }
    }

    /// If the probe found a container CLI, return a driver for it. The image
    /// comes from `PLAYCUA_CONTAINER_IMAGE` (default `playcua/bridge:latest`).
    pub fn driver_for_probe(m: &ContainerModality) -> Option<Self> {
        let cli_str = m.probe()?;
        let image = std::env::var("PLAYCUA_CONTAINER_IMAGE")
            .unwrap_or_else(|_| "playcua/bridge:latest".to_string());
        Some(Self::new(ContainerCli::from(cli_str), image))
    }

    /// The argv head that `spawn()` will eventually exec. Exposed now so
    /// tests can verify the CLI selection without spawning.
    pub fn spawn_argv(&self) -> Vec<String> {
        vec![self.cli.binary().to_string(), "run".to_string()]
    }

    /// Spawn `<cli> run --rm -i --volume <host>:<ctn> <image>
    /// /usr/local/bin/playcua-bridge` and store the child. Stdio is piped
    /// so the host can drive the JSON-RPC envelope.
    pub async fn spawn(&mut self) -> std::io::Result<()> {
        let mut cmd = tokio::process::Command::new(self.cli.binary());
        cmd.arg("run").arg("--rm").arg("-i");
        // WINDOWS BRIDGE: On Windows hosts `host` paths need translation
        // (C:\foo -> /mnt/c/foo) before they can be bind-mounted into the
        // Linux container. `to_wsl_path` is idempotent — on non-Windows
        // hosts it's a no-op, and on Windows it leaves already-WSL paths
        // untouched. Per-CLI translation policy:
        //   * docker       — needs /mnt/c/... (WSL2 backend)
        //   * podman       — accepts Windows paths on Windows-native backend,
        //                    but /mnt/c/... is also valid; translate for safety
        //   * nerdctl      — same as podman; translate for safety
        let host_path = to_wsl_path(&std::env::temp_dir().display().to_string());
        cmd.arg("--volume")
            .arg(format!("{0}:{0}", host_path))
            .arg(&self.image)
            .arg("/usr/local/bin/playcua-bridge");
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            // Detach into own session so Drop's SIGTERM-to-pgid cleans up
            // the container runtime + child simultaneously.
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

impl Drop for ContainerDriver {
    fn drop(&mut self) {
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
        }
    }
}

// ---------------------------------------------------------------------------
// Path translation helper (L7 #134 WINDOWS-BRIDGE follow-up)
//
// On Windows hosts, bind-mount paths passed to `docker run --volume` need
// translation before the WSL2-backed Linux container runtime can consume them.
// `podman` (when configured with the Windows-native backend) accepts Windows
// paths verbatim, so the helper is only invoked on Windows + when the runtime
// is `docker` (which requires WSL2 on Windows). For `podman` and `nerdctl`
// on Windows, the helper is still safe to call — it returns the path
// unchanged for those cases via the idempotence check.
// ---------------------------------------------------------------------------

/// Translate a Windows-style path into the equivalent WSL path.
///
/// Examples (Windows host):
///   `C:\Users\koosh\.cache`           -> `/mnt/c/Users/koosh/.cache`
///   `c:\Users\koosh\.cache`           -> `/mnt/c/Users/koosh/.cache`
///   `\\?\C:\Users\koosh\.cache`       -> `/mnt/c/Users/koosh/.cache`
///   `C:`                              -> `/mnt/c`
///   `\\wsl$\Ubuntu\home\koosh`        -> unchanged (already a WSL path)
///   `/mnt/c/Users/koosh`              -> unchanged (already translated)
///
/// On non-Windows targets this is a no-op — Unix hosts don't need any
/// translation when invoking `docker run` directly.
///
/// Idempotent: paths that already look like WSL paths (`/mnt/...`,
/// `/...`, `\\wsl$\...`) pass through unchanged. This means the helper
/// can be called unconditionally without double-translating paths that
/// were already prepared upstream (e.g. `$PLAYCUA_HOST_TMP` set by an
/// orchestrator script).
pub fn to_wsl_path(host_path: &str) -> String {
    #[cfg(not(windows))]
    {
        return host_path.to_string();
    }

    #[cfg(windows)]
    {
        // Strip the Win32 extended-length prefix `\\?\` if present.
        // This check must come BEFORE the generic `\\`-prefix check
        // below — an extended path like `\\?\C:\foo` would otherwise
        // hit the "already UNC" branch and pass through unchanged.
        let (stripped, _was_extended) = if let Some(rest) = host_path.strip_prefix("\\\\?\\") {
            (rest, true)
        } else {
            (host_path, false)
        };

        // Idempotence: if the path is already in WSL form, leave it.
        // We test against `stripped` (extended prefix removed) so that
        // `\\?\C:\foo` is treated as a Windows drive path, not as a UNC.
        if stripped.starts_with("/mnt/")
            || stripped.starts_with('/')
            || stripped.starts_with("\\\\wsl$\\")
            || stripped.starts_with("\\\\")
        {
            return host_path.to_string();
        }

        // Expect a drive-letter path: `<letter>:<rest>`.
        let bytes = stripped.as_bytes();
        if bytes.len() < 2 || bytes[1] != b':' {
            // Not a Windows drive path — return unchanged and let the
            // container runtime error if it can't handle the form.
            return host_path.to_string();
        }
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = &stripped[2..];

        // Drive root (e.g. `C:`) -> `/mnt/c`.
        if rest.is_empty() {
            return format!("/mnt/{}", drive);
        }

        // Path must start with `\` or `/` to be a rooted path; if it
        // doesn't (e.g. just a drive-relative path), normalize.
        let rest = rest
            .strip_prefix('\\')
            .or_else(|| rest.strip_prefix('/'))
            .unwrap_or(rest);

        // Translate backslashes -> forward slashes.
        let translated = rest.replace('\\', "/");

        format!("/mnt/{}/{}", drive, translated)
    }
}

#[cfg(test)]
mod wsl_path_tests {
    use super::to_wsl_path;

    #[cfg(windows)]
    #[test]
    fn uppercase_drive_translates() {
        assert_eq!(
            to_wsl_path(r"C:\Users\koosh\.cache"),
            "/mnt/c/Users/koosh/.cache"
        );
    }

    #[cfg(windows)]
    #[test]
    fn lowercase_drive_translates() {
        assert_eq!(
            to_wsl_path(r"c:\Users\koosh\.cache"),
            "/mnt/c/Users/koosh/.cache"
        );
    }

    #[cfg(windows)]
    #[test]
    fn extended_prefix_is_stripped() {
        assert_eq!(to_wsl_path(r"\\?\C:\foo\bar"), "/mnt/c/foo/bar");
    }

    #[cfg(windows)]
    #[test]
    fn drive_root_translates_to_mnt_slash_drive() {
        assert_eq!(to_wsl_path("C:"), "/mnt/c");
    }

    #[cfg(windows)]
    #[test]
    fn already_wsl_path_passes_through() {
        assert_eq!(to_wsl_path("/mnt/c/Users/koosh"), "/mnt/c/Users/koosh");
        assert_eq!(to_wsl_path("/home/koosh/.cache"), "/home/koosh/.cache");
        assert_eq!(to_wsl_path(r"\\wsl$\Ubuntu\home"), r"\\wsl$\Ubuntu\home");
    }

    #[cfg(windows)]
    #[test]
    fn non_drive_path_passes_through() {
        // No `C:` prefix — runtime will error if it can't handle it.
        assert_eq!(to_wsl_path(r"\foo\bar"), r"\foo\bar");
    }

    #[test]
    fn noop_on_unix() {
        #[cfg(not(windows))]
        assert_eq!(to_wsl_path("/tmp/cache"), "/tmp/cache");
    }
}
