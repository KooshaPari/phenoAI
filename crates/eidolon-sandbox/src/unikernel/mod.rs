//! Shared unikernel / microVM rootfs scaffolding + plan→backend boot compose.
//!
//! # Status
//!
//! - Always-on: [`RootfsConfig`], [`KernelConfig`], [`UnikernelLaunchConfig`],
//!   validation helpers, and [`probe`] path resolution (`EIDOLON_ROOTFS` /
//!   `EIDOLON_KERNEL` + explicit paths). Missing artifacts → fail-loud codes
//!   [`codes::SANDBOX_ROOTFS_MISSING`] / [`codes::SANDBOX_KERNEL_MISSING`].
//! - Always-on: [`LaunchPlan`] composes with [`crate::nanovm::probe`] /
//!   [`crate::kvm::probe`] (no duplicated `VirtualStage`).
//! - Always-on: [`boot`] plan→Firecracker/`ops` argv + config JSON (macOS-safe
//!   unit tests). Live spawn is env-gated
//!   (`UNIKERNEL_BOOT_INTEGRATION=1` / `FIRECRACKER_INTEGRATION=1` /
//!   `NANOVM_INTEGRATION=1`).
//! - Always-on: [`exec`] guest I/O types + serial/stdio builders; live
//!   destructive writes env-gated (`UNIKERNEL_EXEC_INTEGRATION=1`). Vsock
//!   NDJSON framing is always-on ([`vsock`]); live AF_VSOCK needs feature
//!   `sandbox-vsock` + Linux + `UNIKERNEL_VSOCK_INTEGRATION=1` (macOS
//!   fail-loud [`codes::SANDBOX_GUEST_IO_UNAVAILABLE`]). In-guest agent
//!   framing/handler always-on ([`vsock_agent`]); live listen + bin behind
//!   `sandbox-vsock-agent` (see `docs/guides/vsock-guest-agent.md`).
//! - Always-on: [`pack`] hermetic image packaging
//!   ([`pack::PackageManifest`], [`pack::HermeticPackBuilder`]) — validate
//!   inputs, optional stage copy/link, write manifest JSON. Live `mkfs` /
//!   `virt-make-fs` / `docker export` / compose `DockerToExt4` pipelines
//!   behind feature `sandbox-rootfs-pack` + `ROOTFS_PACK_INTEGRATION=1`
//!   (tools missing → [`codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING`]; command
//!   failure → [`codes::SANDBOX_ROOTFS_PACK_IO`]). DockerExport alone is
//!   always Raw; Ext4 disks require mkfs/virt-make-fs or `DockerToExt4`
//!   (`docs/reference/rootfs-pack.md`). Optional
//!   [`pack::bake_agent`] / [`pack::bake_agent_then_pack`] stages
//!   `eidolon-vsock-agent` into the tree before pack (prefer
//!   `EIDOLON_VSOCK_AGENT`; missing → [`codes::SANDBOX_VSOCK_AGENT_MISSING`]).
//!   [`pack::canned`] / [`pack::build_canned_rootfs`] produces a durable
//!   canned tree (+ optional live Ext4) under `EIDOLON_CANNED_ROOTFS_OUT` /
//!   `target/canned-rootfs/` (`docs/guides/canned-rootfs.md`).
//! - Feature `sandbox-unikernel`: [`UnikernelGuestClient`] — hermetic
//!   construct/start validates rootfs (+ optional kernel) and host CLI; when
//!   boot env is set, spawns Firecracker/`ops` via [`boot`]. Guest `exec`
//!   uses serial/stdio when a live child is held; resource probes remain
//!   fail-loud ([`codes::SANDBOX_UNIKERNEL_STUB`]).
//!
//! Do **not** unarchive KDesktopVirt for routine work; copy patterns only in a
//! scoped extract PR. Do not re-implement `VirtualStage` here — compose
//! sandbox backends.
//!
//! See `docs/EXTRACTION_PLAN.md` (unikernel / microVM guest launch + rootfs).

pub mod boot;
pub mod exec;
pub mod pack;
pub mod probe;
pub mod vsock;
pub mod vsock_agent;

mod unikernel_config;
mod unikernel_runtime;

pub use unikernel_config::*;
#[cfg(feature = "sandbox-unikernel")]
pub use unikernel_runtime::UnikernelGuestClient;
pub use unikernel_runtime::UnikernelStub;

#[cfg(test)]
mod tests {
    use std::io::Write;

    use eidolon_core::error::PhenoError;
    use eidolon_core::traits::SandboxAutomator;

    use super::*;
    use crate::codes;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "eidolon-unikernel-{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut f = std::fs::File::create(&path).expect("create temp");
        writeln!(f, "eidolon-rootfs-fixture").expect("write");
        path
    }

    #[test]
    fn rootfs_empty_path_bad_request() {
        let cfg = RootfsConfig::new("   ");
        let err = cfg.validate_path().unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn rootfs_missing_fail_loud() {
        let cfg = RootfsConfig::new("/nonexistent/eidolon/rootfs.ext4");
        let err = cfg.require_present().unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_ROOTFS_MISSING));
        assert_eq!(err.status_code(), 501);
    }

    #[test]
    fn kernel_missing_fail_loud() {
        let cfg = KernelConfig::new("/nonexistent/eidolon/vmlinux");
        let err = cfg.require_present().unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_KERNEL_MISSING));
    }

    #[test]
    fn rootfs_present_ok() {
        let path = temp_file("rootfs");
        let cfg = RootfsConfig::with_format(&path, RootfsFormat::Ext4);
        cfg.require_present().expect("present");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn launch_config_rejects_zero_resources() {
        let path = temp_file("rootfs-res");
        let cfg = UnikernelLaunchConfig::try_new(
            "uk-1",
            UnikernelBackend::Firecracker,
            RootfsConfig::new(&path),
        )
        .unwrap();
        assert!(cfg.clone().with_resources(0, 128).is_err());
        assert!(cfg.with_resources(1, 0).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn launch_plan_missing_rootfs_code() {
        let cfg = UnikernelLaunchConfig::try_new(
            "uk-miss",
            UnikernelBackend::Firecracker,
            RootfsConfig::new("/no/such/eidolon-rootfs.img"),
        )
        .unwrap();
        let err = LaunchPlan::try_from_config(&cfg).unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_ROOTFS_MISSING));
    }

    #[test]
    fn launch_plan_with_rootfs_requires_backend_cli() {
        let path = temp_file("rootfs-plan");
        let cfg = UnikernelLaunchConfig::try_new(
            "uk-plan",
            UnikernelBackend::Firecracker,
            RootfsConfig::new(&path),
        )
        .unwrap();
        match LaunchPlan::try_from_config(&cfg) {
            Ok(plan) => {
                assert!(plan.backend_cli_ready());
                assert_eq!(plan.rootfs, path);
                plan.require_ready().expect("ready");
            }
            Err(err) => {
                // CI without firecracker: fail-loud unikernel stub, not silent Ok.
                assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_UNIKERNEL_STUB));
            }
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn probe_explicit_path() {
        let path = temp_file("probe");
        assert_eq!(
            probe::resolve_rootfs(Some(path.as_path())),
            Some(path.clone())
        );
        assert!(probe::rootfs_ready(Some(path.as_path())));
        assert!(!probe::rootfs_ready(Some(std::path::Path::new(
            "/no/eidolon/rootfs"
        ))));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn stub_fail_loud() {
        let stub = UnikernelStub::new("uk-stub");
        let err = stub.start().await.unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_UNIKERNEL_STUB));
        assert_eq!(err.status_code(), 501);
    }
}
