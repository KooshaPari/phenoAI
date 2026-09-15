//! Cross-target scaffolding tests for Docker/nanoVM/KVM hooks.
//!
//! These compile and run on **all** platforms so CI/local stays green while
//! asserting fail-loud contracts for sandbox stubs. [`PlayCuaDispatcher`]
//! remains real (covered in unit tests); this file covers backend stubs only.
//! Live paths: `tests/docker_bollard.rs` (`sandbox-docker`),
//! `tests/nanovm_ops.rs` (`sandbox-nanovm`), `tests/kvm_firecracker.rs`
//! (`sandbox-kvm`).

use eidolon_core::error::PhenoError;
use eidolon_core::traits::SandboxAutomator;
use eidolon_sandbox::codes;
use eidolon_sandbox::{
    code_for_backend, docker_probe, kvm_probe, nanovm_probe, ContainerConfig, DockerOrchestrator,
    KvmBackend, KvmStub, NanoVmBackend, NanoVmStub, StubDockerOrchestrator,
};

fn assert_code(err: PhenoError, expected: &str) {
    assert_eq!(err.unsupported_code(), Some(expected));
    assert_eq!(err.status_code(), 501);
    match err {
        PhenoError::UnsupportedPlatform { code, message } => {
            assert_eq!(code, expected);
            assert!(!message.is_empty());
        }
        other => panic!("expected UnsupportedPlatform, got {other:?}"),
    }
}

#[tokio::test]
async fn docker_stub_fail_loud() {
    let stub = StubDockerOrchestrator::new();
    assert!(!stub.docker_ready());

    assert_code(
        stub.start_container(
            "ubuntu:latest",
            ContainerConfig {
                cpu_limit: 1.0,
                memory_limit_mb: 512,
                disk_limit_mb: None,
                ports: vec![],
            },
        )
        .await
        .unwrap_err(),
        codes::SANDBOX_DOCKER_STUB,
    );
    assert_code(
        stub.stop_container("c1").await.unwrap_err(),
        codes::SANDBOX_DOCKER_STUB,
    );
    assert_code(
        stub.exec("c1", "echo hi").await.unwrap_err(),
        codes::SANDBOX_DOCKER_STUB,
    );
    assert_code(
        stub.get_resource_usage("c1").await.unwrap_err(),
        codes::SANDBOX_DOCKER_STUB,
    );
}

#[test]
fn docker_probe_is_boolean() {
    let _ = docker_probe::docker_ready();
    let _ = docker_probe::docker_cli_ready();
    let _ = docker_probe::docker_socket_present();
}

#[test]
fn nanovm_probe_is_boolean() {
    let _ = nanovm_probe::ops_ready();
    let _ = nanovm_probe::ops_cli_ready();
    let _ = nanovm_probe::resolve_ops_cli();
}

#[test]
fn kvm_probe_is_boolean() {
    let _ = kvm_probe::kvm_host_ready();
    let _ = kvm_probe::firecracker_ready();
    let _ = kvm_probe::kvm_device_present();
    let _ = kvm_probe::resolve_firecracker_cli();
}

#[tokio::test]
async fn nanovm_stub_fail_loud() {
    let stub = NanoVmStub::new("nano-1");
    assert!(!stub.nanovm_ready());

    let meta = stub.get_metadata().await.unwrap();
    assert_eq!(meta.image, "stub:nanovm");

    assert_code(stub.start().await.unwrap_err(), codes::SANDBOX_NANOVM_STUB);
    assert_code(stub.exec("ls").await.unwrap_err(), codes::SANDBOX_NANOVM_STUB);
    assert_code(
        stub.resource_usage().await.unwrap_err(),
        codes::SANDBOX_NANOVM_STUB,
    );
    assert_code(stub.stop().await.unwrap_err(), codes::SANDBOX_NANOVM_STUB);
}

#[tokio::test]
async fn kvm_stub_fail_loud() {
    let stub = KvmStub::new("kvm-1");
    assert!(!stub.kvm_ready());
    assert!(!stub.firecracker_ready());

    let meta = stub.get_metadata().await.unwrap();
    assert_eq!(meta.image, "stub:kvm");

    assert_code(stub.start().await.unwrap_err(), codes::SANDBOX_KVM_STUB);
    assert_code(stub.exec("uname").await.unwrap_err(), codes::SANDBOX_KVM_STUB);
    assert_code(
        stub.resource_usage().await.unwrap_err(),
        codes::SANDBOX_KVM_STUB,
    );
    assert_code(stub.stop().await.unwrap_err(), codes::SANDBOX_KVM_STUB);
}

#[test]
fn code_for_backend_mapping() {
    assert_eq!(code_for_backend("docker"), codes::SANDBOX_DOCKER_STUB);
    assert_eq!(code_for_backend("nanovm"), codes::SANDBOX_NANOVM_STUB);
    assert_eq!(code_for_backend("ops"), codes::SANDBOX_NANOVM_STUB);
    assert_eq!(code_for_backend("kvm"), codes::SANDBOX_KVM_STUB);
    assert_eq!(code_for_backend("firecracker"), codes::SANDBOX_KVM_STUB);
    assert_eq!(code_for_backend("unikernel"), codes::SANDBOX_UNIKERNEL_STUB);
    assert_eq!(code_for_backend("rootfs"), codes::SANDBOX_UNIKERNEL_STUB);
    assert_eq!(code_for_backend("microvm"), codes::SANDBOX_UNIKERNEL_STUB);
    assert_eq!(code_for_backend("rootfs-pack"), codes::SANDBOX_ROOTFS_PACK_STUB);
    assert_eq!(code_for_backend("pack"), codes::SANDBOX_ROOTFS_PACK_STUB);
    assert_eq!(code_for_backend("weird"), codes::SANDBOX_OTHER_STUB);
}

#[test]
fn documented_codes_are_stable() {
    assert_eq!(codes::SANDBOX_DOCKER_STUB, "EIDOLON_SANDBOX_DOCKER_STUB");
    assert_eq!(codes::SANDBOX_NANOVM_STUB, "EIDOLON_SANDBOX_NANOVM_STUB");
    assert_eq!(codes::SANDBOX_KVM_STUB, "EIDOLON_SANDBOX_KVM_STUB");
    assert_eq!(codes::SANDBOX_OTHER_STUB, "EIDOLON_SANDBOX_OTHER_STUB");
    assert_eq!(
        codes::SANDBOX_SESSION_BACKEND,
        "EIDOLON_SANDBOX_SESSION_BACKEND"
    );
    assert_eq!(codes::SANDBOX_SESSION_IO, "EIDOLON_SANDBOX_SESSION_IO");
    assert_eq!(
        codes::SANDBOX_AUDIT_BACKEND,
        "EIDOLON_SANDBOX_AUDIT_BACKEND"
    );
    assert_eq!(codes::SANDBOX_AUDIT_IO, "EIDOLON_SANDBOX_AUDIT_IO");
    assert_eq!(
        codes::SANDBOX_LANDLOCK_UNSUPPORTED,
        "EIDOLON_SANDBOX_LANDLOCK_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_CGROUP_UNSUPPORTED,
        "EIDOLON_SANDBOX_CGROUP_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_CGROUP_DISK_UNAVAILABLE,
        "EIDOLON_SANDBOX_CGROUP_DISK_UNAVAILABLE"
    );
    assert_eq!(codes::SANDBOX_ROOTFS_MISSING, "EIDOLON_SANDBOX_ROOTFS_MISSING");
    assert_eq!(
        codes::SANDBOX_KERNEL_MISSING,
        "EIDOLON_SANDBOX_KERNEL_MISSING"
    );
    assert_eq!(
        codes::SANDBOX_UNIKERNEL_STUB,
        "EIDOLON_SANDBOX_UNIKERNEL_STUB"
    );
    assert_eq!(
        codes::SANDBOX_ROOTFS_PACK_STUB,
        "EIDOLON_SANDBOX_ROOTFS_PACK_STUB"
    );
    assert_eq!(
        codes::SANDBOX_ROOTFS_PACK_TOOL_MISSING,
        "EIDOLON_SANDBOX_ROOTFS_PACK_TOOL_MISSING"
    );
    assert_eq!(
        codes::SANDBOX_VSOCK_AGENT_MISSING,
        "EIDOLON_SANDBOX_VSOCK_AGENT_MISSING"
    );
    assert_eq!(
        codes::SANDBOX_ENFORCEMENT_DISABLED,
        "EIDOLON_SANDBOX_ENFORCEMENT_DISABLED"
    );
    assert_eq!(
        codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED,
        "EIDOLON_SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_VIRTUAL_DISPLAY_STUB,
        "EIDOLON_SANDBOX_VIRTUAL_DISPLAY_STUB"
    );
    assert_eq!(codes::SANDBOX_XVFB_MISSING, "EIDOLON_SANDBOX_XVFB_MISSING");
    assert_eq!(
        codes::SANDBOX_VNC_UNSUPPORTED,
        "EIDOLON_SANDBOX_VNC_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED,
        "EIDOLON_SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED"
    );
}

