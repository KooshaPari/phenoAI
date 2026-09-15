//! Package manifest, pack request, and core types.

use std::fs;
use std::path::{Path, PathBuf};

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde::{Deserialize, Serialize};

use super::bake_agent::{BakeAgentRequest, DEFAULT_GUEST_AGENT_REL, DEFAULT_GUEST_UNIT_REL};
use crate::codes;
use crate::unikernel::{KernelConfig, RootfsConfig, RootfsFormat};

/// Default sparse image size for [`PackMethod::MkfsExt4`] when
/// [`PackRequest::image_size_bytes`] is unset (64 MiB).
pub const DEFAULT_IMAGE_SIZE_BYTES: u64 = 64 * 1024 * 1024;

/// Env gate for destructive / live image packaging (`mkfs`, `virt-make-fs`,
/// `docker export`, …).
///
/// When unset (default), only [`super::hermetic::HermeticPackBuilder`] runs
/// (validate + stage + manifest). Live tool invocation requires `"1"`.
pub const PACK_INTEGRATION_ENV: &str = "ROOTFS_PACK_INTEGRATION";

/// Default manifest filename written into a staging directory.
pub const MANIFEST_FILENAME: &str = "eidolon-package-manifest.json";

/// Schema version for [`PackageManifest`] JSON.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Second-stage disk image tool for [`PackMethod::DockerToExt4`] compose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiskImageBackend {
    /// `mkfs.ext4 -F -d <tree> <image>` (default).
    #[default]
    MkfsExt4,
    /// `virt-make-fs --type=ext4 --format=raw <tree> <image>`.
    VirtMakeFs,
}

impl DiskImageBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MkfsExt4 => "mkfs_ext4",
            Self::VirtMakeFs => "virt_make_fs",
        }
    }

    pub fn as_pack_method(self) -> PackMethod {
        match self {
            Self::MkfsExt4 => PackMethod::MkfsExt4,
            Self::VirtMakeFs => PackMethod::VirtMakeFs,
        }
    }
}

/// How artifacts were (or will be) packaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackMethod {
    /// Validate existing files, optional stage, write manifest — no mkfs/docker.
    Hermetic,
    /// Live: `mkfs.ext4` (or `mkfs`) against a directory tree or image file.
    MkfsExt4,
    /// Live: `virt-make-fs` from a directory tree.
    VirtMakeFs,
    /// Live: `docker export` → tarball + tree extract. Manifest format is
    /// always [`RootfsFormat::Raw`] — never claim Ext4 for a tar alone.
    DockerExport,
    /// Live compose: `docker export` → rootfs-tree → [`DiskImageBackend`]
    /// (`mkfs.ext4` or `virt-make-fs`) → Ext4 disk image for Firecracker /
    /// [`super::super::LaunchPlan`]. Requires docker **and** the disk tool.
    DockerToExt4,
}

impl PackMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hermetic => "hermetic",
            Self::MkfsExt4 => "mkfs_ext4",
            Self::VirtMakeFs => "virt_make_fs",
            Self::DockerExport => "docker_export",
            Self::DockerToExt4 => "docker_to_ext4",
        }
    }

    /// Primary host tool for a live pack (`None` for hermetic / dual-tool compose).
    ///
    /// [`Self::DockerToExt4`] needs **both** `docker` and the
    /// [`DiskImageBackend`] tool — use [`super::tools::docker_to_ext4_ready`] /
    /// [`super::tools::resolve_docker_to_ext4_tools`].
    pub fn required_tool(self) -> Option<&'static str> {
        match self {
            Self::Hermetic | Self::DockerToExt4 => None,
            Self::MkfsExt4 => Some("mkfs.ext4"),
            Self::VirtMakeFs => Some("virt-make-fs"),
            Self::DockerExport => Some("docker"),
        }
    }
}

/// Checksum algorithm recorded in the package manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksumAlgorithm {
    Sha256,
}

impl ChecksumAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
        }
    }
}

/// Digest of a staged / source artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChecksum {
    pub algorithm: ChecksumAlgorithm,
    pub hex: String,
}

/// Declared guest package: kernel + rootfs + format + optional checksums.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub schema_version: u32,
    /// Path to the rootfs / disk image / tarball (after staging if used).
    pub rootfs: PathBuf,
    pub rootfs_format: String,
    pub kernel: Option<PathBuf>,
    pub rootfs_checksum: Option<ArtifactChecksum>,
    pub kernel_checksum: Option<ArtifactChecksum>,
    pub pack_method: PackMethod,
    pub staging_dir: Option<PathBuf>,
}

impl PackageManifest {
    /// Build a [`RootfsConfig`] LaunchPlan / UnikernelLaunchConfig can consume.
    pub fn to_rootfs_config(&self) -> Result<RootfsConfig> {
        let format = parse_rootfs_format(&self.rootfs_format)?;
        let cfg = RootfsConfig::with_format(&self.rootfs, format);
        cfg.validate_path()?;
        Ok(cfg)
    }

    /// Optional kernel config for Firecracker-style plans.
    pub fn to_kernel_config(&self) -> Result<Option<KernelConfig>> {
        match &self.kernel {
            Some(path) => {
                let cfg = KernelConfig::new(path);
                cfg.validate_path()?;
                Ok(Some(cfg))
            }
            None => Ok(None),
        }
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| {
            PhenoError::Internal(format!(
                "{}: package manifest serialize failed: {e}",
                codes::SANDBOX_ROOTFS_PACK_IO
            ))
        })
    }

    /// Parse from JSON bytes / string.
    pub fn from_json(s: &str) -> Result<Self> {
        serde_json::from_str(s).map_err(|e| {
            PhenoError::Internal(format!(
                "{}: package manifest parse failed: {e}",
                codes::SANDBOX_ROOTFS_PACK_IO
            ))
        })
    }

    /// Write JSON to `path`.
    pub fn write_to(&self, path: &Path) -> Result<()> {
        let json = self.to_json()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| pack_io(path, e))?;
        }
        fs::write(path, json.as_bytes()).map_err(|e| pack_io(path, e))
    }

    /// Load from a file.
    pub fn read_from(path: &Path) -> Result<Self> {
        let s = fs::read_to_string(path).map_err(|e| pack_io(path, e))?;
        Self::from_json(&s)
    }
}

/// Inputs for a hermetic (or live) pack operation.
#[derive(Debug, Clone)]
pub struct PackRequest {
    /// Hermetic: existing rootfs image file.
    /// Live mkfs/virt-make-fs: directory tree (mkfs also accepts a pre-sized image file).
    /// Live docker export / docker_to_ext4: image or container ref (need not exist on disk).
    pub rootfs_source: PathBuf,
    pub rootfs_format: RootfsFormat,
    pub kernel: Option<PathBuf>,
    /// When set, copy or link artifacts here and write [`MANIFEST_FILENAME`].
    /// **Required** for live methods (output image / tarball destination).
    pub staging_dir: Option<PathBuf>,
    /// Prefer hard-link into staging when possible; else copy.
    pub link_artifacts: bool,
    pub pack_method: PackMethod,
    /// When true (and `sandbox-rootfs-pack` enables sha2), hash staged files.
    pub compute_checksum: bool,
    /// Sparse image size for [`PackMethod::MkfsExt4`] / [`PackMethod::DockerToExt4`]
    /// directory packs. Default: [`DEFAULT_IMAGE_SIZE_BYTES`] (64 MiB).
    pub image_size_bytes: Option<u64>,
    /// Disk tool for [`PackMethod::DockerToExt4`] (default [`DiskImageBackend::MkfsExt4`]).
    pub disk_backend: DiskImageBackend,
    /// When true, stage `eidolon-vsock-agent` into the rootfs tree before
    /// mkfs / virt-make-fs / DockerToExt4 disk step. Missing binary → fail-loud
    /// [`codes::SANDBOX_VSOCK_AGENT_MISSING`] (never silent skip).
    pub bake_vsock_agent: bool,
    /// Explicit host path to `eidolon-vsock-agent`. Else [`bake_agent::AGENT_PATH_ENV`]
    /// (`EIDOLON_VSOCK_AGENT`), else cargo-target discovery.
    pub vsock_agent_bin: Option<PathBuf>,
    /// When baking the agent, also stage the example systemd unit from
    /// `docs/guides/vsock-guest-agent.md`.
    pub bake_systemd_unit: bool,
}

impl PackRequest {
    pub fn hermetic(rootfs_source: impl Into<PathBuf>, format: RootfsFormat) -> Self {
        Self {
            rootfs_source: rootfs_source.into(),
            rootfs_format: format,
            kernel: None,
            staging_dir: None,
            link_artifacts: false,
            pack_method: PackMethod::Hermetic,
            compute_checksum: false,
            image_size_bytes: None,
            disk_backend: DiskImageBackend::MkfsExt4,
            bake_vsock_agent: false,
            vsock_agent_bin: None,
            bake_systemd_unit: false,
        }
    }

    pub fn with_kernel(mut self, kernel: impl Into<PathBuf>) -> Self {
        self.kernel = Some(kernel.into());
        self
    }

    pub fn with_staging(mut self, dir: impl Into<PathBuf>, link_artifacts: bool) -> Self {
        self.staging_dir = Some(dir.into());
        self.link_artifacts = link_artifacts;
        self
    }

    pub fn with_checksum(mut self, compute: bool) -> Self {
        self.compute_checksum = compute;
        self
    }

    pub fn with_method(mut self, method: PackMethod) -> Self {
        self.pack_method = method;
        self
    }

    pub fn with_image_size(mut self, bytes: u64) -> Self {
        self.image_size_bytes = Some(bytes);
        self
    }

    pub fn with_disk_backend(mut self, backend: DiskImageBackend) -> Self {
        self.disk_backend = backend;
        self
    }

    /// Enable bake of `eidolon-vsock-agent` into the rootfs tree before pack.
    pub fn with_bake_vsock_agent(mut self, bake: bool) -> Self {
        self.bake_vsock_agent = bake;
        self
    }

    pub fn with_vsock_agent_bin(mut self, path: impl Into<PathBuf>) -> Self {
        self.vsock_agent_bin = Some(path.into());
        self
    }

    pub fn with_bake_systemd_unit(mut self, bake: bool) -> Self {
        self.bake_systemd_unit = bake;
        self
    }

    /// Path / hygiene validation (does not require tools).
    pub fn validate(&self) -> Result<()> {
        validate_nonempty(&self.rootfs_source, "rootfs_source")?;
        if let Some(k) = &self.kernel {
            validate_nonempty(k, "kernel")?;
        }
        if let Some(d) = &self.staging_dir {
            validate_nonempty(d, "staging_dir")?;
        }
        if let Some(a) = &self.vsock_agent_bin {
            validate_nonempty(a, "vsock_agent_bin")?;
        }
        if self.pack_method == PackMethod::DockerExport
            && matches!(self.rootfs_format, RootfsFormat::Ext4)
        {
            return Err(PhenoError::BadRequest(
                "docker_export produces a tarball (RootfsFormat::Raw), not an Ext4 disk \
                 image — refuse to claim Ext4 for tar alone. Use PackMethod::DockerToExt4 \
                 (compose export → mkfs.ext4 / virt-make-fs) for Firecracker LaunchPlan \
                 disks (docs/reference/rootfs-pack.md)"
                    .into(),
            ));
        }
        if self.bake_vsock_agent && self.pack_method == PackMethod::DockerExport {
            return Err(PhenoError::BadRequest(
                "bake_vsock_agent with DockerExport alone is unsupported — bake needs a \
                 rootfs-tree before Ext4 pack. Use PackMethod::DockerToExt4, MkfsExt4, or \
                 VirtMakeFs (docs/guides/vsock-guest-agent.md)"
                    .into(),
            ));
        }
        if self.bake_vsock_agent && self.pack_method == PackMethod::Hermetic {
            return Err(PhenoError::BadRequest(
                "bake_vsock_agent with PackMethod::Hermetic is unsupported — hermetic packs \
                 existing image files; stage with stage_vsock_agent_into_tree then \
                 MkfsExt4 / VirtMakeFs / DockerToExt4 (docs/guides/vsock-guest-agent.md)"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Build a [`BakeAgentRequest`] from this pack request (when bake is enabled).
    pub fn bake_agent_request(&self) -> BakeAgentRequest {
        BakeAgentRequest {
            agent_bin: self.vsock_agent_bin.clone(),
            guest_agent_rel: PathBuf::from(DEFAULT_GUEST_AGENT_REL),
            bake_systemd_unit: self.bake_systemd_unit,
            guest_unit_rel: PathBuf::from(DEFAULT_GUEST_UNIT_REL),
            link_artifacts: self.link_artifacts,
        }
    }
}

pub(super) fn rootfs_format_label(format: RootfsFormat) -> &'static str {
    match format {
        RootfsFormat::Unspecified => "unspecified",
        RootfsFormat::Ext4 => "ext4",
        RootfsFormat::Squashfs => "squashfs",
        RootfsFormat::Raw => "raw",
        RootfsFormat::OpsPackage => "ops_package",
    }
}

pub(super) fn parse_rootfs_format(label: &str) -> Result<RootfsFormat> {
    match label.trim().to_ascii_lowercase().as_str() {
        "unspecified" | "" => Ok(RootfsFormat::Unspecified),
        "ext4" => Ok(RootfsFormat::Ext4),
        "squashfs" => Ok(RootfsFormat::Squashfs),
        "raw" => Ok(RootfsFormat::Raw),
        "ops_package" | "ops" => Ok(RootfsFormat::OpsPackage),
        other => Err(PhenoError::BadRequest(format!(
            "unknown rootfs_format in package manifest: {other:?}"
        ))),
    }
}

pub(super) fn validate_nonempty(path: &Path, kind: &str) -> Result<()> {
    if path.as_os_str().is_empty() || path.to_string_lossy().trim().is_empty() {
        return Err(PhenoError::BadRequest(format!(
            "rootfs pack {kind} path must be non-empty"
        )));
    }
    Ok(())
}

pub(super) fn require_source_file(path: &Path, kind: &str) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }
    let code = if kind == "kernel" {
        codes::SANDBOX_KERNEL_MISSING
    } else {
        codes::SANDBOX_ROOTFS_MISSING
    };
    Err(PhenoError::unsupported_platform(
        code,
        format!(
            "rootfs pack {kind} missing or not a file at {} — fail-loud \
             (hermetic pack requires existing artifacts; docs/EXTRACTION_PLAN.md)",
            path.display()
        ),
    ))
}

pub(super) fn file_name_or(path: &Path, fallback: &str) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(super) fn stage_artifact(src: &Path, dest: &Path, link: bool) -> Result<()> {
    if dest.exists() {
        fs::remove_file(dest).map_err(|e| pack_io(dest, e))?;
    }
    if link {
        #[cfg(unix)]
        {
            if fs::hard_link(src, dest).is_ok() {
                return Ok(());
            }
        }
    }
    fs::copy(src, dest).map_err(|e| {
        PhenoError::Internal(format!(
            "{}: failed to stage {} → {}: {e}",
            codes::SANDBOX_ROOTFS_PACK_IO,
            src.display(),
            dest.display()
        ))
    })?;
    Ok(())
}

pub(super) fn fill_checksums(manifest: &mut PackageManifest, compute: bool) -> Result<()> {
    if !compute {
        manifest.rootfs_checksum = None;
        manifest.kernel_checksum = None;
        return Ok(());
    }

    #[cfg(any(feature = "sandbox-rootfs-pack", feature = "sandbox-audit"))]
    {
        manifest.rootfs_checksum = Some(hash_file(&manifest.rootfs)?);
        manifest.kernel_checksum = match &manifest.kernel {
            Some(k) => Some(hash_file(k)?),
            None => None,
        };
        return Ok(());
    }

    #[cfg(not(any(feature = "sandbox-rootfs-pack", feature = "sandbox-audit")))]
    {
        Err(PhenoError::unsupported_platform(
            codes::SANDBOX_ROOTFS_PACK_STUB,
            "compute_checksum requires feature `sandbox-rootfs-pack` (or \
             `sandbox-audit` for sha2); hermetic pack without checksums works \
             with compute_checksum=false",
        ))
    }
}

#[cfg(any(feature = "sandbox-rootfs-pack", feature = "sandbox-audit"))]
fn hash_file(path: &Path) -> Result<ArtifactChecksum> {
    use std::io::Read;

    use sha2::{Digest, Sha256};
    let mut file = fs::File::open(path).map_err(|e| pack_io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(|e| pack_io(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(ArtifactChecksum {
        algorithm: ChecksumAlgorithm::Sha256,
        hex: hex_encode(&digest),
    })
}

#[cfg(any(feature = "sandbox-rootfs-pack", feature = "sandbox-audit"))]
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

pub(super) fn pack_io(path: &Path, err: std::io::Error) -> PhenoError {
    PhenoError::Internal(format!(
        "{}: {} ({err})",
        codes::SANDBOX_ROOTFS_PACK_IO,
        path.display()
    ))
}
