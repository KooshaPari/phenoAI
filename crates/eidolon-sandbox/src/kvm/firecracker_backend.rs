//! Live Firecracker / KVM backend via system CLI (`sandbox-kvm` feature).
//!
//! Do not unarchive KDesktopVirt — this is trait-shaped CLI wiring. Hermetic
//! path: version probe on construct / start. When constructed from a
//! [`crate::unikernel::LaunchPlan`] and `FIRECRACKER_INTEGRATION=1` /
//! `UNIKERNEL_BOOT_INTEGRATION=1`, `start` spawns a guest via
//! [`crate::unikernel::boot`]. Guest `exec` uses serial/stdio via
//! [`crate::unikernel::exec`] when a live child is held
//! (`UNIKERNEL_EXEC_INTEGRATION=1`).

use super::{probe, KvmBackend};
use crate::codes;
use crate::unikernel::{boot, exec, LaunchPlan, UnikernelBackend};
use eidolon_core::error::PhenoError;
use eidolon_core::security::{validate_exec_cmd, validate_sandbox_id, SandboxPolicy};
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

fn kvm_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_KVM_STUB,
        format!(
            "KvmBackend::{method} unavailable — Firecracker CLI not reachable \
             ({detail}); enable feature `sandbox-kvm` and install `firecracker` \
             (EIDOLON_FIRECRACKER), or use KvmStub (docs/EXTRACTION_PLAN.md \
             Phase 3; do not unarchive KDesktopVirt routinely)"
        ),
    )
}

fn resource_probe_not_wired(method: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_KVM_STUB,
        format!(
            "FirecrackerKvmClient::{method} not hermetic — guest resource \
             probes still TODO (serial exec via unikernel::exec; set \
             {}=1 / {}=1 for live spawn + {}=1 for serial I/O; Linux \
             `/dev/kvm` required; docs/EXTRACTION_PLAN.md Phase 3)",
            boot::BOOT_INTEGRATION_ENV,
            boot::FIRECRACKER_INTEGRATION_ENV,
            exec::EXEC_INTEGRATION_ENV
        ),
    )
}

/// Live Firecracker-backed KVM sandbox client (`SandboxAutomator`).
///
/// Replaces the fail-loud [`super::KvmStub`] path when feature `sandbox-kvm`
/// is enabled and a system `firecracker` CLI answers `--version`. Compose with
/// [`LaunchPlan`] via [`Self::try_from_plan`] for rootfs/kernel-backed boot.
pub struct FirecrackerKvmClient {
    sandbox_id: String,
    image: String,
    policy: SandboxPolicy,
    firecracker: PathBuf,
    version_line: String,
    kvm_device: bool,
    started: AtomicBool,
    plan: Option<LaunchPlan>,
    guest: Mutex<Option<Child>>,
    config_path: PathBuf,
}

impl FirecrackerKvmClient {
    /// Default image label when callers do not override (`firecracker:local`).
    pub const DEFAULT_IMAGE: &'static str = "firecracker:local";

    /// Resolve Firecracker and prove it answers a version probe.
    pub fn try_new(sandbox_id: &str) -> Result<Self> {
        Self::try_with_image(sandbox_id, Self::DEFAULT_IMAGE, SandboxPolicy::default())
    }

    /// Construct with an explicit image label and isolation policy.
    pub fn try_with_image(
        sandbox_id: &str,
        image: &str,
        policy: SandboxPolicy,
    ) -> Result<Self> {
        validate_sandbox_id(sandbox_id)?;
        if image.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "kvm image label must be non-empty".into(),
            ));
        }
        let Some(firecracker) = probe::resolve_firecracker_cli() else {
            return Err(kvm_unavailable(
                "try_new",
                "firecracker/jailer not found on PATH",
            ));
        };
        let Some(version_line) = Self::run_version(&firecracker) else {
            return Err(kvm_unavailable(
                "try_new",
                format!("firecracker --version failed ({})", firecracker.display()),
            ));
        };
        Ok(Self {
            sandbox_id: sandbox_id.to_string(),
            image: image.to_string(),
            policy,
            firecracker,
            version_line,
            kvm_device: probe::kvm_device_present(),
            started: AtomicBool::new(false),
            plan: None,
            guest: Mutex::new(None),
            config_path: boot::default_firecracker_config_path(sandbox_id),
        })
    }

    /// Compose from a resolved [`LaunchPlan`] (Firecracker backend + artifacts).
    ///
    /// Reuses unikernel probe/validation — no duplicated VirtualStage. Hermetic
    /// `start` re-proves CLI; live spawn when [`boot::firecracker_boot_enabled`].
    pub fn try_from_plan(plan: LaunchPlan) -> Result<Self> {
        if plan.backend != UnikernelBackend::Firecracker {
            return Err(PhenoError::BadRequest(format!(
                "FirecrackerKvmClient::try_from_plan requires \
                 UnikernelBackend::Firecracker (got {:?})",
                plan.backend.as_str()
            )));
        }
        plan.require_ready()?;
        boot::require_firecracker_kernel(&plan)?;
        let mut policy = plan.policy.clone();
        policy.cpu_cores = u32::from(plan.vcpu_count);
        policy.memory_mib = plan.memory_mib;
        let mut client = Self::try_with_image(
            &plan.sandbox_id,
            &format!("firecracker:{}", plan.rootfs.display()),
            policy,
        )?;
        client.config_path = boot::default_firecracker_config_path(&plan.sandbox_id);
        client.plan = Some(plan);
        Ok(client)
    }

    /// Host probe only — does not construct a client.
    pub fn host_firecracker_ready() -> bool {
        probe::firecracker_ready()
    }

    /// Declared sandbox id.
    pub fn sandbox_id(&self) -> &str {
        &self.sandbox_id
    }

    /// Image / rootfs label (metadata; path-backed when from [`LaunchPlan`]).
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Isolation policy carried for microVM resource caps.
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Optional composed [`LaunchPlan`] (set via [`Self::try_from_plan`]).
    pub fn plan(&self) -> Option<&LaunchPlan> {
        self.plan.as_ref()
    }

    /// Path to the resolved Firecracker binary.
    pub fn firecracker_path(&self) -> &std::path::Path {
        &self.firecracker
    }

    /// Cached first line from `firecracker --version` at construct time.
    pub fn cli_version(&self) -> &str {
        &self.version_line
    }

    /// Whether `/dev/kvm` was present at construct time.
    pub fn kvm_device_present(&self) -> bool {
        self.kvm_device
    }

    /// Whether a live guest child was spawned.
    pub fn guest_spawned(&self) -> bool {
        self.guest
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    fn run_version(bin: &std::path::Path) -> Option<String> {
        let output = Command::new(bin).arg("--version").output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    fn recheck_version(&self) -> Result<String> {
        Self::run_version(&self.firecracker).ok_or_else(|| {
            kvm_unavailable(
                "start",
                format!(
                    "firecracker --version failed ({})",
                    self.firecracker.display()
                ),
            )
        })
    }

    fn kill_guest(&self) {
        if let Ok(mut guard) = self.guest.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        let _ = std::fs::remove_file(&self.config_path);
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for FirecrackerKvmClient {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: self.sandbox_id.clone(),
            image: self.image.clone(),
            cpu_limit: self.policy.cpu_cores,
            memory_limit_mb: self.policy.memory_mib,
            disk_limit_mb: self.policy.disk_mib,
        })
    }

    async fn start(&self) -> Result<()> {
        let _ = self.recheck_version()?;
        if let Some(plan) = &self.plan {
            plan.require_ready()?;
            if boot::firecracker_boot_enabled() {
                let child =
                    boot::spawn_firecracker(&self.firecracker, plan, &self.config_path)?;
                let mut guard = self.guest.lock().map_err(|_| {
                    PhenoError::unsupported_platform(
                        codes::SANDBOX_KVM_STUB,
                        "FirecrackerKvmClient guest mutex poisoned",
                    )
                })?;
                *guard = Some(child);
            }
        }
        self.started.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.kill_guest();
        self.started.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn exec(&self, cmd: &str) -> Result<String> {
        validate_exec_cmd(cmd)?;
        if !self.started.load(Ordering::Relaxed) {
            return Err(PhenoError::BadRequest(
                "FirecrackerKvmClient::exec requires start() first".into(),
            ));
        }
        let mut guard = self.guest.lock().map_err(|_| {
            PhenoError::unsupported_platform(
                codes::SANDBOX_KVM_STUB,
                "FirecrackerKvmClient guest mutex poisoned",
            )
        })?;
        exec::exec_cmd_on_guest("FirecrackerKvmClient", &mut guard, cmd)
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        if !self.started.load(Ordering::Relaxed) {
            return Err(PhenoError::BadRequest(
                "FirecrackerKvmClient::resource_usage requires start() first".into(),
            ));
        }
        Err(resource_probe_not_wired("resource_usage"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded sandbox kvm event: {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl KvmBackend for FirecrackerKvmClient {
    fn kvm_ready(&self) -> bool {
        self.kvm_device
    }

    fn firecracker_ready(&self) -> bool {
        true
    }
}
