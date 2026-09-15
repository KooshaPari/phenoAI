//! Unit tests for eidolon-sandbox.
//!
//! # Two-layer test surface (A+ T0 / T1 honesty)
//!
//! 1. Lifecycle / exec / resource methods on [`SandboxClient`] are **fail-loud**
//!    stubs (`PhenoError::UnsupportedPlatform` + documented codes) after input
//!    validation.
//! 2. Security validation still rejects malformed / injection-shaped input
//!    before the unimplemented backend error.
//! 3. `get_metadata` returns requested-policy stub metadata (`stub:latest`).

use std::sync::Arc;

use eidolon_core::error::PhenoError;
use eidolon_core::event::Platform;
use eidolon_core::security::{
    validate_exec_cmd, validate_sandbox_id, NetworkPolicy, SandboxPolicy, EXEC_CMD_MAX_LEN,
    SANDBOX_ID_MAX_LEN,
};
use eidolon_core::traits::SandboxAutomator;
use eidolon_core::AutomationEvent;
use eidolon_sandbox::{codes, SandboxClient};
use Platform::*;

fn make_client(sandbox_id: &str) -> Arc<dyn SandboxAutomator> {
    Arc::new(SandboxClient::new(sandbox_id).expect("test sandbox_id must validate"))
}

fn assert_unsupported(err: PhenoError, method: &str, expected_code: &str) {
    assert_eq!(err.unsupported_code(), Some(expected_code));
    assert_eq!(err.status_code(), 501);
    match err {
        PhenoError::UnsupportedPlatform { code, message } => {
            assert_eq!(code, expected_code);
            assert!(
                message.contains(method),
                "expected method {method} in error, got: {message}"
            );
            assert!(
                message.contains("not implemented"),
                "expected unimplemented wording, got: {message}"
            );
        }
        other => panic!("expected UnsupportedPlatform, got {other:?}"),
    }
}

fn assert_docker_stub(err: PhenoError, method: &str) {
    assert_unsupported(err, method, codes::SANDBOX_DOCKER_STUB);
}

#[tokio::test]
async fn get_metadata_returns_sandbox_metadata() {
    let client = make_client("test-sandbox");
    let meta = client.get_metadata().await.unwrap();
    assert_eq!(meta.id, "test-sandbox");
    assert_eq!(meta.image, "stub:latest");
    assert_eq!(meta.cpu_limit, 2);
    assert_eq!(meta.memory_limit_mb, 512);
    assert!(meta.disk_limit_mb.is_some());
    assert_eq!(meta.disk_limit_mb.unwrap(), 5120);
}

#[tokio::test]
async fn get_metadata_id_matches_client() {
    for id in ["sbox-1", "nano-abc", "docker-xyz"] {
        let client = make_client(id);
        let meta = client.get_metadata().await.unwrap();
        assert_eq!(meta.id, id);
    }
}

#[tokio::test]
async fn get_metadata_disk_limit() {
    let client = make_client("test-disk");
    let meta = client.get_metadata().await.unwrap();
    assert!(meta.disk_limit_mb.is_some());
    assert!(*meta.disk_limit_mb.as_ref().unwrap() > 0);
}

#[tokio::test]
async fn get_metadata_reflects_explicit_policy() {
    let policy = SandboxPolicy {
        cpu_cores: 8,
        memory_mib: 4096,
        disk_mib: Some(20_480),
        network: NetworkPolicy::Allow,
    };
    let client = SandboxClient::with_policy("test-policy", policy.clone())
        .expect("with_policy must accept a valid id");
    let meta = client.get_metadata().await.unwrap();
    assert_eq!(meta.cpu_limit, 8);
    assert_eq!(meta.memory_limit_mb, 4096);
    assert_eq!(meta.disk_limit_mb, Some(20_480));
    assert_eq!(client.policy(), &policy);
}

#[tokio::test]
async fn start_is_unimplemented() {
    let client = make_client("test-start");
    assert_docker_stub(client.start().await.unwrap_err(), "start");
}

#[tokio::test]
async fn start_unimplemented_is_stable() {
    let client = make_client("test-idempotent");
    assert_docker_stub(client.start().await.unwrap_err(), "start");
    assert_docker_stub(client.start().await.unwrap_err(), "start");
}

#[tokio::test]
async fn stop_is_unimplemented() {
    let client = make_client("test-stop");
    assert_docker_stub(client.stop().await.unwrap_err(), "stop");
}

#[tokio::test]
async fn stop_unimplemented_is_stable() {
    let client = make_client("test-stop-idempotent");
    assert_docker_stub(client.stop().await.unwrap_err(), "stop");
    assert_docker_stub(client.stop().await.unwrap_err(), "stop");
}

#[tokio::test]
async fn start_stop_both_unimplemented() {
    let client = make_client("test-seq");
    assert_docker_stub(client.start().await.unwrap_err(), "start");
    assert_docker_stub(client.stop().await.unwrap_err(), "stop");
}

#[tokio::test]
async fn exec_valid_cmd_is_unimplemented() {
    let client = make_client("test-exec");
    assert_docker_stub(client.exec("echo hello").await.unwrap_err(), "exec");
}

#[tokio::test]
async fn exec_different_valid_commands_unimplemented() {
    let client = make_client("test-exec2");
    for cmd in ["ls -la", "cat /etc/hostname", "whoami", "pwd", "date"] {
        assert_docker_stub(client.exec(cmd).await.unwrap_err(), "exec");
    }
}

#[tokio::test]
async fn exec_empty_command_is_rejected() {
    let client = make_client("test-empty-cmd");
    let err = client.exec("").await.unwrap_err();
    assert!(
        matches!(err, PhenoError::BadRequest(_)),
        "empty exec must be BadRequest, got {err:?}"
    );
}

#[tokio::test]
async fn exec_long_command_under_limit_is_unimplemented() {
    let client = make_client("test-long-cmd");
    let long_cmd = "echo ".to_string() + &"x".repeat(1000);
    assert_docker_stub(client.exec(&long_cmd).await.unwrap_err(), "exec");
}

#[tokio::test]
async fn exec_over_limit_command_is_rejected() {
    let client = make_client("test-over-limit");
    let cmd = "a".repeat(EXEC_CMD_MAX_LEN + 1);
    let err = client.exec(&cmd).await.unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[tokio::test]
async fn exec_rejects_nul_byte() {
    let client = make_client("test-nul");
    let err = client.exec("echo\0hello").await.unwrap_err();
    assert!(
        matches!(err, PhenoError::BadRequest(_)),
        "NUL byte must be BadRequest, got {err:?}"
    );
}

#[tokio::test]
async fn exec_rejects_newline() {
    let client = make_client("test-newline");
    let err = client.exec("echo hi\necho bye").await.unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[tokio::test]
async fn exec_rejects_shell_injection_as_forbidden() {
    let client = make_client("test-inject");
    for cmd in [
        "echo hi && rm -rf /",
        "cat /etc/passwd > out.txt",
        "echo $(reboot)",
        "echo `id`",
        "echo a; echo b",
        "echo a || echo b",
        "echo a | grep b",
    ] {
        let err = client.exec(cmd).await.unwrap_err();
        assert!(
            matches!(err, PhenoError::Forbidden(_)),
            "injection {cmd:?} must be Forbidden, got {err:?}"
        );
    }
}

#[tokio::test]
async fn resource_usage_is_unimplemented() {
    let client = make_client("test-resource");
    assert_docker_stub(client.resource_usage().await.unwrap_err(), "resource_usage");
}

#[tokio::test]
async fn record_event_returns_ok() {
    let client = make_client("test-record");
    let event = AutomationEvent::screenshot(eidolon_core::Platform::Linux, "/sandbox/screen.png");
    let result = client.record_event(event).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn stub_lifecycle_fails_loud_after_metadata() {
    let client = make_client("test-lifecycle");
    assert!(client.get_metadata().await.is_ok());
    assert_docker_stub(client.start().await.unwrap_err(), "start");
    assert_docker_stub(client.exec("ls").await.unwrap_err(), "exec");
    assert_docker_stub(client.resource_usage().await.unwrap_err(), "resource_usage");
    assert_docker_stub(client.stop().await.unwrap_err(), "stop");
    let end_event = AutomationEvent::screenshot(eidolon_core::Platform::Linux, "/end.png");
    assert!(client.record_event(end_event).await.is_ok());
}

#[test]
fn sandbox_client_exposes_docker_code_by_default() {
    let client = SandboxClient::new("code-check").expect("valid id");
    assert_eq!(client.backend(), "docker");
    assert_eq!(client.unsupported_code(), codes::SANDBOX_DOCKER_STUB);
}

#[test]
fn sandbox_client_backend_label_selects_code() {
    let nano =
        SandboxClient::with_backend("n1", "nanovm", SandboxPolicy::default()).expect("valid id");
    assert_eq!(nano.unsupported_code(), codes::SANDBOX_NANOVM_STUB);
    let kvm = SandboxClient::with_backend("k1", "kvm", SandboxPolicy::default()).expect("valid id");
    assert_eq!(kvm.unsupported_code(), codes::SANDBOX_KVM_STUB);
}

#[tokio::test]
async fn multiple_clients_independent_metadata() {
    let client1 = make_client("sandbox-1");
    let client2 = make_client("sandbox-2");
    let meta1 = client1.get_metadata().await.unwrap();
    let meta2 = client2.get_metadata().await.unwrap();
    assert_ne!(meta1.id, meta2.id);
}

#[tokio::test]
async fn sandbox_client_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SandboxClient>();
}

#[test]
fn new_rejects_empty_sandbox_id() {
    let err = SandboxClient::new("").unwrap_err();
    assert!(
        matches!(err, PhenoError::BadRequest(_)),
        "empty id must be BadRequest, got {err:?}"
    );
}

#[test]
fn new_rejects_oversized_sandbox_id() {
    let id = "a".repeat(SANDBOX_ID_MAX_LEN + 1);
    let err = SandboxClient::new(&id).unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[test]
fn new_rejects_leading_dash() {
    let err = SandboxClient::new("--flag").unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[test]
fn new_rejects_shell_metachars_in_id() {
    for bad in [
        "a;b", "a&b", "a|b", "a$b", "a`b", "a/b", "a\\b", "a:b", "a*b",
    ] {
        let err = SandboxClient::new(bad).unwrap_err();
        assert!(
            matches!(err, PhenoError::BadRequest(_)),
            "id {bad:?} must be BadRequest, got {err:?}"
        );
    }
}

#[test]
fn with_policy_rejects_invalid_id() {
    let err = SandboxClient::with_policy("", SandboxPolicy::default()).unwrap_err();
    assert!(matches!(err, PhenoError::BadRequest(_)));
}

#[test]
fn validate_sandbox_id_round_trip_smoke() {
    assert!(validate_sandbox_id("docker-7c9f").is_ok());
}

#[test]
fn validate_exec_cmd_round_trip_smoke() {
    assert!(validate_exec_cmd("ls -la /tmp").is_ok());
}
