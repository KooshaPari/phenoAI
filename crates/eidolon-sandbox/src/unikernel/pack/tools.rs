//! Probe helpers for packaging host tools (no bundled binaries).

use super::{DiskImageBackend, PackMethod};
use std::path::PathBuf;
use std::process::Command;

/// Env override for `mkfs.ext4` / `mkfs` (`EIDOLON_MKFS`).
pub const MKFS_PATH_ENV: &str = "EIDOLON_MKFS";
/// Env override for `virt-make-fs` (`EIDOLON_VIRT_MAKE_FS`).
pub const VIRT_MAKE_FS_PATH_ENV: &str = "EIDOLON_VIRT_MAKE_FS";
/// Env override for `docker` (`EIDOLON_DOCKER`).
pub const DOCKER_PATH_ENV: &str = "EIDOLON_DOCKER";

pub const MKFS_CANDIDATES: &[&str] = &["mkfs.ext4", "mkfs"];
pub const VIRT_MAKE_FS_CANDIDATES: &[&str] = &["virt-make-fs"];
pub const DOCKER_CANDIDATES: &[&str] = &["docker"];

/// Resolved tool pair for [`PackMethod::DockerToExt4`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerToExt4Tools {
    pub docker: PathBuf,
    pub disk: PathBuf,
}

/// Resolve the host binary for a live [`PackMethod`].
///
/// [`PackMethod::DockerToExt4`] returns `None` here — use
/// [`resolve_docker_to_ext4_tools`].
pub fn resolve_pack_tool(method: PackMethod) -> Option<PathBuf> {
    match method {
        PackMethod::Hermetic | PackMethod::DockerToExt4 => None,
        PackMethod::MkfsExt4 => resolve_named(MKFS_PATH_ENV, MKFS_CANDIDATES),
        PackMethod::VirtMakeFs => resolve_named(VIRT_MAKE_FS_PATH_ENV, VIRT_MAKE_FS_CANDIDATES),
        PackMethod::DockerExport => resolve_named(DOCKER_PATH_ENV, DOCKER_CANDIDATES),
    }
}

/// Resolve docker + disk-image tool for compose [`PackMethod::DockerToExt4`].
pub fn resolve_docker_to_ext4_tools(backend: DiskImageBackend) -> Option<DockerToExt4Tools> {
    let docker = resolve_pack_tool(PackMethod::DockerExport)?;
    let disk = resolve_pack_tool(backend.as_pack_method())?;
    Some(DockerToExt4Tools { docker, disk })
}

/// `true` when docker **and** the disk backend tool are discoverable.
pub fn docker_to_ext4_ready(backend: DiskImageBackend) -> bool {
    resolve_docker_to_ext4_tools(backend).is_some()
}

/// `true` when the tool for `method` is discoverable (hermetic → always true).
pub fn pack_tool_ready(method: PackMethod) -> bool {
    match method {
        PackMethod::Hermetic => true,
        PackMethod::DockerToExt4 => docker_to_ext4_ready(DiskImageBackend::MkfsExt4),
        other => resolve_pack_tool(other).is_some(),
    }
}

/// Snapshot of which packaging tools are on PATH / env.
pub fn tool_snapshot() -> PackToolSnapshot {
    PackToolSnapshot {
        mkfs: resolve_named(MKFS_PATH_ENV, MKFS_CANDIDATES),
        virt_make_fs: resolve_named(VIRT_MAKE_FS_PATH_ENV, VIRT_MAKE_FS_CANDIDATES),
        docker: resolve_named(DOCKER_PATH_ENV, DOCKER_CANDIDATES),
    }
}

/// Discoverable packaging tool paths (informational).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackToolSnapshot {
    pub mkfs: Option<PathBuf>,
    pub virt_make_fs: Option<PathBuf>,
    pub docker: Option<PathBuf>,
}

impl PackToolSnapshot {
    /// Both docker and mkfs present (default DockerToExt4 backend).
    pub fn docker_to_ext4_ready(&self) -> bool {
        self.docker.is_some() && self.mkfs.is_some()
    }
}

fn resolve_named(env_key: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var(env_key) {
        let trimmed = override_path.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed);
            // Explicit override set: fail-loud if missing — never fall through to PATH.
            return p.is_file().then_some(p);
        }
    }
    for name in candidates {
        if let Some(p) = which_bin(name) {
            return Some(p);
        }
    }
    None
}

fn which_bin(name: &str) -> Option<PathBuf> {
    let output = Command::new("which").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let p = PathBuf::from(path);
    p.is_file().then_some(p)
}
