//! Unikernel / microVM rootfs scaffolding + plan→boot argv + guest exec tests.
//!
//! Default `cargo test --locked` stays green without Firecracker, ops, or a
//! real rootfs. Feature `sandbox-unikernel` exercises
//! [`UnikernelGuestClient`] when artifacts + host CLI are present.
//! Live guest process spawn requires `UNIKERNEL_BOOT_INTEGRATION=1` (or
//! `FIRECRACKER_INTEGRATION=1` / `NANOVM_INTEGRATION=1`). Live serial exec
//! requires a live child + `UNIKERNEL_EXEC_INTEGRATION=1`.

use eidolon_core::error::PhenoError;
use eidolon_core::traits::SandboxAutomator;
use eidolon_sandbox::codes;
use eidolon_sandbox::{
    unikernel_boot, unikernel_exec, unikernel_probe, KernelConfig, LaunchPlan, RootfsConfig,
    RootfsFormat, UnikernelBackend, UnikernelLaunchConfig, UnikernelStub,
};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

fn temp_file(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "eidolon-uk-it-{}-{}-{}",
        name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let mut f = std::fs::File::create(&path).expect("create temp");
    writeln!(f, "eidolon-unikernel-integration-fixture").expect("write");
    path
}

fn assert_code(err: PhenoError, expected: &str) {
    assert_eq!(err.unsupported_code(), Some(expected));
    assert_eq!(err.status_code(), 501);
}

#[tokio::test]
async fn unikernel_stub_fail_loud() {
    let stub = UnikernelStub::new("uk-it");
    let meta = stub.get_metadata().await.unwrap();
    assert_eq!(meta.image, "stub:unikernel");
    assert_code(stub.start().await.unwrap_err(), codes::SANDBOX_UNIKERNEL_STUB);
}

#[test]
fn path_validation_empty_and_missing() {
    let empty = RootfsConfig::new("");
    assert!(matches!(empty.validate_path(), Err(PhenoError::BadRequest(_))));

    let missing = RootfsConfig::new("/nonexistent/eidolon/rootfs-it.ext4");
    assert_code(
        missing.require_present().unwrap_err(),
        codes::SANDBOX_ROOTFS_MISSING,
    );

    let kern = KernelConfig::new("/nonexistent/eidolon/vmlinux-it");
    assert_code(
        kern.require_present().unwrap_err(),
        codes::SANDBOX_KERNEL_MISSING,
    );
}

#[test]
fn probe_explicit_and_consistency() {
    let path = temp_file("probe-it");
    assert!(unikernel_probe::rootfs_ready(Some(path.as_path())));
    assert_eq!(
        unikernel_probe::resolve_rootfs(Some(path.as_path())),
        Some(path.clone())
    );
    assert!(!unikernel_probe::rootfs_ready(Some(Path::new(
        "/no/eidolon/rootfs-it"
    ))));
    let _ = unikernel_probe::kernel_ready(None);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn launch_plan_missing_rootfs() {
    let cfg = UnikernelLaunchConfig::try_new(
        "uk-plan-miss",
        UnikernelBackend::NanoVm,
        RootfsConfig::with_format("/no/such/rootfs.img", RootfsFormat::OpsPackage),
    )
    .unwrap();
    assert_code(
        LaunchPlan::try_from_config(&cfg).unwrap_err(),
        codes::SANDBOX_ROOTFS_MISSING,
    );
}

#[test]
fn launch_plan_to_firecracker_argv_and_config() {
    // Direct LaunchPlan (no CLI) — macOS-safe plan→backend argv unit test.
    let rootfs = temp_file("plan-fc-rootfs");
    let kernel = temp_file("plan-fc-kernel");
    let plan = LaunchPlan {
        sandbox_id: "uk-argv".into(),
        backend: UnikernelBackend::Firecracker,
        rootfs: rootfs.clone(),
        rootfs_format: RootfsFormat::Ext4,
        kernel: Some(kernel.clone()),
        boot_args: "console=ttyS0".into(),
        vcpu_count: 2,
        memory_mib: 256,
        policy: eidolon_core::security::SandboxPolicy::default(),
    };
    let cfg_path = Path::new("/tmp/eidolon-fc-test.json");
    let argv = plan.firecracker_argv(Path::new("/usr/bin/firecracker"), cfg_path);
    assert_eq!(
        argv,
        vec![
            OsString::from("/usr/bin/firecracker"),
            OsString::from("--no-api"),
            OsString::from("--config-file"),
            OsString::from("/tmp/eidolon-fc-test.json"),
        ]
    );
    let json = plan.firecracker_config_json().expect("json");
    assert!(json.contains(kernel.to_str().unwrap()));
    assert!(json.contains(rootfs.to_str().unwrap()));
    assert!(json.contains("\"vcpu_count\": 2"));
    assert!(json.contains("console=ttyS0"));
    let _ = std::fs::remove_file(&rootfs);
    let _ = std::fs::remove_file(&kernel);
}

#[test]
fn launch_plan_to_ops_argv() {
    let rootfs = temp_file("plan-ops-rootfs");
    let plan = LaunchPlan {
        sandbox_id: "uk-ops-argv".into(),
        backend: UnikernelBackend::NanoVm,
        rootfs: rootfs.clone(),
        rootfs_format: RootfsFormat::OpsPackage,
        kernel: None,
        boot_args: String::new(),
        vcpu_count: 1,
        memory_mib: 128,
        policy: eidolon_core::security::SandboxPolicy::default(),
    };
    let argv = plan
        .ops_run_argv(Path::new("/usr/local/bin/ops"))
        .expect("ops argv");
    assert_eq!(
        argv,
        vec![
            OsString::from("/usr/local/bin/ops"),
            OsString::from("run"),
            rootfs.as_os_str().to_os_string(),
            OsString::from("-m"),
            OsString::from("128"),
            OsString::from("-c"),
            OsString::from("1"),
        ]
    );
    let _ = std::fs::remove_file(&rootfs);
}

#[test]
fn firecracker_config_missing_kernel_fail_loud() {
    let rootfs = temp_file("nok-rootfs");
    let plan = LaunchPlan {
        sandbox_id: "uk-nok".into(),
        backend: UnikernelBackend::Firecracker,
        rootfs: rootfs.clone(),
        rootfs_format: RootfsFormat::Ext4,
        kernel: None,
        boot_args: String::new(),
        vcpu_count: 1,
        memory_mib: 128,
        policy: eidolon_core::security::SandboxPolicy::default(),
    };
    assert_code(
        unikernel_boot::firecracker_config_json(&plan).unwrap_err(),
        codes::SANDBOX_KERNEL_MISSING,
    );
    let _ = std::fs::remove_file(&rootfs);
}

#[test]
fn launch_plan_rootfs_present_composes_backend_cli() {
    let path = temp_file("plan-it");
    let cfg = UnikernelLaunchConfig::try_new(
        "uk-plan-ok",
        UnikernelBackend::Firecracker,
        RootfsConfig::with_format(&path, RootfsFormat::Ext4),
    )
    .unwrap()
    .with_resources(2, 256)
    .unwrap();

    match LaunchPlan::try_from_config(&cfg) {
        Ok(plan) => {
            assert_eq!(plan.vcpu_count, 2);
            assert_eq!(plan.memory_mib, 256);
            assert!(plan.backend_cli_ready());
            plan.require_ready().expect("artifacts+cli");
        }
        Err(err) => {
            assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_UNIKERNEL_STUB));
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn guest_exec_builders_mac_safe() {
    let frame = unikernel_exec::serial_stdin_frame("uname -a");
    assert_eq!(frame, b"uname -a\n");
    let req = unikernel_exec::GuestExecRequest::serial("pwd").expect("req");
    assert_eq!(
        req.transport,
        unikernel_exec::GuestIoTransport::SerialStdio
    );
    let vsock_frame = eidolon_sandbox::unikernel_vsock::vsock_exec_request_frame("pwd").unwrap();
    assert!(vsock_frame.ends_with(b"\n"));
    let result = unikernel_exec::vsock_connect_spec(3, 52);
    #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
    {
        assert_eq!(result.unwrap(), (3, 52));
    }
    #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
    {
        assert_code(result.unwrap_err(), codes::SANDBOX_GUEST_IO_UNAVAILABLE);
    }
    let mut none = None;
    let err = unikernel_exec::require_live_guest("ItClient", &mut none).unwrap_err();
    assert_code(err, codes::SANDBOX_GUEST_NOT_RUNNING);
}

#[cfg(feature = "sandbox-unikernel")]
mod feature_tests {
    use super::*;
    use eidolon_sandbox::UnikernelGuestClient;

    #[tokio::test]
    async fn guest_client_hermetic_or_fail_loud() {
        let path = temp_file("guest");
        let cfg = UnikernelLaunchConfig::try_new(
            "uk-guest",
            UnikernelBackend::Firecracker,
            RootfsConfig::new(&path),
        )
        .unwrap();

        match UnikernelGuestClient::try_new(cfg) {
            Ok(client) => {
                assert!(client.host_cli_ready());
                if unikernel_boot::boot_integration_enabled() {
                    // Live spawn may fail without real kernel/KVM — fail-loud OK.
                    let _ = client.start().await;
                } else {
                    client.start().await.expect("hermetic start");
                    assert!(
                        !client.guest_spawned(),
                        "hermetic start must not spawn without boot env"
                    );
                    // Hermetic start: no live child → GUEST_NOT_RUNNING.
                    let err = client.exec("uname").await.unwrap_err();
                    assert_eq!(
                        err.unsupported_code(),
                        Some(codes::SANDBOX_GUEST_NOT_RUNNING)
                    );
                }
                let _ = client.stop().await;
            }
            Err(err) => {
                // Missing firecracker on PATH — fail-loud, not silent Ok.
                assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_UNIKERNEL_STUB));
            }
        }
        let _ = std::fs::remove_file(&path);
    }
}
