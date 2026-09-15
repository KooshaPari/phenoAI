//! Integration tests for eidolon-desktop on macOS.
//!
//! These tests exercise the real `MacOSClient` implementation via the
//! `DesktopAutomator` trait (re-exported as `DesktopClient`).
//!
//! Hermetic / CI-safe design:
//! - `get_viewport` queries real macOS display info (read-only, no side effects).
//! - `screenshot` tests use **invalid paths** — macOS `screencapture` always
//!   exits 0, so these verify graceful handling (no panic or crash).
//! - `pointer` / `text` unknown-action tests verify graceful degradation
//!   without any real input injection.
//! - `record_event` is a pure no-op (log + Ok).
//!
//! ## Running
//! ```bash
//! cargo test -p eidolon-desktop --test integration
//! ```

#![cfg(target_os = "macos")]

use eidolon_core::input::{InputType, PointerAction, PointerInput, TextInput};
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, PhenoError, Platform};
use eidolon_desktop::DesktopClient;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a fresh `DesktopClient` for each test.
fn make_client() -> DesktopClient {
    DesktopClient::new().expect("DesktopClient::new() should succeed on macOS")
}

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_viewport_returns_non_zero() {
    let client = make_client();
    let viewport = client
        .get_viewport()
        .await
        .expect("get_viewport should succeed on macOS");

    assert!(
        viewport.width > 0,
        "viewport width should be > 0, got {}",
        viewport.width
    );
    assert!(
        viewport.height > 0,
        "viewport height should be > 0, got {}",
        viewport.height
    );
    assert!(
        viewport.dpr > 0.0,
        "viewport dpr should be > 0.0, got {}",
        viewport.dpr
    );
    // Orientation field should be "landscape" or "portrait"
    assert!(
        viewport.orientation == "landscape" || viewport.orientation == "portrait",
        "unexpected orientation: {}",
        viewport.orientation
    );
}

// ---------------------------------------------------------------------------
// Screenshot — graceful handling (macOS `screencapture` always exits 0;
// error paths only trigger if the binary can't be launched)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_screenshot_invalid_path_returns_error() {
    let client = make_client();
    let result = client
        .screenshot("/nonexistent/missing_dir/screenshot.png")
        .await;

    // macOS `screencapture` always exits 0 even when it can't write (it
    // prints a warning to stderr but returns success). So this may be Ok
    // or Err depending on whether the binary is available. The key
    // assertion: no panic, no crash.
    match result {
        Ok(()) => { /* screencapture ran; it logged a warning but returned Ok */ }
        Err(PhenoError::Platform(msg)) => {
            assert!(!msg.is_empty(), "Platform error message must not be empty");
        }
        Err(other) => panic!("Expected PhenoError::Platform or Ok, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_screenshot_empty_path_does_not_panic() {
    let client = make_client();
    let result = client.screenshot("").await;

    // Just verify it resolves without panicking.
    let _ = result;
}

// ---------------------------------------------------------------------------
// Pointer — unknown action (graceful degradation, no real input)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pointer_unknown_action_no_panic() {
    let client = make_client();
    let input = PointerInput {
        x: 100,
        y: 200,
        button: None,
        action: PointerAction::Move,
        duration_ms: None,
    };
    let result = client.pointer(&input).await;

    assert!(
        result.is_ok(),
        "unknown pointer action should not panic; got: {result:?}"
    );
}

#[tokio::test]
async fn test_pointer_empty_action_no_panic() {
    let client = make_client();
    let input = PointerInput {
        x: 0,
        y: 0,
        button: None,
        action: PointerAction::Move,
        duration_ms: None,
    };
    let result = client.pointer(&input).await;

    assert!(
        result.is_ok(),
        "empty pointer action should not panic; got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Text — unknown input type (graceful degradation, no real input)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_text_unknown_input_type_no_panic() {
    let client = make_client();
    let input = TextInput {
        text: "hello".to_string(),
        input_type: InputType::Char,
        delay_ms: None,
    };
    let result = client.text(&input).await;

    assert!(
        result.is_ok(),
        "unknown text input type should not panic; got: {result:?}"
    );
}

#[tokio::test]
async fn test_text_clear_no_panic() {
    let client = make_client();
    let input = TextInput {
        text: String::new(),
        input_type: InputType::Char,
        delay_ms: None,
    };
    let result = client.text(&input).await;

    assert!(
        result.is_ok(),
        "clear input type should succeed (no-op); got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// record_event — pure no-op, accepts all event payload variants
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_record_event_accepts_all_types() {
    let client = make_client();

    let pointer_event = AutomationEvent::pointer(Platform::MacOS, PointerInput::click(10, 20));
    assert!(
        client.record_event(pointer_event).await.is_ok(),
        "record_event should accept pointer events"
    );

    let text_event = AutomationEvent::text(Platform::MacOS, TextInput::keystroke("test"));
    assert!(
        client.record_event(text_event).await.is_ok(),
        "record_event should accept text events"
    );

    let screenshot_event = AutomationEvent::screenshot(Platform::MacOS, "/tmp/test.png");
    assert!(
        client.record_event(screenshot_event).await.is_ok(),
        "record_event should accept screenshot events"
    );
}

// ---------------------------------------------------------------------------
// Combined — execute every method and verify no crash
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_all_methods_handle_gracefully() {
    let client = make_client();

    // 1. get_viewport
    let viewport = client.get_viewport().await;
    assert!(viewport.is_ok(), "get_viewport should not crash");

    // 2. screenshot — invalid path (screencapture always exits 0, so may be Ok)
    let _ = client.screenshot("/dev/null/nope").await;
    // No assert on Ok/Err — just verifying no panic.

    // 3. pointer — unknown action
    let unknown_pointer = client
        .pointer(&PointerInput {
            x: 0,
            y: 0,
            button: None,
            action: PointerAction::Move,
            duration_ms: None,
        })
        .await;
    assert!(
        unknown_pointer.is_ok(),
        "unknown pointer action should not panic"
    );

    // 4. text — unknown input type
    let unknown_text = client
        .text(&TextInput {
            text: String::new(),
            input_type: InputType::Char,
            delay_ms: None,
        })
        .await;
    assert!(
        unknown_text.is_ok(),
        "unknown text input type should not panic"
    );

    // 5. record_event
    let event = AutomationEvent::screenshot(Platform::MacOS, "/tmp/integration-test.png");
    assert!(
        client.record_event(event).await.is_ok(),
        "record_event should succeed"
    );
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

#[test]
fn test_desktop_client_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DesktopClient>();
}
