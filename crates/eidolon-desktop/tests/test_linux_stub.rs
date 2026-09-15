//! Tests for Eidolon Linux desktop driver stubs.
//!
//! Covers [`LinuxStub`] (DesktopAutomator fail-loud), [`AtspiStub`]
//! (LinuxAtspiAutomator fail-loud), and the [`Platform`] enum roundtrip.
//!
//! All tests run on every host (macOS, Linux, Windows) — the Linux stubs are
//! always compiled regardless of target OS.

use eidolon_core::error::PhenoError;
use eidolon_core::event::Platform;
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, PointerInput, TextInput};
use eidolon_desktop::{
    codes, AtspiNode, AtspiStub, LinuxAtspiAutomator, LinuxDesktopDriver, LinuxStub,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assert_unsupported(err: PhenoError, code: &str, method: &str) {
    assert_eq!(err.unsupported_code(), Some(code));
    assert_eq!(err.status_code(), 501);
    match &err {
        PhenoError::UnsupportedPlatform {
            code: c,
            message: msg,
        } => {
            assert_eq!(c, code);
            assert!(
                msg.contains(method),
                "error message should mention method {method}, got: {msg}"
            );
            assert!(
                msg.contains("not implemented"),
                "error message should say 'not implemented', got: {msg}"
            );
        }
        other => panic!("expected UnsupportedPlatform, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// LinuxStub — DesktopAutomator actions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linux_stub_get_viewport_unsupported() {
    let stub = LinuxStub::new();
    assert_unsupported(
        stub.get_viewport().await.unwrap_err(),
        codes::DESKTOP_LINUX_STUB,
        "get_viewport",
    );
}

#[tokio::test]
async fn linux_stub_screenshot_unsupported() {
    let stub = LinuxStub::new();
    assert_unsupported(
        stub.screenshot("/tmp/test.png").await.unwrap_err(),
        codes::DESKTOP_LINUX_STUB,
        "screenshot",
    );
}

#[tokio::test]
async fn linux_stub_pointer_unsupported() {
    let stub = LinuxStub::new();
    assert_unsupported(
        stub.pointer(&PointerInput::click(100, 200))
            .await
            .unwrap_err(),
        codes::DESKTOP_LINUX_STUB,
        "pointer",
    );
}

#[tokio::test]
async fn linux_stub_text_unsupported() {
    let stub = LinuxStub::new();
    assert_unsupported(
        stub.text(&TextInput::keystroke("hello")).await.unwrap_err(),
        codes::DESKTOP_LINUX_STUB,
        "text",
    );
}

#[tokio::test]
async fn linux_stub_record_event_ok() {
    let stub = LinuxStub::new();
    let event = AutomationEvent::screenshot(Platform::Linux, "/tmp/screen.png");
    stub.record_event(event)
        .await
        .expect("record_event should succeed on stub");
}

#[tokio::test]
async fn linux_stub_all_actions_fail_loud() {
    let stub = LinuxStub::new();
    let errors = vec![
        stub.get_viewport().await.unwrap_err(),
        stub.screenshot("/tmp/s.png").await.unwrap_err(),
        stub.pointer(&PointerInput::click(0, 0)).await.unwrap_err(),
        stub.text(&TextInput::keystroke("x")).await.unwrap_err(),
    ];
    for err in &errors {
        assert_eq!(err.unsupported_code(), Some(codes::DESKTOP_LINUX_STUB));
        assert_eq!(err.status_code(), 501);
    }
    assert_eq!(errors.len(), 4);
}

// ---------------------------------------------------------------------------
// LinuxStub — LinuxDesktopDriver trait
// ---------------------------------------------------------------------------

#[test]
fn linux_stub_driver_readiness_flags_false() {
    let stub = LinuxStub::new();
    assert!(!stub.x11_ready(), "x11_ready should be false for stub");
    assert!(
        !stub.wayland_ready(),
        "wayland_ready should be false for stub"
    );
    assert!(!stub.atspi_ready(), "atspi_ready should be false for stub");
}

// ---------------------------------------------------------------------------
// LinuxStub — send/sync and clone
// ---------------------------------------------------------------------------

#[test]
fn linux_stub_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LinuxStub>();
}

#[test]
fn linux_stub_is_clone() {
    let stub = LinuxStub::new();
    let _cloned = stub.clone();
}

#[test]
fn linux_stub_is_debug() {
    let stub = LinuxStub::new();
    let dbg = format!("{:?}", stub);
    assert_eq!(dbg, "LinuxStub");
}

// ---------------------------------------------------------------------------
// AtspiStub — LinuxAtspiAutomator actions
// ---------------------------------------------------------------------------

#[test]
fn atspi_stub_not_ready() {
    let stub = AtspiStub::new();
    assert!(!stub.atspi_ready(), "atspi_ready should be false for stub");
}

#[tokio::test]
async fn atspi_stub_list_nodes_unsupported() {
    let stub = AtspiStub::new();
    assert_unsupported(
        stub.list_nodes(8).await.unwrap_err(),
        codes::DESKTOP_LINUX_ATSPI_STUB,
        "list_nodes",
    );
}

#[tokio::test]
async fn atspi_stub_find_by_role_unsupported() {
    let stub = AtspiStub::new();
    assert_unsupported(
        stub.find_by_role("push button", Some("OK"))
            .await
            .unwrap_err(),
        codes::DESKTOP_LINUX_ATSPI_STUB,
        "find_by_role",
    );
}

#[tokio::test]
async fn atspi_stub_find_by_role_no_name_unsupported() {
    let stub = AtspiStub::new();
    assert_unsupported(
        stub.find_by_role("button", None).await.unwrap_err(),
        codes::DESKTOP_LINUX_ATSPI_STUB,
        "find_by_role",
    );
}

#[tokio::test]
async fn atspi_stub_activate_unsupported() {
    let stub = AtspiStub::new();
    let node = AtspiNode {
        role: "push button".into(),
        name: "OK".into(),
        path: "/org/a11y/atspi/accessible/null".into(),
        bus_name: ":0.0".into(),
        description: String::new(),
        states: vec![],
    };
    assert_unsupported(
        stub.activate(&node).await.unwrap_err(),
        codes::DESKTOP_LINUX_ATSPI_STUB,
        "activate",
    );
}

#[tokio::test]
async fn atspi_stub_all_actions_fail_loud() {
    let stub = AtspiStub::new();
    let node = AtspiNode {
        role: "push button".into(),
        name: "OK".into(),
        path: "/org/a11y/atspi/accessible/null".into(),
        bus_name: ":0.0".into(),
        description: String::new(),
        states: vec![],
    };
    let errors = vec![
        stub.list_nodes(8).await.unwrap_err(),
        stub.find_by_role("button", None).await.unwrap_err(),
        stub.activate(&node).await.unwrap_err(),
    ];
    for err in &errors {
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_ATSPI_STUB)
        );
        assert_eq!(err.status_code(), 501);
    }
    assert_eq!(errors.len(), 3);
}

// ---------------------------------------------------------------------------
// AtspiStub — send/sync and clone
// ---------------------------------------------------------------------------

#[test]
fn atspi_stub_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AtspiStub>();
}

#[test]
fn atspi_stub_is_clone() {
    let stub = AtspiStub::new();
    let _cloned = stub.clone();
}

#[test]
fn atspi_stub_is_debug() {
    let stub = AtspiStub::new();
    let dbg = format!("{:?}", stub);
    assert_eq!(dbg, "AtspiStub");
}

// ---------------------------------------------------------------------------
// Platform enum — debug format
// ---------------------------------------------------------------------------

#[test]
fn platform_debug_format_macos() {
    let p = Platform::MacOS;
    assert!(format!("{:?}", p).contains("MacOS"));
}

#[test]
fn platform_debug_format_linux() {
    let p = Platform::Linux;
    assert!(format!("{:?}", p).contains("Linux"));
}

#[test]
fn platform_debug_format_windows() {
    let p = Platform::Windows;
    assert!(format!("{:?}", p).contains("Windows"));
}

#[test]
fn platform_debug_format_ios() {
    let p = Platform::Ios;
    assert!(format!("{:?}", p).contains("Ios"));
}

#[test]
fn platform_debug_format_android() {
    let p = Platform::Android;
    assert!(format!("{:?}", p).contains("Android"));
}

#[test]
fn platform_debug_format_unknown() {
    let p = Platform::Unknown;
    assert!(format!("{:?}", p).contains("Unknown"));
}

// ---------------------------------------------------------------------------
// Platform enum — serde roundtrip
// ---------------------------------------------------------------------------

#[test]
fn platform_serde_roundtrip() {
    let platforms = vec![
        Platform::MacOS,
        Platform::Windows,
        Platform::Linux,
        Platform::Ios,
        Platform::Android,
        Platform::Unknown,
    ];
    for p in platforms {
        let json = serde_json::to_string(&p).unwrap();
        let back: Platform = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{:?}", p), format!("{:?}", back));
    }
}

#[test]
fn platform_serde_snake_case() {
    let mac_json = serde_json::to_string(&Platform::MacOS).unwrap();
    assert_eq!(mac_json, "\"mac_o_s\"");
    let linux_json = serde_json::to_string(&Platform::Linux).unwrap();
    assert_eq!(linux_json, "\"linux\"");
    let win_json = serde_json::to_string(&Platform::Windows).unwrap();
    assert_eq!(win_json, "\"windows\"");
}

#[test]
fn platform_serde_deserialize_all_variants() {
    let cases = vec![
        ("\"mac_o_s\"", Platform::MacOS),
        ("\"windows\"", Platform::Windows),
        ("\"linux\"", Platform::Linux),
        ("\"ios\"", Platform::Ios),
        ("\"android\"", Platform::Android),
        ("\"unknown\"", Platform::Unknown),
    ];
    for (json_str, expected) in cases {
        let p: Platform = serde_json::from_str(json_str).unwrap();
        assert_eq!(p, expected);
    }
}

// ---------------------------------------------------------------------------
// Platform enum — Display trait
// ---------------------------------------------------------------------------

#[test]
fn platform_display_lowercase() {
    assert_eq!(format!("{}", Platform::MacOS), "macos");
    assert_eq!(format!("{}", Platform::Windows), "windows");
    assert_eq!(format!("{}", Platform::Linux), "linux");
    assert_eq!(format!("{}", Platform::Ios), "ios");
    assert_eq!(format!("{}", Platform::Android), "android");
    assert_eq!(format!("{}", Platform::Unknown), "unknown");
}

// ---------------------------------------------------------------------------
// LinuxStub error message contains expected guidance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn linux_stub_error_message_contains_feature_hint() {
    let stub = LinuxStub::new();
    let err = stub.get_viewport().await.unwrap_err();
    match err {
        PhenoError::UnsupportedPlatform { message, .. } => {
            assert!(
                message.contains("desktop-linux"),
                "message should mention desktop-linux feature: {message}"
            );
        }
        other => panic!("expected UnsupportedPlatform, got {other:?}"),
    }
}

#[tokio::test]
async fn atspi_stub_error_message_contains_feature_hint() {
    let stub = AtspiStub::new();
    let err = stub.list_nodes(1).await.unwrap_err();
    match err {
        PhenoError::UnsupportedPlatform { message, .. } => {
            assert!(
                message.contains("desktop-linux-atspi"),
                "message should mention desktop-linux-atspi feature: {message}"
            );
        }
        other => panic!("expected UnsupportedPlatform, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Code string constants are stable
// ---------------------------------------------------------------------------

#[test]
fn linux_stub_code_is_stable() {
    assert_eq!(codes::DESKTOP_LINUX_STUB, "EIDOLON_DESKTOP_LINUX_STUB");
}

#[test]
fn linux_atspi_stub_code_is_stable() {
    assert_eq!(
        codes::DESKTOP_LINUX_ATSPI_STUB,
        "EIDOLON_DESKTOP_LINUX_ATSPI_STUB"
    );
}

// ---------------------------------------------------------------------------
// LinuxDesktopDriver trait default methods
// ---------------------------------------------------------------------------

#[test]
fn linux_desktop_driver_trait_defaults() {
    let stub = LinuxStub::new();
    // LinuxDesktopDriver is implemented for LinuxStub; defaults return false
    assert!(!LinuxDesktopDriver::x11_ready(&stub));
    assert!(!LinuxDesktopDriver::wayland_ready(&stub));
    assert!(!LinuxDesktopDriver::atspi_ready(&stub));
}
