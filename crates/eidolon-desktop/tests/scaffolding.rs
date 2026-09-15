//! Cross-target scaffolding tests for Win/Linux/recording/security hooks.
//!
//! These compile and run on **all** platforms (including macOS) so CI/local
//! macOS stays green while asserting fail-loud contracts for stubs and the
//! Windows readiness / gate surface.
//!
//! Platform-specific live-driver smoke tests live in
//! [`scaffolding_test_actions`]; stub/code-stability assertions live in
//! [`scaffolding_test_assertions`].

use eidolon_core::error::PhenoError;
use eidolon_desktop::{
    actions_allowed, caps, codes, require_actions_allowed, DesktopSecurityGate,
    PolicySecurityGate,
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

#[test]
fn desktop_actions_gate_contract() {
    assert_eq!(
        eidolon_desktop::ACTIONS_ALLOW_ENV,
        "EIDOLON_DESKTOP_ALLOW_ACTIONS"
    );
    if actions_allowed() {
        return;
    }
    assert_code(
        require_actions_allowed("pointer").unwrap_err(),
        codes::DESKTOP_ACTIONS_GATED,
    );
}

#[test]
fn security_hooks_policy_gate_allows_and_wires_core() {
    let gate = PolicySecurityGate::permissive_desktop();
    gate.check_capability(caps::POINTER_INJECT)
        .expect("pointer.inject allowed");
    gate.check_sandbox_id("docker-7c9f")
        .expect("core sandbox id validator");
    gate.check_exec_cmd("ls -la /tmp")
        .expect("core exec cmd validator");

    let forbidden = gate.check_capability("admin.root").unwrap_err();
    assert!(matches!(forbidden, PhenoError::Forbidden(_)));
    assert_eq!(forbidden.status_code(), 403);

    assert_code(
        gate.check_capability("vault.encrypt").unwrap_err(),
        codes::DESKTOP_SECURITY_UNAVAILABLE,
    );
}

#[cfg(feature = "desktop-linux")]
#[test]
fn wayland_restore_smoke_env_contract_hermetic() {
    use eidolon_desktop::linux::wayland_restore_token::restore_enabled;

    if std::env::var("EIDOLON_DESKTOP_WAYLAND_RESTORE").is_ok() {
        return;
    }
    assert!(!restore_enabled());
}
