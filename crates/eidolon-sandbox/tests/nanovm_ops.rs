//! nanoVMs / ops feature-gated tests (A+ T2 Phase 3).
//!
//! Default `cargo test --locked` stays green without a NanoVMs install.
//! Hermetic start requires `--features sandbox-nanovm` and system `ops`.
//! Live guest spawn: `NANOVM_INTEGRATION=1` / `UNIKERNEL_BOOT_INTEGRATION=1`
//! with a [`LaunchPlan`]. Guest serial exec via [`unikernel::exec`] when a
//! live child is held (`UNIKERNEL_EXEC_INTEGRATION=1`).

use eidolon_core::error::PhenoError;
use eidolon_sandbox::codes;
use eidolon_sandbox::{nanovm_probe, NanoVmBackend, NanoVmStub};
use eidolon_core::traits::SandboxAutomator;

fn assert_nanovm_stub(err: PhenoError) {
    assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_NANOVM_STUB));
    assert_eq!(err.status_code(), 501);
}

#[tokio::test]
async fn stub_always_fail_loud_without_feature_path() {
    let stub = NanoVmStub::new("nano-probe");
    assert!(!stub.nanovm_ready());
    assert_nanovm_stub(stub.start().await.unwrap_err());
}

#[test]
fn probe_ops_consistency() {
    let ready = nanovm_probe::ops_ready();
    assert_eq!(ready, nanovm_probe::resolve_ops_cli().is_some());
    if nanovm_probe::ops_cli_ready() {
        assert!(ready, "CLI version-ready implies ops_ready");
        assert!(nanovm_probe::ops_version_line().is_some());
    }
}

#[cfg(feature = "sandbox-nanovm")]
mod with_feature {
    use super::*;
    use eidolon_sandbox::{
        unikernel_boot, LaunchPlan, OpsNanoVmClient, RootfsConfig, RootfsFormat,
        UnikernelBackend, UnikernelLaunchConfig,
    };
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_file(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "eidolon-nano-it-{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut f = std::fs::File::create(&path).expect("create");
        writeln!(f, "nano-fixture").expect("write");
        path
    }

    #[tokio::test]
    async fn try_new_matches_ops_cli() {
        match OpsNanoVmClient::try_new("eidolon-t2-nano") {
            Ok(client) => {
                assert!(client.nanovm_ready());
                assert!(nanovm_probe::ops_ready());
                assert!(!client.cli_version().is_empty());
                let meta = client.get_metadata().await.expect("metadata");
                assert_eq!(meta.id, "eidolon-t2-nano");
                assert_eq!(meta.image, OpsNanoVmClient::DEFAULT_IMAGE);
                client.start().await.expect("hermetic start");
                assert!(!client.guest_spawned());
                assert_nanovm_stub(client.exec("echo hi").await.unwrap_err());
                client.stop().await.expect("stop");
            }
            Err(err) => {
                assert!(!nanovm_probe::ops_cli_ready());
                assert_nanovm_stub(err);
            }
        }
    }

    #[test]
    fn try_from_plan_composes_ops_argv() {
        let rootfs = temp_file("ops-plan-rootfs");
        let cfg = UnikernelLaunchConfig::try_new(
            "eidolon-ops-plan",
            UnikernelBackend::NanoVm,
            RootfsConfig::with_format(&rootfs, RootfsFormat::OpsPackage),
        )
        .unwrap()
        .with_resources(1, 128)
        .unwrap();

        match LaunchPlan::try_from_config(&cfg) {
            Ok(plan) => {
                let argv = plan
                    .ops_run_argv(std::path::Path::new("/usr/local/bin/ops"))
                    .expect("argv");
                assert!(argv.iter().any(|a| a == "run"));
                match OpsNanoVmClient::try_from_plan(plan) {
                    Ok(client) => assert!(client.plan().is_some()),
                    Err(err) => assert_nanovm_stub(err),
                }
            }
            Err(err) => {
                assert_eq!(
                    err.unsupported_code(),
                    Some(codes::SANDBOX_UNIKERNEL_STUB)
                );
            }
        }
        let _ = std::fs::remove_file(&rootfs);
    }

    #[tokio::test]
    async fn integration_env_plan_boot_or_fail_loud() {
        if !unikernel_boot::nanovm_boot_enabled() {
            return;
        }
        let rootfs = temp_file("ops-live-rootfs");
        let cfg = UnikernelLaunchConfig::try_new(
            "eidolon-t2-nano-live",
            UnikernelBackend::NanoVm,
            RootfsConfig::new(&rootfs),
        )
        .unwrap();
        let plan = LaunchPlan::try_from_config(&cfg)
            .expect("NANOVM_INTEGRATION=1 requires ops + rootfs");
        let client = OpsNanoVmClient::try_from_plan(plan)
            .expect("NANOVM_INTEGRATION=1 requires OpsNanoVmClient");
        match client.start().await {
            Ok(()) => {
                let _ = client.stop().await;
            }
            Err(err) => {
                assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_NANOVM_STUB));
            }
        }
        let _ = std::fs::remove_file(&rootfs);
    }
}
