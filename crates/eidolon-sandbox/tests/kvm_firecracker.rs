//! KVM / Firecracker feature-gated tests (A+ T2 Phase 3).
//!
//! Default `cargo test --locked` stays green without Firecracker or `/dev/kvm`.
//! Hermetic start requires `--features sandbox-kvm` and system `firecracker`.
//! Live guest spawn: `FIRECRACKER_INTEGRATION=1` / `UNIKERNEL_BOOT_INTEGRATION=1`
//! with a [`LaunchPlan`]. Guest serial exec via [`unikernel::exec`] when a
//! live child is held (`UNIKERNEL_EXEC_INTEGRATION=1`).

use eidolon_core::error::PhenoError;
use eidolon_core::traits::SandboxAutomator;
use eidolon_sandbox::codes;
use eidolon_sandbox::{kvm_probe, KvmBackend, KvmStub};

fn assert_kvm_stub(err: PhenoError) {
    assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_KVM_STUB));
    assert_eq!(err.status_code(), 501);
}

#[tokio::test]
async fn stub_always_fail_loud_without_feature_path() {
    let stub = KvmStub::new("kvm-probe");
    assert!(!stub.kvm_ready());
    assert!(!stub.firecracker_ready());
    assert_kvm_stub(stub.start().await.unwrap_err());
}

#[test]
fn probe_kvm_consistency() {
    let ready = kvm_probe::kvm_host_ready();
    assert_eq!(ready, kvm_probe::firecracker_ready());
    assert_eq!(
        kvm_probe::firecracker_ready(),
        kvm_probe::resolve_firecracker_cli().is_some()
    );
    let _ = kvm_probe::kvm_device_present();
    if kvm_probe::firecracker_cli_ready() {
        assert!(ready, "CLI version-ready implies kvm_host_ready");
        assert!(kvm_probe::firecracker_version_line().is_some());
    }
}

#[cfg(feature = "sandbox-kvm")]
mod with_feature {
    use super::*;
    use eidolon_sandbox::{
        unikernel_boot, FirecrackerKvmClient, KernelConfig, LaunchPlan, RootfsConfig,
        RootfsFormat, UnikernelBackend, UnikernelLaunchConfig,
    };
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_file(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "eidolon-kvm-it-{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut f = std::fs::File::create(&path).expect("create");
        writeln!(f, "kvm-fixture").expect("write");
        path
    }

    #[tokio::test]
    async fn try_new_matches_firecracker_cli() {
        match FirecrackerKvmClient::try_new("eidolon-t2-kvm") {
            Ok(client) => {
                assert!(client.firecracker_ready());
                assert!(kvm_probe::firecracker_ready());
                assert!(!client.cli_version().is_empty());
                assert_eq!(client.kvm_ready(), client.kvm_device_present());
                let meta = client.get_metadata().await.expect("metadata");
                assert_eq!(meta.id, "eidolon-t2-kvm");
                assert_eq!(meta.image, FirecrackerKvmClient::DEFAULT_IMAGE);
                client.start().await.expect("hermetic start");
                assert!(!client.guest_spawned());
                assert_kvm_stub(client.exec("uname").await.unwrap_err());
                client.stop().await.expect("stop");
            }
            Err(err) => {
                assert!(!kvm_probe::firecracker_cli_ready());
                assert_kvm_stub(err);
            }
        }
    }

    #[test]
    fn try_from_plan_requires_kernel_and_composes_argv() {
        let rootfs = temp_file("fc-plan-rootfs");
        let kernel = temp_file("fc-plan-kernel");
        let cfg = UnikernelLaunchConfig::try_new(
            "eidolon-fc-plan",
            UnikernelBackend::Firecracker,
            RootfsConfig::with_format(&rootfs, RootfsFormat::Ext4),
        )
        .unwrap()
        .with_kernel(KernelConfig::new(&kernel))
        .unwrap()
        .with_resources(2, 256)
        .unwrap();

        match LaunchPlan::try_from_config(&cfg) {
            Ok(plan) => {
                let json = plan.firecracker_config_json().expect("json");
                assert!(json.contains("vcpu_count"));
                match FirecrackerKvmClient::try_from_plan(plan) {
                    Ok(client) => {
                        assert!(client.plan().is_some());
                        assert!(!client.guest_spawned());
                    }
                    Err(err) => assert_kvm_stub(err),
                }
            }
            Err(err) => {
                // CLI absent on macOS CI.
                assert_eq!(
                    err.unsupported_code(),
                    Some(codes::SANDBOX_UNIKERNEL_STUB)
                );
            }
        }
        let _ = std::fs::remove_file(&rootfs);
        let _ = std::fs::remove_file(&kernel);
    }

    #[tokio::test]
    async fn integration_env_plan_boot_or_fail_loud() {
        if !unikernel_boot::firecracker_boot_enabled() {
            return;
        }
        let rootfs = temp_file("fc-live-rootfs");
        let kernel = temp_file("fc-live-kernel");
        let cfg = UnikernelLaunchConfig::try_new(
            "eidolon-t2-kvm-live",
            UnikernelBackend::Firecracker,
            RootfsConfig::new(&rootfs),
        )
        .unwrap()
        .with_kernel(KernelConfig::new(&kernel))
        .unwrap();
        let plan = LaunchPlan::try_from_config(&cfg)
            .expect("FIRECRACKER_INTEGRATION=1 requires firecracker + artifacts");
        let client = FirecrackerKvmClient::try_from_plan(plan)
            .expect("FIRECRACKER_INTEGRATION=1 requires FirecrackerKvmClient");
        // Spawn may fail without real images/KVM — must be fail-loud, not silent Ok spawn.
        match client.start().await {
            Ok(()) => {
                let _ = client.stop().await;
            }
            Err(err) => {
                assert!(
                    err.unsupported_code() == Some(codes::SANDBOX_KVM_STUB)
                        || err.unsupported_code() == Some(codes::SANDBOX_KERNEL_MISSING)
                        || err.unsupported_code() == Some(codes::SANDBOX_UNIKERNEL_STUB)
                );
            }
        }
        let _ = std::fs::remove_file(&rootfs);
        let _ = std::fs::remove_file(&kernel);
    }
}
