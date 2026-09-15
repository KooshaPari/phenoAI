//! KVM / Firecracker backend (A+ T2 / bare-cua Phase 3).
//!
//! # Status
//!
//! - Always-on: [`KvmBackend`] trait, fail-loud [`KvmStub`], and [`probe`]
//!   helpers that detect `/dev/kvm` and a system `firecracker` CLI without
//!   launching microVMs.
//! - Feature `sandbox-kvm`: [`FirecrackerKvmClient`] — live when `firecracker`
//!   is on `PATH` (or `EIDOLON_FIRECRACKER`). `try_new` / hermetic `start` run
//!   `firecracker --version`. Compose [`crate::unikernel::LaunchPlan`] via
//!   [`FirecrackerKvmClient::try_from_plan`]; env-gated live spawn
//!   (`FIRECRACKER_INTEGRATION=1` / `UNIKERNEL_BOOT_INTEGRATION=1`). Guest
//!   `exec` remains fail-loud. Absent tools →
//!   [`PhenoError::UnsupportedPlatform`] with [`codes::SANDBOX_KVM_STUB`].
//!
//! Shared rootfs / launch types live in [`crate::unikernel`] (compose; do not
//! duplicate VirtualStage).
//!
//! Do **not** unarchive KDesktopVirt for routine work; copy patterns only in a
//! scoped extract PR.
//!
//! # Extraction targets
//!
//! | Source | Destination | Notes |
//! |---|---|---|
//! | KDesktopVirt `src/virtualization.rs` | this module / [`FirecrackerKvmClient`] | behind `sandbox-kvm` |
//! | PlayCua VM session patterns | composition via `PlayCuaDispatcher` | keep dispatcher real |
//!
//! See `docs/EXTRACTION_PLAN.md` Phase 3 and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};

#[cfg(feature = "sandbox-kvm")]
mod firecracker_backend;
#[cfg(feature = "sandbox-kvm")]
pub use firecracker_backend::FirecrackerKvmClient;

/// Trait hooks for a KVM / Firecracker backend.
///
/// Prefer [`FirecrackerKvmClient`] when the `sandbox-kvm` feature is enabled and
/// [`probe::firecracker_ready`] is true. Otherwise use [`KvmStub`] (fail-loud).
#[async_trait::async_trait]
pub trait KvmBackend: SandboxAutomator {
    /// Whether a KVM hypervisor path is wired and ready.
    fn kvm_ready(&self) -> bool {
        false
    }

    /// Whether a Firecracker / microVM CLI path is wired.
    fn firecracker_ready(&self) -> bool {
        false
    }
}

/// Probe helpers for KVM device + Firecracker CLI (no bundled binaries).
pub mod probe {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Env override for the Firecracker binary path (`EIDOLON_FIRECRACKER`).
    pub const FIRECRACKER_PATH_ENV: &str = "EIDOLON_FIRECRACKER";

    /// Candidate Firecracker-family CLI names.
    pub const CANDIDATE_BINS: &[&str] = &["firecracker", "jailer"];

    /// Default KVM device path on Linux.
    pub const KVM_DEVICE: &str = "/dev/kvm";

    /// Resolve `firecracker` (preferred) or `jailer` via env / `PATH`.
    pub fn resolve_firecracker_cli() -> Option<PathBuf> {
        if let Ok(override_path) = std::env::var(FIRECRACKER_PATH_ENV) {
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

    /// `true` when [`resolve_firecracker_cli`] finds a runnable binary.
    pub fn firecracker_ready() -> bool {
        resolve_firecracker_cli().is_some()
    }

    /// `/dev/kvm` exists (Linux host with KVM).
    pub fn kvm_device_present() -> bool {
        Path::new(KVM_DEVICE).exists()
    }

    /// Host looks capable of a live Firecracker path (CLI + optional KVM device).
    ///
    /// CLI alone is enough for hermetic version probes; KVM device is required
    /// for actual microVM launch (future `KVM_INTEGRATION` path).
    pub fn kvm_host_ready() -> bool {
        firecracker_ready()
    }

    /// First line of `firecracker --version`, if available.
    pub fn firecracker_version_line() -> Option<String> {
        let bin = resolve_firecracker_cli()?;
        let output = Command::new(&bin).arg("--version").output().ok()?;
        if !output.status.success() {
            // Some builds print version on stderr or exit non-zero with text.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout
                .lines()
                .chain(stderr.lines())
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(str::to_string);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    /// `firecracker --version` yields a non-empty line.
    pub fn firecracker_cli_ready() -> bool {
        firecracker_version_line().is_some()
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

/// Fail-loud KVM / Firecracker backend stub — default when `sandbox-kvm` is off
/// or no Firecracker CLI is available.
#[derive(Debug, Default, Clone)]
pub struct KvmStub {
    sandbox_id: String,
}

impl KvmStub {
    pub fn new(sandbox_id: &str) -> Self {
        Self {
            sandbox_id: sandbox_id.to_string(),
        }
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::SANDBOX_KVM_STUB,
            format!(
                "KvmBackend::{method} not implemented — enable feature \
                 `sandbox-kvm` and install system `firecracker` \
                 (EIDOLON_FIRECRACKER override; Linux `/dev/kvm` for launch), \
                 or extract remaining KDesktopVirt `src/virtualization.rs` \
                 (docs/EXTRACTION_PLAN.md Phase 3; do not unarchive routinely)"
            ),
        )
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for KvmStub {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: self.sandbox_id.clone(),
            image: "stub:kvm".to_string(),
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
        log::debug!("Recorded event (kvm stub): {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl KvmBackend for KvmStub {
    fn kvm_ready(&self) -> bool {
        false
    }

    fn firecracker_ready(&self) -> bool {
        // Stub never claims readiness even if firecracker is on PATH.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_fail_loud() {
        let stub = KvmStub::new("kvm-ut");
        assert!(!stub.kvm_ready());
        assert!(!stub.firecracker_ready());
        let err = stub.start().await.unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_KVM_STUB));
        assert_eq!(err.status_code(), 501);
    }

    #[test]
    fn probe_kvm_consistency() {
        let ready = probe::kvm_host_ready();
        assert_eq!(ready, probe::firecracker_ready());
        assert_eq!(
            probe::firecracker_ready(),
            probe::resolve_firecracker_cli().is_some()
        );
        // Device presence is independent of CLI (macOS CI has neither).
        let _ = probe::kvm_device_present();
        if probe::firecracker_cli_ready() {
            assert!(probe::firecracker_ready());
            assert!(probe::firecracker_version_line().is_some());
        }
    }
}
