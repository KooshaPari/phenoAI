//! macOS-specific integration tests for eidolon-desktop.
//!
//! These tests exercise the real MacOSClient implementation using Core Graphics.
//! They only compile and run on macOS.

#![cfg(target_os = "macos")]

use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{PhenoError, TextInput};
use eidolon_desktop::DesktopClient;

#[tokio::test]
async fn test_get_viewport_returns_positive_dimensions() {
    let client = DesktopClient::new().unwrap();
    let viewport = client.get_viewport().await.unwrap();
    assert!(viewport.width > 0, "viewport width should be positive");
    assert!(viewport.height > 0, "viewport height should be positive");
    assert!(viewport.dpr > 0.0, "viewport dpr should be positive");
}

#[tokio::test]
async fn test_screenshot_invalid_path_returns_platform_error() {
    let client = DesktopClient::new().unwrap();
    let result = client.screenshot("/nonexistent/dir/screenshot.png").await;
    assert!(result.is_err(), "screenshot with invalid path should fail");
    match result {
        Err(PhenoError::Platform(msg)) => {
            assert!(
                !msg.is_empty(),
                "platform error message should not be empty"
            );
        }
        Err(other) => panic!("expected Platform error, got: {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

#[tokio::test]
async fn test_text_keystroke_empty_string() {
    let client = DesktopClient::new().unwrap();
    let input = TextInput::keystroke("");
    let result = client.text(&input).await;
    assert!(
        result.is_ok(),
        "keystroke with empty string should succeed (no-op)"
    );
}

#[test]
fn test_platform_error_status_code_is_500() {
    let err: PhenoError = PhenoError::Platform("test platform error".into());
    assert_eq!(err.status_code(), 500);
}
