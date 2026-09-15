//! Stage `eidolon-vsock-agent` (+ optional systemd unit) into a rootfs tree
//! before mkfs / virt-make-fs / DockerToExt4.
//!
//! Prefer [`AGENT_PATH_ENV`] (`EIDOLON_VSOCK_AGENT`) for production bake paths.
//! Cargo-target discovery is a test/dev convenience only — never a silent skip
//! when bake is requested ([`codes::SANDBOX_VSOCK_AGENT_MISSING`]).

use super::{pack_io, stage_artifact};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Env override for the host-built `eidolon-vsock-agent` binary to bake.
///
/// Explicit path on [`BakeAgentRequest::agent_bin`] / [`super::PackRequest::vsock_agent_bin`]
/// wins. When set-but-missing → fail-loud (no PATH fallback).
pub const AGENT_PATH_ENV: &str = "EIDOLON_VSOCK_AGENT";

/// Guest install path relative to the rootfs tree.
pub const DEFAULT_GUEST_AGENT_REL: &str = "usr/local/bin/eidolon-vsock-agent";

/// Guest systemd unit path relative to the rootfs tree.
pub const DEFAULT_GUEST_UNIT_REL: &str = "etc/systemd/system/eidolon-vsock-agent.service";

/// Binary file name expected under cargo `target/` when discovering for tests.
pub const AGENT_BIN_NAME: &str = "eidolon-vsock-agent";

/// Example unit from `docs/guides/vsock-guest-agent.md` (bake optional).
pub const DEFAULT_SYSTEMD_UNIT: &str = r#"[Unit]
Description=Eidolon vsock NDJSON agent
After=local-fs.target

[Service]
ExecStart=/usr/local/bin/eidolon-vsock-agent --any-cid --port 5252
Restart=on-failure

[Install]
WantedBy=multi-user.target
"#;

/// Options for staging the vsock agent into a rootfs directory tree.
#[derive(Debug, Clone)]
pub struct BakeAgentRequest {
    /// Host path to `eidolon-vsock-agent`. When `None`, resolve via
    /// [`resolve_vsock_agent_bin`] (`EIDOLON_VSOCK_AGENT` → cargo discovery).
    pub agent_bin: Option<PathBuf>,
    /// Path under the tree (default [`DEFAULT_GUEST_AGENT_REL`]).
    pub guest_agent_rel: PathBuf,
    /// When true, also write [`DEFAULT_SYSTEMD_UNIT`] under
    /// [`DEFAULT_GUEST_UNIT_REL`] (or [`Self::guest_unit_rel`]).
    pub bake_systemd_unit: bool,
    /// Override unit relative path (ignored when `bake_systemd_unit` is false).
    pub guest_unit_rel: PathBuf,
    /// Prefer hard-link when staging the binary; else copy.
    pub link_artifacts: bool,
}

impl Default for BakeAgentRequest {
    fn default() -> Self {
        Self {
            agent_bin: None,
            guest_agent_rel: PathBuf::from(DEFAULT_GUEST_AGENT_REL),
            bake_systemd_unit: false,
            guest_unit_rel: PathBuf::from(DEFAULT_GUEST_UNIT_REL),
            link_artifacts: false,
        }
    }
}

impl BakeAgentRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_agent_bin(mut self, path: impl Into<PathBuf>) -> Self {
        self.agent_bin = Some(path.into());
        self
    }

    pub fn with_systemd_unit(mut self, bake: bool) -> Self {
        self.bake_systemd_unit = bake;
        self
    }

    pub fn with_link(mut self, link: bool) -> Self {
        self.link_artifacts = link;
        self
    }
}

/// Paths written into the tree by [`stage_vsock_agent_into_tree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakeAgentResult {
    pub agent_host_source: PathBuf,
    pub agent_guest_path: PathBuf,
    pub unit_guest_path: Option<PathBuf>,
}

/// Resolve the host agent binary for bake.
///
/// Order: `explicit` → [`AGENT_PATH_ENV`] → cargo-target discovery (debug/release
/// under `CARGO_TARGET_DIR` / workspace `target/`). Missing → fail-loud
/// [`codes::SANDBOX_VSOCK_AGENT_MISSING`] (never silent skip).
pub fn resolve_vsock_agent_bin(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return require_agent_file(p, "explicit vsock_agent_bin");
    }

    if let Ok(override_path) = std::env::var(AGENT_PATH_ENV) {
        let trimmed = override_path.trim();
        if !trimmed.is_empty() {
            // Set-but-missing: fail-loud — no cargo/PATH fallthrough.
            return require_agent_file(Path::new(trimmed), &format!("{AGENT_PATH_ENV} override"));
        }
    }

    if let Some(discovered) = discover_cargo_agent_bin() {
        return Ok(discovered);
    }

    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_VSOCK_AGENT_MISSING,
        format!(
            "eidolon-vsock-agent missing for rootfs bake — set {AGENT_PATH_ENV} to a built \
             binary, pass PackRequest::vsock_agent_bin / BakeAgentRequest::agent_bin, or build \
             with: cargo build -p eidolon-sandbox --features sandbox-vsock-agent \
             --bin eidolon-vsock-agent --locked (docs/guides/vsock-guest-agent.md); \
             fail-loud — no silent skip"
        ),
    ))
}

/// Stage the agent (+ optional systemd unit) into `rootfs_tree`.
///
/// `rootfs_tree` must be an existing directory. Creates parent dirs as needed.
/// Sets mode `0755` on the staged binary (Unix).
pub fn stage_vsock_agent_into_tree(
    rootfs_tree: &Path,
    req: &BakeAgentRequest,
) -> Result<BakeAgentResult> {
    if !rootfs_tree.is_dir() {
        return Err(PhenoError::BadRequest(format!(
            "bake vsock agent requires rootfs_tree to be a directory; got {}",
            rootfs_tree.display()
        )));
    }

    let agent_src = resolve_vsock_agent_bin(req.agent_bin.as_deref())?;
    let guest_rel = if req.guest_agent_rel.as_os_str().is_empty() {
        PathBuf::from(DEFAULT_GUEST_AGENT_REL)
    } else {
        req.guest_agent_rel.clone()
    };
    let dest = rootfs_tree.join(&guest_rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| pack_io(parent, e))?;
    }
    stage_artifact(&agent_src, &dest, req.link_artifacts)?;
    set_executable(&dest)?;

    let unit_guest_path = if req.bake_systemd_unit {
        let unit_rel = if req.guest_unit_rel.as_os_str().is_empty() {
            PathBuf::from(DEFAULT_GUEST_UNIT_REL)
        } else {
            req.guest_unit_rel.clone()
        };
        let unit_dest = rootfs_tree.join(&unit_rel);
        if let Some(parent) = unit_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| pack_io(parent, e))?;
        }
        fs::write(&unit_dest, DEFAULT_SYSTEMD_UNIT.as_bytes()).map_err(|e| pack_io(&unit_dest, e))?;
        Some(unit_dest)
    } else {
        None
    };

    Ok(BakeAgentResult {
        agent_host_source: agent_src,
        agent_guest_path: dest,
        unit_guest_path,
    })
}

fn require_agent_file(path: &Path, kind: &str) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_VSOCK_AGENT_MISSING,
        format!(
            "eidolon-vsock-agent missing ({kind}) at {} — fail-loud; no silent bake skip \
             (docs/guides/vsock-guest-agent.md)",
            path.display()
        ),
    ))
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).map_err(|e| pack_io(path, e))?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(|e| pack_io(path, e))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Best-effort cargo target discovery for tests / local bake without env.
///
/// Checks `CARGO_TARGET_DIR`, then `CARGO_MANIFEST_DIR/../../target`, then
/// cwd/`target` for `debug`/`release`/`<triple>/{debug,release}` layouts.
fn discover_cargo_agent_bin() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
        let t = td.trim();
        if !t.is_empty() {
            roots.push(PathBuf::from(t));
        }
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        // crates/eidolon-sandbox → workspace target/
        let crate_dir = PathBuf::from(manifest);
        if let Some(ws) = crate_dir.parent().and_then(|p| p.parent()) {
            roots.push(ws.join("target"));
        }
        roots.push(crate_dir.join("target"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.join("target"));
    }

    let profiles = ["debug", "release"];
    for root in &roots {
        for profile in &profiles {
            let direct = root.join(profile).join(AGENT_BIN_NAME);
            if direct.is_file() {
                return Some(direct);
            }
        }
        // target/<triple>/{debug,release}/eidolon-vsock-agent
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name == "debug" || name == "release" || name == "tmp" || name.starts_with('.') {
                    continue;
                }
                for profile in &profiles {
                    let candidate = path.join(profile).join(AGENT_BIN_NAME);
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "eidolon-bake-agent-{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    fn fake_agent(dir: &Path) -> PathBuf {
        let p = dir.join("fake-eidolon-vsock-agent");
        let mut f = fs::File::create(&p).unwrap();
        writeln!(f, "#!/bin/sh\necho fake-agent").unwrap();
        drop(f);
        set_executable(&p).unwrap();
        p
    }

    #[test]
    fn stage_agent_into_tree_copies_binary() {
        let host = temp_dir("host");
        let tree = temp_dir("tree");
        let agent = fake_agent(&host);
        let result = stage_vsock_agent_into_tree(
            &tree,
            &BakeAgentRequest::new().with_agent_bin(&agent),
        )
        .expect("stage");
        assert!(result.agent_guest_path.is_file());
        assert!(result.agent_guest_path.ends_with(DEFAULT_GUEST_AGENT_REL));
        assert!(result.unit_guest_path.is_none());
        let body = fs::read_to_string(&result.agent_guest_path).unwrap();
        assert!(body.contains("fake-agent"));
        let _ = fs::remove_dir_all(&host);
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn stage_agent_with_systemd_unit() {
        let host = temp_dir("host-unit");
        let tree = temp_dir("tree-unit");
        let agent = fake_agent(&host);
        let result = stage_vsock_agent_into_tree(
            &tree,
            &BakeAgentRequest::new()
                .with_agent_bin(&agent)
                .with_systemd_unit(true),
        )
        .expect("stage+unit");
        let unit = result.unit_guest_path.expect("unit");
        assert!(unit.is_file());
        let text = fs::read_to_string(&unit).unwrap();
        assert!(text.contains("eidolon-vsock-agent --any-cid --port 5252"));
        let _ = fs::remove_dir_all(&host);
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn missing_agent_fail_loud() {
        let tree = temp_dir("missing");
        let err = stage_vsock_agent_into_tree(
            &tree,
            &BakeAgentRequest::new().with_agent_bin("/no/such/eidolon-vsock-agent"),
        )
        .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_VSOCK_AGENT_MISSING)
        );
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn env_override_set_but_missing_fail_loud() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(AGENT_PATH_ENV, "/no/such/eidolon-vsock-agent-env");
        let err = resolve_vsock_agent_bin(None).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_VSOCK_AGENT_MISSING)
        );
        std::env::remove_var(AGENT_PATH_ENV);
    }

    #[test]
    fn env_override_resolves_when_present() {
        let _g = ENV_LOCK.lock().unwrap();
        let host = temp_dir("env-host");
        let agent = fake_agent(&host);
        std::env::set_var(AGENT_PATH_ENV, &agent);
        let resolved = resolve_vsock_agent_bin(None).expect("env resolve");
        assert_eq!(resolved, agent);
        std::env::remove_var(AGENT_PATH_ENV);
        let _ = fs::remove_dir_all(&host);
    }

    #[test]
    fn non_directory_tree_bad_request() {
        let host = temp_dir("nd");
        let file = host.join("not-a-dir");
        fs::write(&file, b"x").unwrap();
        let agent = fake_agent(&host);
        let err = stage_vsock_agent_into_tree(
            &file,
            &BakeAgentRequest::new().with_agent_bin(&agent),
        )
        .unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
        let _ = fs::remove_dir_all(&host);
    }
}
