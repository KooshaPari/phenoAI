//! Canned CI/release rootfs disk with `eidolon-vsock-agent` preinstalled.
//!
//! Wraps [`super::bake_agent_then_pack`] / [`super::stage_vsock_agent_into_tree`]
//! + [`super::pack_rootfs_tree_to_ext4`] / [`super::compose_docker_to_ext4`] —
//! does **not** reimplement mkfs/docker.
//!
//! # Honesty
//!
//! - **Pipeline shipped**: seed tree → bake agent → (optional) Ext4 pack into a
//!   durable output dir ([`CANNED_ROOTFS_OUT_ENV`] / `target/canned-rootfs/`).
//! - **GH release Ext4 staging + pin/fetch**: see [`super::release_asset`] /
//!   `docs/guides/gh-ext4-release.md` (default pin may be unpublished).
//! - Hermetic tests use a stub tree + fake agent binary.
//! - Missing Linux agent bin → fail-loud [`codes::SANDBOX_VSOCK_AGENT_MISSING`]
//!   (never invent a silent empty bake).
//! - Optional pin/fetch: [`AGENT_URL_ENV`] + [`AGENT_SHA256_ENV`] (curl + SHA-256),
//!   same shape as UIA2 APK cache — only when both envs are set.
//!
//! See `docs/guides/canned-rootfs.md` and `docs/reference/rootfs-pack.md`.

pub(crate) mod extract;

use std::fs;
use std::path::{Path, PathBuf};

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use extract::{build_docker_to_ext4, build_pack_ext4, build_tree_only};
pub use extract::{ensure_vsock_agent_bin, materialize_base_tree, seed_minimal_rootfs_tree};

use super::bake_agent::BakeAgentResult;
use super::{pack_io, DiskImageBackend, PackageManifest};

/// Durable canned-rootfs output directory override (`EIDOLON_CANNED_ROOTFS_OUT`).
///
/// When unset, defaults to workspace-relative [`DEFAULT_CANNED_OUT_REL`]
/// (`target/canned-rootfs`). Paths under `/tmp` are rejected.
pub const CANNED_ROOTFS_OUT_ENV: &str = "EIDOLON_CANNED_ROOTFS_OUT";

/// Default output directory relative to the workspace (or cwd) root.
pub const DEFAULT_CANNED_OUT_REL: &str = "target/canned-rootfs";

/// Documented alternate durable layout under the worktree.
pub const ALT_CANNED_OUT_REL: &str = "artifacts/rootfs";

/// Filename for the Ext4 disk image written by live pack.
pub const CANNED_ROOTFS_IMG: &str = "rootfs.img";

/// Directory name for the baked guest tree under the out dir.
pub const CANNED_TREE_DIR: &str = "rootfs-tree";

/// Manifest filename for tree-only (hermetic) canned builds.
pub const CANNED_TREE_MANIFEST: &str = "eidolon-canned-rootfs-manifest.json";

/// Optional HTTPS URL of a pinned Linux `eidolon-vsock-agent` release binary.
///
/// Used with [`AGENT_SHA256_ENV`]. Both must be set to fetch; neither is a
/// silent default — there is **no** published GH disk asset required yet.
pub const AGENT_URL_ENV: &str = "EIDOLON_VSOCK_AGENT_URL";

/// Expected lowercase hex SHA-256 for [`AGENT_URL_ENV`] download.
pub const AGENT_SHA256_ENV: &str = "EIDOLON_VSOCK_AGENT_SHA256";

/// Durable cache for fetched agent binaries (`EIDOLON_VSOCK_AGENT_CACHE`).
///
/// Default: `$HOME/.cache/eidolon/vsock-agent/`. Rejects `/tmp`.
pub const AGENT_CACHE_ENV: &str = "EIDOLON_VSOCK_AGENT_CACHE";

/// How the canned pipeline produces artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CannedMode {
    /// Seed (or use) a rootfs tree, bake agent, write tree + JSON manifest.
    /// No mkfs/docker — safe on macOS CI. Does **not** claim Ext4.
    TreeOnly,
    /// Bake + pack tree → Ext4 via mkfs/virt-make-fs (needs
    /// `sandbox-rootfs-pack` + `ROOTFS_PACK_INTEGRATION=1` + tools).
    PackExt4,
    /// Bake during Docker → Ext4 compose (same live gates).
    DockerToExt4,
}

impl CannedMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TreeOnly => "tree_only",
            Self::PackExt4 => "pack_ext4",
            Self::DockerToExt4 => "docker_to_ext4",
        }
    }
}

/// Inputs for [`build_canned_rootfs`].
#[derive(Debug, Clone)]
pub struct CannedRootfsRequest {
    /// Explicit out dir (else [`CANNED_ROOTFS_OUT_ENV`] / default).
    pub out_dir: Option<PathBuf>,
    /// Existing rootfs-tree (directory). When `None`, seed a minimal stub tree
    /// under `out_dir/rootfs-tree` (hermetic fixture).
    pub base_tree: Option<PathBuf>,
    /// Docker image/container ref when [`CannedMode::DockerToExt4`].
    pub docker_ref: Option<String>,
    /// Explicit host agent binary (else resolve / optional pin fetch).
    pub agent_bin: Option<PathBuf>,
    /// When true, attempt [`ensure_vsock_agent_bin`] (env / cargo / pin fetch).
    pub allow_agent_fetch: bool,
    pub bake_systemd_unit: bool,
    pub mode: CannedMode,
    pub disk_backend: DiskImageBackend,
    pub image_size_bytes: Option<u64>,
    pub compute_checksum: bool,
    pub link_artifacts: bool,
}

impl Default for CannedRootfsRequest {
    fn default() -> Self {
        Self {
            out_dir: None,
            base_tree: None,
            docker_ref: None,
            agent_bin: None,
            allow_agent_fetch: false,
            bake_systemd_unit: true,
            mode: CannedMode::TreeOnly,
            disk_backend: DiskImageBackend::MkfsExt4,
            image_size_bytes: None,
            compute_checksum: false,
            link_artifacts: false,
        }
    }
}

impl CannedRootfsRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_out_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.out_dir = Some(dir.into());
        self
    }

    pub fn with_base_tree(mut self, tree: impl Into<PathBuf>) -> Self {
        self.base_tree = Some(tree.into());
        self
    }

    pub fn with_docker_ref(mut self, reference: impl Into<String>) -> Self {
        self.docker_ref = Some(reference.into());
        self
    }

    pub fn with_agent_bin(mut self, path: impl Into<PathBuf>) -> Self {
        self.agent_bin = Some(path.into());
        self
    }

    pub fn with_allow_agent_fetch(mut self, allow: bool) -> Self {
        self.allow_agent_fetch = allow;
        self
    }

    pub fn with_systemd_unit(mut self, bake: bool) -> Self {
        self.bake_systemd_unit = bake;
        self
    }

    pub fn with_mode(mut self, mode: CannedMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_disk_backend(mut self, backend: DiskImageBackend) -> Self {
        self.disk_backend = backend;
        self
    }

    pub fn with_image_size(mut self, bytes: u64) -> Self {
        self.image_size_bytes = Some(bytes);
        self
    }

    pub fn with_checksum(mut self, compute: bool) -> Self {
        self.compute_checksum = compute;
        self
    }
}

/// Result of a successful canned build.
#[derive(Debug, Clone)]
pub struct CannedRootfsResult {
    pub mode: CannedMode,
    pub out_dir: PathBuf,
    /// Baked guest tree path (always present for tree / pack_ext4).
    pub rootfs_tree: PathBuf,
    /// Ext4 image path when live pack succeeded; `None` for [`CannedMode::TreeOnly`].
    pub rootfs_img: Option<PathBuf>,
    pub bake: BakeAgentResult,
    pub manifest: PackageManifest,
}

/// Reject durable roots under `/tmp` (agent-infra durability).
pub fn guard_against_tmp(path: &Path) -> Result<()> {
    let raw = path.to_string_lossy();
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("/tmp")
        || lower.starts_with("/private/tmp")
        || lower.contains("/tmp/")
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::Normal(s) if s == "tmp"))
    {
        return Err(PhenoError::BadRequest(format!(
            "canned rootfs refuses path under /tmp ({raw}) — set {CANNED_ROOTFS_OUT_ENV} \
             to a durable directory (e.g. target/canned-rootfs or artifacts/rootfs); \
             docs/guides/canned-rootfs.md"
        )));
    }
    Ok(())
}

/// Resolve the durable output directory.
///
/// Order: `explicit` → [`CANNED_ROOTFS_OUT_ENV`] → workspace/`target/canned-rootfs`
/// (via `CARGO_MANIFEST_DIR` / cwd). Always creates the directory. Rejects `/tmp`.
pub fn resolve_canned_out_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    let dir = if let Some(p) = explicit {
        p.to_path_buf()
    } else if let Ok(override_path) = std::env::var(CANNED_ROOTFS_OUT_ENV) {
        let trimmed = override_path.trim();
        if trimmed.is_empty() {
            return Err(PhenoError::BadRequest(format!(
                "{CANNED_ROOTFS_OUT_ENV} is set but empty — unset or provide a durable path"
            )));
        }
        PathBuf::from(trimmed)
    } else {
        default_canned_out_dir()
    };
    guard_against_tmp(&dir)?;
    fs::create_dir_all(&dir).map_err(|e| pack_io(&dir, e))?;
    Ok(dir)
}

fn default_canned_out_dir() -> PathBuf {
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let crate_dir = PathBuf::from(manifest);
        if let Some(ws) = crate_dir.parent().and_then(|p| p.parent()) {
            return ws.join(DEFAULT_CANNED_OUT_REL);
        }
    }
    PathBuf::from(DEFAULT_CANNED_OUT_REL)
}

/// Checkout fixture directory (`…/assets/canned-rootfs/minimal-tree`).
pub fn checkout_minimal_tree_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("canned-rootfs")
        .join("minimal-tree")
}

/// Build a canned rootfs artifact (tree-only or live Ext4).
///
/// Tree-only is always available. Live Ext4 modes require
/// `sandbox-rootfs-pack` + `ROOTFS_PACK_INTEGRATION=1` + host tools, and a
/// resolvable agent binary.
pub fn build_canned_rootfs(req: &CannedRootfsRequest) -> Result<CannedRootfsResult> {
    let out_dir = resolve_canned_out_dir(req.out_dir.as_deref())?;
    let agent = ensure_vsock_agent_bin(req.agent_bin.as_deref(), req.allow_agent_fetch)?;

    match req.mode {
        CannedMode::TreeOnly => build_tree_only(req, &out_dir, &agent),
        CannedMode::PackExt4 => build_pack_ext4(req, &out_dir, &agent),
        CannedMode::DockerToExt4 => build_docker_to_ext4(req, &out_dir, &agent),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Mutex;

    use super::super::bake_agent::DEFAULT_GUEST_AGENT_REL;
    use super::*;
    use crate::codes;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let root = if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
            PathBuf::from(td)
        } else if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
            PathBuf::from(m)
                .parent()
                .and_then(|p| p.parent())
                .unwrap_or(Path::new("."))
                .join("target")
        } else {
            PathBuf::from("target")
        };
        let path = root.join("canned-rootfs-test").join(format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("mkdir");
        path
    }

    pub(crate) fn fake_agent(dir: &Path) -> PathBuf {
        let p = dir.join("fake-eidolon-vsock-agent");
        let mut f = fs::File::create(&p).unwrap();
        writeln!(f, "#!/bin/sh\necho fake-canned-agent").unwrap();
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).unwrap();
        }
        p
    }

    #[test]
    fn guard_rejects_tmp() {
        let err = guard_against_tmp(Path::new("/tmp/eidolon-canned")).unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn tree_only_bakes_agent_into_stub() {
        let out = temp_dir("tree-only");
        let host = temp_dir("host-agent");
        let agent = fake_agent(&host);
        let result = build_canned_rootfs(
            &CannedRootfsRequest::new()
                .with_out_dir(&out)
                .with_agent_bin(&agent)
                .with_mode(CannedMode::TreeOnly)
                .with_systemd_unit(true),
        )
        .expect("tree_only");
        assert_eq!(result.mode, CannedMode::TreeOnly);
        assert!(result.rootfs_img.is_none());
        assert!(result.bake.agent_guest_path.is_file());
        assert!(result
            .bake
            .agent_guest_path
            .ends_with(DEFAULT_GUEST_AGENT_REL));
        let unit = result.bake.unit_guest_path.expect("unit");
        assert!(unit.is_file());
        assert!(out.join(CANNED_TREE_MANIFEST).is_file());
        assert_eq!(result.manifest.rootfs_format, "raw");
        let body = fs::read_to_string(&result.bake.agent_guest_path).unwrap();
        assert!(body.contains("fake-canned-agent"));
        let _ = fs::remove_dir_all(&out);
        let _ = fs::remove_dir_all(&host);
    }

    #[test]
    fn missing_agent_fail_loud() {
        let out = temp_dir("missing-agent");
        let err = build_canned_rootfs(
            &CannedRootfsRequest::new()
                .with_out_dir(&out)
                .with_agent_bin("/no/such/canned-agent")
                .with_mode(CannedMode::TreeOnly),
        )
        .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_VSOCK_AGENT_MISSING)
        );
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn env_out_dir_override() {
        let _g = ENV_LOCK.lock().unwrap();
        let out = temp_dir("env-out");
        std::env::set_var(CANNED_ROOTFS_OUT_ENV, &out);
        let resolved = resolve_canned_out_dir(None).expect("resolve");
        assert_eq!(resolved, out);
        std::env::remove_var(CANNED_ROOTFS_OUT_ENV);
        let _ = fs::remove_dir_all(&out);
    }

    #[test]
    fn pack_ext4_without_integration_fail_loud() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(super::super::PACK_INTEGRATION_ENV);
        let out = temp_dir("pack-gated");
        let host = temp_dir("pack-host");
        let agent = fake_agent(&host);
        let err = build_canned_rootfs(
            &CannedRootfsRequest::new()
                .with_out_dir(&out)
                .with_agent_bin(&agent)
                .with_mode(CannedMode::PackExt4),
        )
        .unwrap_err();
        assert!(
            err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_STUB)
                || err.unsupported_code() == Some(codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING),
            "expected pack stub/tool missing, got {err:?}"
        );
        let _ = fs::remove_dir_all(&out);
        let _ = fs::remove_dir_all(&host);
    }

    #[test]
    fn seed_minimal_tree_creates_layout() {
        let dest = temp_dir("seed");
        let tree = dest.join("tree");
        seed_minimal_rootfs_tree(&tree).unwrap();
        assert!(tree.join("usr/local/bin").is_dir());
        assert!(tree.join("README.eidolon").is_file());
        let _ = fs::remove_dir_all(&dest);
    }
}
