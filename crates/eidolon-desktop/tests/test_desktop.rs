//! Unit tests for eidolon-desktop (cross-platform stub).
//!
//! Non-macOS / non-Win-driver / non-Linux-driver `DesktopClient` is a
//! fail-loud stub (A+ T1). These tests only compile when `DesktopClient` is
//! the stub (not macOS, not Windows with `desktop-windows`, not Linux with
//! `desktop-linux`).
//!
//! See `macos_integration.rs` for macOS-specific integration tests and
//! `scaffolding.rs` for Win/Linux/recording hooks (all targets).

#![cfg(not(any(
    target_os = "macos",
    all(target_os = "windows", feature = "desktop-windows"),
    all(target_os = "linux", feature = "desktop-linux")
)))]

use eidolon_core::error::PhenoError;
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, PointerInput, TextInput};
use eidolon_desktop::codes;
use eidolon_desktop::DesktopClient;
use std::sync::Arc;

fn make_client(platform: &str) -> Arc<dyn DesktopAutomator> {
    Arc::new(DesktopClient::new(platform))
}

fn expected_code() -> &'static str {
    if cfg!(target_os = "windows") {
        codes::DESKTOP_WIN_STUB
    } else if cfg!(target_os = "linux") {
        codes::DESKTOP_LINUX_STUB
    } else {
        codes::DESKTOP_OTHER_STUB
    }
}

fn assert_unsupported(err: PhenoError, method: &str) {
    match err {
        PhenoError::UnsupportedPlatform { code, message } => {
            assert_eq!(code, expected_code());
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

#[tokio::test]
async fn get_viewport_is_unsupported() {
    let client = make_client("linux");
    assert_unsupported(client.get_viewport().await.unwrap_err(), "get_viewport");
}

#[tokio::test]
async fn get_viewport_unsupported_cross_platform_labels() {
    for platform in ["macos", "windows", "linux"] {
        let client = make_client(platform);
        assert_unsupported(client.get_viewport().await.unwrap_err(), "get_viewport");
    }
}

#[tokio::test]
async fn screenshot_is_unsupported() {
    let client = make_client("windows");
    assert_unsupported(
        client.screenshot("/tmp/test-screenshot.png").await.unwrap_err(),
        "screenshot",
    );
}

#[tokio::test]
async fn pointer_is_unsupported() {
    let client = make_client("linux");
    let input = PointerInput::click(100, 200);
    assert_unsupported(client.pointer(&input).await.unwrap_err(), "pointer");
}

#[tokio::test]
async fn text_is_unsupported() {
    let client = make_client("windows");
    let input = TextInput::keystroke("hello world");
    assert_unsupported(client.text(&input).await.unwrap_err(), "text");
}

#[tokio::test]
async fn record_event_returns_ok() {
    let client = make_client("linux");
    let event = AutomationEvent::screenshot("desktop", "/tmp/screen.png");
    let result = client.record_event(event).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn all_action_methods_fail_loud_with_code() {
    let client = DesktopClient::new("linux");
    assert_eq!(client.unsupported_code(), expected_code());

    let client: Arc<dyn DesktopAutomator> = Arc::new(client);
    for err in [
        client.get_viewport().await.unwrap_err(),
        client.screenshot("/tmp/s.png").await.unwrap_err(),
        client
            .pointer(&PointerInput::click(1, 2))
            .await
            .unwrap_err(),
        client
            .text(&TextInput::keystroke("test"))
            .await
            .unwrap_err(),
    ] {
        assert_eq!(err.unsupported_code(), Some(expected_code()));
        assert_eq!(err.status_code(), 501);
    }

    assert!(client
        .record_event(AutomationEvent::screenshot("linux", "/s.png"))
        .await
        .is_ok());
}

#[tokio::test]
async fn desktop_client_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DesktopClient>();
}

#[test]
fn platform_label_is_retained() {
    let client = DesktopClient::new("windows");
    assert_eq!(client.platform(), "windows");
}
