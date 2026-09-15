//! Cross-target assertion tests for stubs, error codes, and gate contracts.
//!
//! These compile and run on **all** platforms (including macOS) so CI/local
//! macOS stays green while asserting fail-loud contracts for stubs and the
//! documented code stability.

use eidolon_core::error::PhenoError;
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{PointerInput, TextInput};
use eidolon_desktop::{
    actions_allowed, caps, capture_plan, codes, parse_capture_mode, require_actions_allowed,
    require_windows_capture_host, AtspiNode, AtspiStub, DesktopRecorder, DesktopSecurityGate,
    LinuxAtspiAutomator, LinuxDesktopDriver, LinuxStub, PolicySecurityGate, QualityProfile,
    RecordingStub, SecurityHooksStub, WinCaptureBackend, WinCaptureMode, WindowsDesktopDriver,
    WindowsStub, ACTIONS_ALLOW_ENV, CAPTURE_MODE_ENV, CAPTURE_SMOKE_ENV, WAYLAND_SMOKE_ENV,
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
async fn windows_stub_fail_loud() {
    let stub = WindowsStub::new();
    assert!(!stub.send_input_ready());
    assert!(!stub.capture_ready());

    assert_code(
        stub.get_viewport().await.unwrap_err(),
        codes::DESKTOP_WIN_STUB,
    );
    assert_code(
        stub.screenshot("/tmp/w.png").await.unwrap_err(),
        codes::DESKTOP_WIN_STUB,
    );
    assert_code(
        stub.pointer(&PointerInput::click(0, 0)).await.unwrap_err(),
        codes::DESKTOP_WIN_STUB,
    );
    assert_code(
        stub.text(&TextInput::keystroke("x")).await.unwrap_err(),
        codes::DESKTOP_WIN_STUB,
    );
}

#[test]
fn windows_feature_does_not_activate_on_non_windows() {
    if cfg!(all(target_os = "windows", feature = "desktop-windows")) {
        return;
    }
    let stub = WindowsStub::new();
    assert!(!stub.send_input_ready());
    assert!(!WindowsStub::new().capture_ready());
}

#[test]
fn desktop_actions_gate_contract() {
    assert_eq!(ACTIONS_ALLOW_ENV, "EIDOLON_DESKTOP_ALLOW_ACTIONS");
    if actions_allowed() {
        return;
    }
    assert_code(
        require_actions_allowed("pointer").unwrap_err(),
        codes::DESKTOP_ACTIONS_GATED,
    );
}

#[tokio::test]
async fn linux_stub_fail_loud() {
    let stub = LinuxStub::new();
    assert!(!stub.x11_ready());
    assert!(!stub.wayland_ready());

    assert_code(
        stub.get_viewport().await.unwrap_err(),
        codes::DESKTOP_LINUX_STUB,
    );
    assert_code(
        stub.screenshot("/tmp/l.png").await.unwrap_err(),
        codes::DESKTOP_LINUX_STUB,
    );
}

#[test]
fn linux_feature_does_not_activate_on_non_linux() {
    if cfg!(all(target_os = "linux", feature = "desktop-linux")) {
        return;
    }
    let stub = LinuxStub::new();
    assert!(!stub.x11_ready());
    assert!(!stub.wayland_ready());
    assert!(!stub.atspi_ready());
}

#[tokio::test]
async fn atspi_stub_fail_loud() {
    let stub = AtspiStub::new();
    assert!(!stub.atspi_ready());

    assert_code(
        stub.list_nodes(8).await.unwrap_err(),
        codes::DESKTOP_LINUX_ATSPI_STUB,
    );
    assert_code(
        stub.find_by_role("push button", Some("OK"))
            .await
            .unwrap_err(),
        codes::DESKTOP_LINUX_ATSPI_STUB,
    );
    let node = AtspiNode {
        role: "push button".into(),
        name: "OK".into(),
        path: "/org/a11y/atspi/accessible/null".into(),
        bus_name: ":0.0".into(),
        description: String::new(),
        states: vec![],
    };
    assert_code(
        stub.activate(&node).await.unwrap_err(),
        codes::DESKTOP_LINUX_ATSPI_STUB,
    );
}

#[test]
fn atspi_feature_does_not_activate_on_non_linux() {
    if cfg!(all(target_os = "linux", feature = "desktop-linux-atspi")) {
        return;
    }
    let stub = AtspiStub::new();
    assert!(!stub.atspi_ready());
}

#[tokio::test]
async fn recording_stub_fail_loud() {
    let stub = RecordingStub::new();
    assert!(!stub.ffmpeg_ready());
    assert_code(
        stub.start("/tmp/out.mp4", QualityProfile::Balanced)
            .await
            .unwrap_err(),
        codes::DESKTOP_RECORDING_UNAVAILABLE,
    );
    assert_code(
        stub.stop().await.unwrap_err(),
        codes::DESKTOP_RECORDING_UNAVAILABLE,
    );
}

#[test]
fn security_hooks_stub_fail_loud() {
    let stub = SecurityHooksStub::new();
    assert_code(
        stub.check_capability(caps::POINTER_INJECT).unwrap_err(),
        codes::DESKTOP_SECURITY_UNAVAILABLE,
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

#[test]
fn documented_codes_are_stable() {
    assert_eq!(codes::DESKTOP_WIN_STUB, "EIDOLON_DESKTOP_WIN_STUB");
    assert_eq!(codes::DESKTOP_LINUX_STUB, "EIDOLON_DESKTOP_LINUX_STUB");
    assert_eq!(
        codes::DESKTOP_LINUX_WAYLAND_UNSUPPORTED,
        "EIDOLON_DESKTOP_LINUX_WAYLAND_UNSUPPORTED"
    );
    assert_eq!(
        codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
        "EIDOLON_DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE"
    );
    assert_eq!(
        codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED,
        "EIDOLON_DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED"
    );
    assert_eq!(
        codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO,
        "EIDOLON_DESKTOP_LINUX_WAYLAND_RESTORE_IO"
    );
    assert_eq!(
        codes::DESKTOP_LINUX_ATSPI_STUB,
        "EIDOLON_DESKTOP_LINUX_ATSPI_STUB"
    );
    assert_eq!(
        codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE,
        "EIDOLON_DESKTOP_LINUX_ATSPI_UNAVAILABLE"
    );
    assert_eq!(codes::DESKTOP_OTHER_STUB, "EIDOLON_DESKTOP_OTHER_STUB");
    assert_eq!(
        codes::DESKTOP_ACTIONS_GATED,
        "EIDOLON_DESKTOP_ACTIONS_GATED"
    );
    assert_eq!(
        codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE,
        "EIDOLON_DESKTOP_WIN_CAPTURE_UNAVAILABLE"
    );
    assert_eq!(
        codes::DESKTOP_RECORDING_UNAVAILABLE,
        "EIDOLON_DESKTOP_RECORDING_UNAVAILABLE"
    );
    assert_eq!(
        codes::DESKTOP_SECURITY_UNAVAILABLE,
        "EIDOLON_DESKTOP_SECURITY_UNAVAILABLE"
    );
}

#[test]
fn windows_capture_preference_hermetic() {
    assert_eq!(CAPTURE_MODE_ENV, "EIDOLON_DESKTOP_WIN_CAPTURE");
    assert_eq!(CAPTURE_SMOKE_ENV, "EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE");
    assert_eq!(WAYLAND_SMOKE_ENV, "EIDOLON_DESKTOP_WAYLAND_SMOKE");
    assert_eq!(parse_capture_mode(None), WinCaptureMode::Auto);
    assert_eq!(parse_capture_mode(Some("dxgi")), WinCaptureMode::Dxgi);
    assert_eq!(parse_capture_mode(Some("gdi")), WinCaptureMode::Gdi);
    assert_eq!(
        capture_plan(WinCaptureMode::Auto),
        &[WinCaptureBackend::Dxgi, WinCaptureBackend::Gdi]
    );
    assert_eq!(
        capture_plan(WinCaptureMode::Dxgi),
        &[WinCaptureBackend::Dxgi]
    );
    assert_eq!(capture_plan(WinCaptureMode::Gdi), &[WinCaptureBackend::Gdi]);
}

#[test]
fn windows_capture_smoke_fails_loud_off_windows() {
    #[cfg(not(all(target_os = "windows", feature = "desktop-windows")))]
    {
        assert_code(
            require_windows_capture_host().unwrap_err(),
            codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE,
        );
    }
    #[cfg(all(target_os = "windows", feature = "desktop-windows"))]
    {
        assert!(require_windows_capture_host().is_ok());
    }
}

/// Hermetic: `EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1` on macOS/Linux must fail loud
/// (no silent skip). Uses a subprocess so parallel tests stay env-clean.
#[test]
#[cfg(not(all(target_os = "windows", feature = "desktop-windows")))]
fn windows_capture_smoke_env_fails_loud_off_windows() {
    use std::process::Command;

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let output = Command::new(env!("CARGO"))
        .current_dir(manifest_dir)
        .env(CAPTURE_SMOKE_ENV, "1")
        .args([
            "test",
            "--locked",
            "--lib",
            "windows::capture::tests::capture_smoke_gate_fails_loud_off_windows_with_env",
            "--",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .expect("spawn capture smoke subprocess");
    assert!(
        output.status.success(),
        "subprocess failed (status={:?})\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn wayland_smoke_fails_loud_off_linux_or_without_wayland() {
    use eidolon_desktop::{
        require_wayland_smoke, require_wayland_smoke_host, wayland_smoke_requested,
    };

    if wayland_smoke_requested() {
        let host = require_wayland_smoke_host();
        #[cfg(all(target_os = "linux", feature = "desktop-linux"))]
        {
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                assert!(host.is_ok());
            } else {
                assert_code(
                    host.unwrap_err(),
                    codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
                );
            }
        }
        #[cfg(not(all(target_os = "linux", feature = "desktop-linux")))]
        {
            assert_code(
                host.unwrap_err(),
                codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
            );
        }
        return;
    }

    assert_code(
        require_wayland_smoke().unwrap_err(),
        codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
    );
    #[cfg(not(all(target_os = "linux", feature = "desktop-linux")))]
    {
        assert_code(
            require_wayland_smoke_host().unwrap_err(),
            codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
        );
    }
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
