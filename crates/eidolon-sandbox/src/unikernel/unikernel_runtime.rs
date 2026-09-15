//! Unikernel runtime lifecycle: guest client and stub.

#[cfg(feature = "sandbox-unikernel")]
use std::path::Path;
#[cfg(feature = "sandbox-unikernel")]
use std::process::Child;
#[cfg(feature = "sandbox-unikernel")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "sandbox-unikernel")]
use std::sync::Mutex;

use eidolon_core::error::PhenoError;
#[cfg(feature = "sandbox-unikernel")]
use eidolon_core::security::validate_exec_cmd;
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};

#[cfg(feature = "sandbox-unikernel")]
use super::{LaunchPlan, UnikernelBackend, UnikernelLaunchConfig};
use crate::codes;
#[cfg(feature = "sandbox-unikernel")]
use crate::{kvm, nanovm};

#[cfg(feature = "sandbox-unikernel")]
pub(super) fn resource_probe_not_wired(method: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_UNIKERNEL_STUB,
        format!(
            "UnikernelGuestClient::{method} not hermetic — guest resource \
             probes still TODO (serial exec via unikernel::exec; set \
             {}=1 for live spawn + {}=1 for serial I/O; \
             docs/EXTRACTION_PLAN.md Phase 3; do not unarchive KDesktopVirt \
             routinely)",
            super::boot::BOOT_INTEGRATION_ENV,
            super::exec::EXEC_INTEGRATION_ENV
        ),
    )
}

/// Feature-gated guest client: validates rootfs / kernel / CLI; optional live boot.
///
/// Enabled with `sandbox-unikernel`. Hermetic `start` re-runs
/// [`LaunchPlan::require_ready`]. When [`super::boot::boot_integration_enabled`],
/// `start` also spawns Firecracker/`ops` from the plan (compose with probes).
#[cfg(feature = "sandbox-unikernel")]
pub struct UnikernelGuestClient {
    plan: LaunchPlan,
    started: AtomicBool,
    guest: Mutex<Option<Child>>,
    config_path: std::path::PathBuf,
}

#[cfg(feature = "sandbox-unikernel")]
impl UnikernelGuestClient {
    /// Validate config → [`LaunchPlan`], requiring rootfs (+ optional kernel) and host CLI.
    pub fn try_new(cfg: UnikernelLaunchConfig) -> Result<Self> {
        let plan = LaunchPlan::try_from_config(&cfg)?;
        Self::try_from_plan(plan)
    }

    /// Construct from an already-resolved plan (re-checks readiness).
    pub fn try_from_plan(plan: LaunchPlan) -> Result<Self> {
        plan.require_ready()?;
        let config_path = super::boot::default_firecracker_config_path(&plan.sandbox_id);
        Ok(Self {
            plan,
            started: AtomicBool::new(false),
            guest: Mutex::new(None),
            config_path,
        })
    }

    pub fn plan(&self) -> &LaunchPlan {
        &self.plan
    }

    pub fn sandbox_id(&self) -> &str {
        &self.plan.sandbox_id
    }

    pub fn backend(&self) -> UnikernelBackend {
        self.plan.backend
    }

    pub fn rootfs_path(&self) -> &Path {
        &self.plan.rootfs
    }

    pub fn kernel_path(&self) -> Option<&Path> {
        self.plan.kernel.as_deref()
    }

    /// Host CLI for the plan backend is discoverable (ops / firecracker).
    pub fn host_cli_ready(&self) -> bool {
        self.plan.backend_cli_ready()
    }

    /// Whether a live guest child process is currently held.
    pub fn guest_spawned(&self) -> bool {
        self.guest.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    fn spawn_guest_locked(&self) -> Result<Child> {
        match self.plan.backend {
            UnikernelBackend::Firecracker => {
                let bin = kvm::probe::resolve_firecracker_cli().ok_or_else(|| {
                    PhenoError::unsupported_platform(
                        codes::SANDBOX_UNIKERNEL_STUB,
                        "firecracker not found (EIDOLON_FIRECRACKER / PATH) for \
                         live unikernel boot",
                    )
                })?;
                super::boot::spawn_firecracker(&bin, &self.plan, &self.config_path)
            }
            UnikernelBackend::NanoVm => {
                let ops = nanovm::probe::resolve_ops_cli().ok_or_else(|| {
                    PhenoError::unsupported_platform(
                        codes::SANDBOX_UNIKERNEL_STUB,
                        "NanoVMs ops not found (EIDOLON_OPS / PATH) for live \
                         unikernel boot",
                    )
                })?;
                super::boot::spawn_ops(&ops, &self.plan)
            }
        }
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

#[cfg(feature = "sandbox-unikernel")]
#[async_trait::async_trait]
impl SandboxAutomator for UnikernelGuestClient {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: self.plan.sandbox_id.clone(),
            image: format!(
                "unikernel:{}:{}",
                self.plan.backend.as_str(),
                self.plan.rootfs.display()
            ),
            cpu_limit: u32::from(self.plan.vcpu_count),
            memory_limit_mb: self.plan.memory_mib,
            disk_limit_mb: self.plan.policy.disk_mib,
        })
    }

    async fn start(&self) -> Result<()> {
        self.plan.require_ready()?;
        if super::boot::boot_integration_enabled() {
            let child = self.spawn_guest_locked()?;
            let mut guard = self.guest.lock().map_err(|_| {
                PhenoError::unsupported_platform(
                    codes::SANDBOX_UNIKERNEL_STUB,
                    "UnikernelGuestClient guest mutex poisoned",
                )
            })?;
            *guard = Some(child);
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
                "UnikernelGuestClient::exec requires start() first".into(),
            ));
        }
        let mut guard = self.guest.lock().map_err(|_| {
            PhenoError::unsupported_platform(
                codes::SANDBOX_UNIKERNEL_STUB,
                "UnikernelGuestClient guest mutex poisoned",
            )
        })?;
        super::exec::exec_cmd_on_guest("UnikernelGuestClient", &mut guard, cmd)
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        if !self.started.load(Ordering::Relaxed) {
            return Err(PhenoError::BadRequest(
                "UnikernelGuestClient::resource_usage requires start() first".into(),
            ));
        }
        Err(resource_probe_not_wired("resource_usage"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded unikernel event: {:?}", event);
        Ok(())
    }
}

/// Fail-loud stub when `sandbox-unikernel` is off or launch is not attempted.
#[derive(Debug, Default, Clone)]
pub struct UnikernelStub {
    sandbox_id: String,
}

impl UnikernelStub {
    pub fn new(sandbox_id: &str) -> Self {
        Self {
            sandbox_id: sandbox_id.to_string(),
        }
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::SANDBOX_UNIKERNEL_STUB,
            format!(
                "UnikernelGuest::{method} not implemented — enable feature \
                 `sandbox-unikernel`, provide rootfs via path / {rootfs_env}, \
                 and ensure ops or firecracker is on PATH (compose with \
                 nanovm/kvm probes; docs/EXTRACTION_PLAN.md; do not unarchive \
                 KDesktopVirt routinely)",
                rootfs_env = super::probe::ROOTFS_PATH_ENV
            ),
        )
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for UnikernelStub {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: self.sandbox_id.clone(),
            image: "stub:unikernel".to_string(),
            cpu_limit: 0,
            memory_limit_mb: 0,
            disk_limit_mb: None,
        })
    }

    async fn start(&self) -> Result<()> {
        Err(Self::unsupported("start"))
    }

    async fn stop(&self) -> Result<()> {
        Err(Self::unsupported("stop"))
    }

    async fn exec(&self, _cmd: &str) -> Result<String> {
        Err(Self::unsupported("exec"))
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        Err(Self::unsupported("resource_usage"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event (unikernel stub): {:?}", event);
        Ok(())
    }
}
