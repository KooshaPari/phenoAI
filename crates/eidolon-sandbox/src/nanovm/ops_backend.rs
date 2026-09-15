//! Live nanoVMs backend via system `ops` CLI (`sandbox-nanovm` feature).
//!
//! Do not unarchive KDesktopVirt — this is trait-shaped CLI wiring. Hermetic
//! path: version probe on construct / start. When constructed from a
//! [`crate::unikernel::LaunchPlan`] and `NANOVM_INTEGRATION=1` /
//! `UNIKERNEL_BOOT_INTEGRATION=1`, `start` spawns `ops run` via
//! [`crate::unikernel::boot`]. Guest `exec` uses serial/stdio via
//! [`crate::unikernel::exec`] when a live child is held
//! (`UNIKERNEL_EXEC_INTEGRATION=1`).

use super::{probe, NanoVmBackend};
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

fn nanovm_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_NANOVM_STUB,
        format!(
            "NanoVmBackend::{method} unavailable — NanoVMs `ops` CLI not \
             reachable ({detail}); enable feature `sandbox-nanovm` and install \
             `ops` (EIDOLON_OPS), or use NanoVmStub (docs/EXTRACTION_PLAN.md \
             Phase 3; do not unarchive KDesktopVirt routinely)"
        ),
    )
}

fn resource_probe_not_wired(method: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_NANOVM_STUB,
        format!(
            "OpsNanoVmClient::{method} not hermetic — guest resource probes \
             still TODO (serial exec via unikernel::exec; set {}=1 / {}=1 \
             for live spawn + {}=1 for serial I/O; docs/EXTRACTION_PLAN.md \
             Phase 3)",
            boot::BOOT_INTEGRATION_ENV,
            boot::NANOVM_BOOT_INTEGRATION_ENV,
            exec::EXEC_INTEGRATION_ENV
        ),
    )
}

/// Live `ops`-backed nanoVMs sandbox client (`SandboxAutomator`).
///
/// Replaces the fail-loud [`super::NanoVmStub`] path when feature
/// `sandbox-nanovm` is enabled and a system `ops` CLI answers `version`.
/// Compose with [`LaunchPlan`] via [`Self::try_from_plan`] for image boot.
pub struct OpsNanoVmClient {
    sandbox_id: String,
    image: String,
    policy: SandboxPolicy,
    ops: PathBuf,
    version_line: String,
    started: AtomicBool,
    plan: Option<LaunchPlan>,
    guest: Mutex<Option<Child>>,
}

impl OpsNanoVmClient {
    /// Default image label when callers do not override (`ops:local`).
    pub const DEFAULT_IMAGE: &'static str = "ops:local";

    /// Resolve `ops` and prove it answers a version probe.
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
                "nanovm image label must be non-empty".into(),
            ));
        }
        let Some(ops) = probe::resolve_ops_cli() else {
            return Err(nanovm_unavailable(
                "try_new",
                "ops/nanos not found on PATH",
            ));
        };
        let Some(version_line) = Self::run_version(&ops) else {
            return Err(nanovm_unavailable(
                "try_new",
                format!("ops version failed ({})", ops.display()),
            ));
        };
        Ok(Self {
            sandbox_id: sandbox_id.to_string(),
            image: image.to_string(),
            policy,
            ops,
            version_line,
            started: AtomicBool::new(false),
            plan: None,
            guest: Mutex::new(None),
        })
    }

    /// Compose from a resolved [`LaunchPlan`] (NanoVm backend + rootfs).
    pub fn try_from_plan(plan: LaunchPlan) -> Result<Self> {
        if plan.backend != UnikernelBackend::NanoVm {
            return Err(PhenoError::BadRequest(format!(
                "OpsNanoVmClient::try_from_plan requires UnikernelBackend::NanoVm \
                 (got {:?})",
                plan.backend.as_str()
            )));
        }
        plan.require_ready()?;
        let mut policy = plan.policy.clone();
        policy.cpu_cores = u32::from(plan.vcpu_count);
        policy.memory_mib = plan.memory_mib;
        let mut client = Self::try_with_image(
            &plan.sandbox_id,
            &format!("ops:{}", plan.rootfs.display()),
            policy,
        )?;
        client.plan = Some(plan);
        Ok(client)
    }

    /// Host probe only — does not construct a client.
    pub fn host_ops_ready() -> bool {
        probe::ops_ready()
    }

    /// Declared sandbox id.
    pub fn sandbox_id(&self) -> &str {
        &self.sandbox_id
    }

    /// Image / package label (path-backed when from [`LaunchPlan`]).
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Isolation policy carried for unikernel resource caps.
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Optional composed [`LaunchPlan`] (set via [`Self::try_from_plan`]).
    pub fn plan(&self) -> Option<&LaunchPlan> {
        self.plan.as_ref()
    }

    /// Path to the resolved `ops` binary.
    pub fn ops_path(&self) -> &std::path::Path {
        &self.ops
    }

    /// Cached first line from `ops version` at construct time.
    pub fn cli_version(&self) -> &str {
        &self.version_line
    }

    /// Whether a live guest child was spawned.
    pub fn guest_spawned(&self) -> bool {
        self.guest
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    fn run_version(ops: &std::path::Path) -> Option<String> {
        for args in [["version"].as_slice(), ["--version"].as_slice()] {
            let output = Command::new(ops).args(args).output().ok()?;
            if !output.status.success() {
                continue;
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if let Some(line) = stdout
                .lines()
                .chain(stderr.lines())
                .map(str::trim)
                .find(|l| !l.is_empty())
            {
                return Some(line.to_string());
            }
        }
        None
    }

    fn recheck_version(&self) -> Result<String> {
        Self::run_version(&self.ops).ok_or_else(|| {
            nanovm_unavailable(
                "start",
                format!("ops version failed ({})", self.ops.display()),
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
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for OpsNanoVmClient {
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
            if boot::nanovm_boot_enabled() {
                let child = boot::spawn_ops(&self.ops, plan)?;
                let mut guard = self.guest.lock().map_err(|_| {
                    PhenoError::unsupported_platform(
                        codes::SANDBOX_NANOVM_STUB,
                        "OpsNanoVmClient guest mutex poisoned",
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
                "OpsNanoVmClient::exec requires start() first".into(),
            ));
        }
        let mut guard = self.guest.lock().map_err(|_| {
            PhenoError::unsupported_platform(
                codes::SANDBOX_NANOVM_STUB,
                "OpsNanoVmClient guest mutex poisoned",
            )
        })?;
        exec::exec_cmd_on_guest("OpsNanoVmClient", &mut guard, cmd)
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        if !self.started.load(Ordering::Relaxed) {
            return Err(PhenoError::BadRequest(
                "OpsNanoVmClient::resource_usage requires start() first".into(),
            ));
        }
        Err(resource_probe_not_wired("resource_usage"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded sandbox nanovm event: {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl NanoVmBackend for OpsNanoVmClient {
    fn nanovm_ready(&self) -> bool {
        true
    }
}
