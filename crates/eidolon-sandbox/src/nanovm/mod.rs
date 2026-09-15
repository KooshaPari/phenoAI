//! nanoVMs backend (A+ T2 / KVirtualStage Phase 3).
//!
//! # Status
//!
//! - Always-on: [`NanoVmBackend`] trait, fail-loud [`NanoVmStub`], and [`probe`]
//!   helpers that locate a system NanoVMs `ops` CLI without spawning unikernels.
//! - Feature `sandbox-nanovm`: [`OpsNanoVmClient`] — live when `ops` is on
//!   `PATH` (or `EIDOLON_OPS`). `try_new` / hermetic `start` run `ops version`.
//!   Compose [`crate::unikernel::LaunchPlan`] via
//!   [`OpsNanoVmClient::try_from_plan`]; env-gated live spawn
//!   (`NANOVM_INTEGRATION=1` / `UNIKERNEL_BOOT_INTEGRATION=1`). Guest `exec`
//!   remains fail-loud. Absent tools → [`PhenoError::UnsupportedPlatform`]
//!   with [`codes::SANDBOX_NANOVM_STUB`].
//!
//! Shared rootfs / launch types live in [`crate::unikernel`] (compose with
//! [`crate::unikernel::UnikernelLaunchConfig`]; do not duplicate VirtualStage).
//!
//! Do **not** unarchive KDesktopVirt for routine work; copy patterns only in a
//! scoped extract PR.
//!
//! # Extraction targets
//!
//! | Source | Destination | Notes |
//! |---|---|---|
//! | KVirtualStage / KDesktopVirt nanoVM patterns | this module / [`OpsNanoVmClient`] | behind `sandbox-nanovm` |
//! | PlayCua session patterns | composition via `PlayCuaDispatcher` | keep dispatcher real |
//!
//! See `docs/EXTRACTION_PLAN.md` Phase 3 and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};

#[cfg(feature = "sandbox-nanovm")]
mod ops_backend;
#[cfg(feature = "sandbox-nanovm")]
pub use ops_backend::OpsNanoVmClient;

/// Trait hooks for a nanoVMs / microVM backend.
///
/// Prefer [`OpsNanoVmClient`] when the `sandbox-nanovm` feature is enabled and
/// [`probe::ops_ready`] is true. Otherwise use [`NanoVmStub`] (fail-loud).
#[async_trait::async_trait]
pub trait NanoVmBackend: SandboxAutomator {
    /// Whether a nanoVMs / microVM runtime is wired and ready.
    fn nanovm_ready(&self) -> bool {
        false
    }
}

/// Probe helpers for a system NanoVMs `ops` CLI (no bundled binaries).
pub mod probe {
    use std::path::PathBuf;
    use std::process::Command;

    /// Env override for the NanoVMs `ops` binary path (`EIDOLON_OPS`).
    pub const OPS_PATH_ENV: &str = "EIDOLON_OPS";

    /// Candidate CLI names (NanoVMs ships `ops`; some installs expose `nanos`).
    pub const CANDIDATE_BINS: &[&str] = &["ops", "nanos"];

    /// Resolve `ops` / `nanos` via `EIDOLON_OPS` or `PATH`.
    pub fn resolve_ops_cli() -> Option<PathBuf> {
        if let Ok(override_path) = std::env::var(OPS_PATH_ENV) {
            let p = PathBuf::from(override_path);
            if p.is_file() {
                return Some(p);
            }
        }
        for name in CANDIDATE_BINS {
            if let Some(p) = which_bin(name) {
                return Some(p);
            }
        }
        None
    }

    /// `true` when [`resolve_ops_cli`] finds a runnable binary.
    pub fn ops_ready() -> bool {
        resolve_ops_cli().is_some()
    }

    /// First line of `ops version` / `ops --version`, if available.
    pub fn ops_version_line() -> Option<String> {
        let bin = resolve_ops_cli()?;
        for args in [["version"].as_slice(), ["--version"].as_slice()] {
            let output = Command::new(&bin).args(args).output().ok()?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let line = stdout
                    .lines()
                    .chain(stderr.lines())
                    .map(str::trim)
                    .find(|l| !l.is_empty())?;
                return Some(line.to_string());
            }
        }
        None
    }

    /// `ops version` succeeds (CLI answers).
    pub fn ops_cli_ready() -> bool {
        ops_version_line().is_some()
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
}

/// Fail-loud nanoVMs backend stub — default when `sandbox-nanovm` is off or
/// no `ops` CLI is available.
#[derive(Debug, Default, Clone)]
pub struct NanoVmStub {
    sandbox_id: String,
}

impl NanoVmStub {
    pub fn new(sandbox_id: &str) -> Self {
        Self {
            sandbox_id: sandbox_id.to_string(),
        }
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::SANDBOX_NANOVM_STUB,
            format!(
                "NanoVmBackend::{method} not implemented — enable feature \
                 `sandbox-nanovm` and install system NanoVMs `ops` \
                 (EIDOLON_OPS override), or extract remaining KVirtualStage \
                 nanoVM patterns (docs/EXTRACTION_PLAN.md Phase 3; do not \
                 unarchive KDesktopVirt routinely)"
            ),
        )
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for NanoVmStub {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: self.sandbox_id.clone(),
            image: "stub:nanovm".to_string(),
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
        log::debug!("Recorded event (nanovm stub): {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl NanoVmBackend for NanoVmStub {
    fn nanovm_ready(&self) -> bool {
        // Stub never claims readiness even if ops is on PATH — callers must
        // use OpsNanoVmClient behind `sandbox-nanovm`.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_fail_loud() {
        let stub = NanoVmStub::new("nano-ut");
        assert!(!stub.nanovm_ready());
        let err = stub.start().await.unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_NANOVM_STUB));
        assert_eq!(err.status_code(), 501);
    }

    #[test]
    fn probe_ops_consistency() {
        let ready = probe::ops_ready();
        assert_eq!(ready, probe::resolve_ops_cli().is_some());
        if probe::ops_cli_ready() {
            assert!(ready);
            assert!(probe::ops_version_line().is_some());
        }
    }
}
