//! Unikernel configuration types: backend, rootfs, kernel, launch config, and plan.

use std::path::{Path, PathBuf};

use eidolon_core::error::PhenoError;
use eidolon_core::security::{validate_sandbox_id, SandboxPolicy};
use eidolon_core::Result;

use crate::{codes, kvm, nanovm};

/// Which host CLI / hypervisor path a launch plan targets.
///
/// Maps onto existing [`crate::nanovm`] / [`crate::kvm`] probes — shared types
/// only; no second VirtualStage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnikernelBackend {
    /// NanoVMs `ops` CLI path ([`crate::nanovm::probe`]).
    NanoVm,
    /// Firecracker / KVM path ([`crate::kvm::probe`]).
    Firecracker,
}

impl UnikernelBackend {
    /// Stable label for metadata / diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NanoVm => "nanovm",
            Self::Firecracker => "firecracker",
        }
    }
}

/// On-disk root filesystem / guest image artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootfsConfig {
    /// Absolute or relative path to the rootfs / image file.
    pub path: PathBuf,
    /// Optional format hint (validation is path/existence only today).
    pub format: RootfsFormat,
}

/// Rootfs image format hint (informational until a launcher consumes it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RootfsFormat {
    /// Unspecified / caller-managed.
    #[default]
    Unspecified,
    /// ext4 disk image (typical Firecracker rootfs).
    Ext4,
    /// squashfs image.
    Squashfs,
    /// Raw disk image.
    Raw,
    /// NanoVMs ops package / unikernel image.
    OpsPackage,
}

impl RootfsConfig {
    /// Construct from an explicit path with default format.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            format: RootfsFormat::default(),
        }
    }

    /// Construct with an explicit format hint.
    pub fn with_format(path: impl Into<PathBuf>, format: RootfsFormat) -> Self {
        Self {
            path: path.into(),
            format,
        }
    }

    /// Validate path hygiene (non-empty) without requiring the file to exist.
    pub fn validate_path(&self) -> Result<()> {
        validate_artifact_path(&self.path, "rootfs")
    }

    /// Validate path and require the file to exist on disk.
    pub fn require_present(&self) -> Result<()> {
        self.validate_path()?;
        require_file(&self.path, codes::SANDBOX_ROOTFS_MISSING, "rootfs")
    }
}

/// Optional guest kernel image (Firecracker-style; unused for some NanoVMs flows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelConfig {
    pub path: PathBuf,
    /// Boot arguments passed to the guest kernel (may be empty).
    pub boot_args: String,
}

impl KernelConfig {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            boot_args: String::new(),
        }
    }

    pub fn with_boot_args(path: impl Into<PathBuf>, boot_args: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            boot_args: boot_args.into(),
        }
    }

    pub fn validate_path(&self) -> Result<()> {
        validate_artifact_path(&self.path, "kernel")
    }

    pub fn require_present(&self) -> Result<()> {
        self.validate_path()?;
        require_file(&self.path, codes::SANDBOX_KERNEL_MISSING, "kernel")
    }
}

/// Declarative unikernel / microVM launch configuration (shared types).
#[derive(Debug, Clone)]
pub struct UnikernelLaunchConfig {
    pub sandbox_id: String,
    pub backend: UnikernelBackend,
    pub rootfs: RootfsConfig,
    pub kernel: Option<KernelConfig>,
    pub vcpu_count: u8,
    pub memory_mib: u32,
    pub policy: SandboxPolicy,
}

impl UnikernelLaunchConfig {
    /// Build a config; validates `sandbox_id` and path hygiene (not presence).
    pub fn try_new(
        sandbox_id: &str,
        backend: UnikernelBackend,
        rootfs: RootfsConfig,
    ) -> Result<Self> {
        validate_sandbox_id(sandbox_id)?;
        rootfs.validate_path()?;
        Ok(Self {
            sandbox_id: sandbox_id.to_string(),
            backend,
            rootfs,
            kernel: None,
            vcpu_count: 1,
            memory_mib: 128,
            policy: SandboxPolicy::default(),
        })
    }

    pub fn with_kernel(mut self, kernel: KernelConfig) -> Result<Self> {
        kernel.validate_path()?;
        self.kernel = Some(kernel);
        Ok(self)
    }

    pub fn with_resources(mut self, vcpu_count: u8, memory_mib: u32) -> Result<Self> {
        if vcpu_count == 0 {
            return Err(PhenoError::BadRequest(
                "unikernel vcpu_count must be >= 1".into(),
            ));
        }
        if memory_mib == 0 {
            return Err(PhenoError::BadRequest(
                "unikernel memory_mib must be >= 1".into(),
            ));
        }
        self.vcpu_count = vcpu_count;
        self.memory_mib = memory_mib;
        Ok(self)
    }

    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Resolve rootfs (and optional kernel) from env when paths were placeholders,
    /// then require artifacts on disk. Fail-loud — never silent success.
    pub fn resolve_and_require(&self) -> Result<LaunchPlan> {
        LaunchPlan::try_from_config(self)
    }
}

/// Resolved, validated launch plan ready for a future guest boot.
///
/// Composes host CLI readiness from nanovm/kvm probes without owning a
/// second VirtualStage implementation.
#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub sandbox_id: String,
    pub backend: UnikernelBackend,
    pub rootfs: PathBuf,
    pub rootfs_format: RootfsFormat,
    pub kernel: Option<PathBuf>,
    pub boot_args: String,
    pub vcpu_count: u8,
    pub memory_mib: u32,
    pub policy: SandboxPolicy,
}

impl LaunchPlan {
    /// Validate config, resolve env overrides, require artifacts, and check
    /// that the target backend's host CLI is discoverable.
    pub fn try_from_config(cfg: &UnikernelLaunchConfig) -> Result<Self> {
        validate_sandbox_id(&cfg.sandbox_id)?;
        cfg.rootfs.validate_path()?;
        if let Some(k) = &cfg.kernel {
            k.validate_path()?;
        }
        if cfg.vcpu_count == 0 {
            return Err(PhenoError::BadRequest(
                "unikernel vcpu_count must be >= 1".into(),
            ));
        }
        if cfg.memory_mib == 0 {
            return Err(PhenoError::BadRequest(
                "unikernel memory_mib must be >= 1".into(),
            ));
        }

        let rootfs =
            super::probe::resolve_rootfs(Some(cfg.rootfs.path.as_path())).ok_or_else(|| {
                rootfs_missing(format!(
                    "rootfs not found at {} (set {} or pass an existing path)",
                    cfg.rootfs.path.display(),
                    super::probe::ROOTFS_PATH_ENV
                ))
            })?;
        require_file(&rootfs, codes::SANDBOX_ROOTFS_MISSING, "rootfs")?;

        let (kernel, boot_args) = match &cfg.kernel {
            Some(k) => {
                let path =
                    super::probe::resolve_kernel(Some(k.path.as_path())).ok_or_else(|| {
                        kernel_missing(format!(
                            "kernel not found at {} (set {} or pass an existing path)",
                            k.path.display(),
                            super::probe::KERNEL_PATH_ENV
                        ))
                    })?;
                require_file(&path, codes::SANDBOX_KERNEL_MISSING, "kernel")?;
                (Some(path), k.boot_args.clone())
            }
            None => {
                // Env-only kernel is optional; if set it must exist.
                if let Some(path) = super::probe::resolve_kernel(None) {
                    require_file(&path, codes::SANDBOX_KERNEL_MISSING, "kernel")?;
                    (Some(path), String::new())
                } else {
                    (None, String::new())
                }
            }
        };

        require_backend_cli(cfg.backend)?;

        Ok(Self {
            sandbox_id: cfg.sandbox_id.clone(),
            backend: cfg.backend,
            rootfs,
            rootfs_format: cfg.rootfs.format,
            kernel,
            boot_args,
            vcpu_count: cfg.vcpu_count,
            memory_mib: cfg.memory_mib,
            policy: cfg.policy.clone(),
        })
    }

    /// Re-check artifacts + backend CLI (hermetic preflight).
    pub fn require_ready(&self) -> Result<()> {
        require_file(&self.rootfs, codes::SANDBOX_ROOTFS_MISSING, "rootfs")?;
        if let Some(k) = &self.kernel {
            require_file(k, codes::SANDBOX_KERNEL_MISSING, "kernel")?;
        }
        require_backend_cli(self.backend)
    }

    /// Whether the composed host CLI for this backend is discoverable.
    pub fn backend_cli_ready(&self) -> bool {
        backend_cli_present(self.backend)
    }

    /// Build Firecracker `--config-file` JSON for this plan (kernel required).
    pub fn firecracker_config_json(&self) -> Result<String> {
        super::boot::firecracker_config_json(self)
    }

    /// Build Firecracker argv for `bin` + `config_path` (pure; no spawn).
    pub fn firecracker_argv(&self, bin: &Path, config_path: &Path) -> Vec<std::ffi::OsString> {
        super::boot::firecracker_argv(bin, config_path)
    }

    /// Build NanoVMs `ops run` argv for `ops` (pure; no spawn).
    pub fn ops_run_argv(&self, ops: &Path) -> Result<Vec<std::ffi::OsString>> {
        super::boot::ops_run_argv(ops, self)
    }
}

pub(super) fn backend_cli_present(backend: UnikernelBackend) -> bool {
    match backend {
        UnikernelBackend::NanoVm => nanovm::probe::ops_ready(),
        UnikernelBackend::Firecracker => kvm::probe::firecracker_ready(),
    }
}

pub(super) fn require_backend_cli(backend: UnikernelBackend) -> Result<()> {
    if backend_cli_present(backend) {
        return Ok(());
    }
    let detail = match backend {
        UnikernelBackend::NanoVm => {
            "NanoVMs `ops` not found (EIDOLON_OPS / PATH); enable sandbox-nanovm for OpsNanoVmClient"
        }
        UnikernelBackend::Firecracker => {
            "firecracker not found (EIDOLON_FIRECRACKER / PATH); enable sandbox-kvm for FirecrackerKvmClient"
        }
    };
    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_UNIKERNEL_STUB,
        format!(
            "UnikernelLaunchPlan backend={:?} host CLI unavailable — {detail} \
             (docs/EXTRACTION_PLAN.md; do not unarchive KDesktopVirt routinely)",
            backend.as_str()
        ),
    ))
}

fn validate_artifact_path(path: &Path, kind: &str) -> Result<()> {
    let s = path.to_string_lossy();
    if s.trim().is_empty() {
        return Err(PhenoError::BadRequest(format!(
            "unikernel {kind} path must be non-empty"
        )));
    }
    Ok(())
}

fn require_file(path: &Path, code: &'static str, kind: &str) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }
    Err(PhenoError::unsupported_platform(
        code,
        format!(
            "unikernel {kind} missing or not a file at {} — fail-loud \
             (set EIDOLON_ROOTFS / EIDOLON_KERNEL or pass an explicit existing path; \
              docs/EXTRACTION_PLAN.md; do not unarchive KDesktopVirt routinely)",
            path.display()
        ),
    ))
}

fn rootfs_missing(detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_ROOTFS_MISSING,
        format!("unikernel rootfs unavailable — {detail}"),
    )
}

fn kernel_missing(detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_KERNEL_MISSING,
        format!("unikernel kernel unavailable — {detail}"),
    )
}
