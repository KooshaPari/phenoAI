//! Unit tests for eidolon-mobile.
//!
//! `MobileClient` is a fail-loud stub (A+ T1). Action methods must return
//! `PhenoError::UnsupportedPlatform` with documented codes, not silent Ok.
//! `record_event` remains Ok (local sink).

use std::sync::Arc;

use eidolon_core::error::PhenoError;
use eidolon_core::event::Platform;
use eidolon_core::traits::MobileAutomator;
use eidolon_core::AutomationEvent;
use eidolon_mobile::{code_for_platform, codes, MobileClient};

fn make_client(platform: &str) -> Arc<dyn MobileAutomator> {
    Arc::new(MobileClient::new(platform))
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

#[tokio::test]
async fn get_viewport_is_unsupported() {
    let client = make_client("ios");
    assert_unsupported(
        client.get_viewport().await.unwrap_err(),
        "get_viewport",
        codes::MOBILE_IOS_STUB,
    );
}

#[tokio::test]
async fn get_viewport_unsupported_cross_platform() {
    for (platform, code) in [
        ("ios", codes::MOBILE_IOS_STUB),
        ("android", codes::MOBILE_ANDROID_STUB),
    ] {
        let client = make_client(platform);
        assert_unsupported(
            client.get_viewport().await.unwrap_err(),
            "get_viewport",
            code,
        );
    }
}

#[tokio::test]
async fn screenshot_is_unsupported() {
    let client = make_client("ios");
    assert_unsupported(
        client
            .screenshot("/tmp/mobile-screen.png")
            .await
            .unwrap_err(),
        "screenshot",
        codes::MOBILE_IOS_STUB,
    );
}

#[tokio::test]
async fn screenshot_unsupported_different_paths() {
    let client = make_client("android");
    for path in ["/dcim/screen.png", "/Pictures/s.png", "screen.png"] {
        assert_unsupported(
            client.screenshot(path).await.unwrap_err(),
            "screenshot",
            codes::MOBILE_ANDROID_STUB,
        );
    }
}

#[tokio::test]
async fn tap_is_unsupported() {
    let client = make_client("ios");
    assert_unsupported(
        client.tap(540, 960).await.unwrap_err(),
        "tap",
        codes::MOBILE_IOS_STUB,
    );
}

#[tokio::test]
async fn tap_unsupported_corner_coordinates() {
    let client = make_client("ios");
    assert_unsupported(
        client.tap(0, 0).await.unwrap_err(),
        "tap",
        codes::MOBILE_IOS_STUB,
    );
    assert_unsupported(
        client.tap(1079, 1919).await.unwrap_err(),
        "tap",
        codes::MOBILE_IOS_STUB,
    );
}

#[tokio::test]
async fn swipe_is_unsupported() {
    let client = make_client("ios");
    assert_unsupported(
        client.swipe(540, 960, 600, 1000).await.unwrap_err(),
        "swipe",
        codes::MOBILE_IOS_STUB,
    );
}

#[tokio::test]
async fn swipe_unsupported_directions() {
    let client = make_client("android");
    assert_unsupported(
        client.swipe(100, 960, 980, 960).await.unwrap_err(),
        "swipe",
        codes::MOBILE_ANDROID_STUB,
    );
    assert_unsupported(
        client.swipe(540, 100, 540, 1820).await.unwrap_err(),
        "swipe",
        codes::MOBILE_ANDROID_STUB,
    );
}

#[tokio::test]
async fn input_text_is_unsupported() {
    let client = make_client("ios");
    assert_unsupported(
        client.input_text("hello").await.unwrap_err(),
        "input_text",
        codes::MOBILE_IOS_STUB,
    );
}

#[tokio::test]
async fn input_text_unsupported_unicode_and_empty() {
    let client = make_client("android");
    assert_unsupported(
        client.input_text("こんにちは世界 🌍").await.unwrap_err(),
        "input_text",
        codes::MOBILE_ANDROID_STUB,
    );
    assert_unsupported(
        client.input_text("").await.unwrap_err(),
        "input_text",
        codes::MOBILE_ANDROID_STUB,
    );
}

#[tokio::test]
async fn record_event_returns_ok() {
    let client = make_client("ios");
    let event = AutomationEvent::screenshot(Platform::Unknown, "/tmp/screen.png");
    let result = client.record_event(event).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn all_action_methods_fail_loud() {
    let client = make_client("ios");
    assert!(client.get_viewport().await.is_err());
    assert!(client.screenshot("/tmp/s.png").await.is_err());
    assert!(client.tap(100, 200).await.is_err());
    assert!(client.swipe(100, 200, 300, 400).await.is_err());
    assert!(client.input_text("test").await.is_err());
    assert!(client
        .record_event(AutomationEvent::screenshot(Platform::Ios, "/s.png"))
        .await
        .is_ok());
}

#[tokio::test]
async fn mobile_client_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MobileClient>();
}

#[test]
fn code_for_platform_maps_labels() {
    assert_eq!(code_for_platform("ios"), codes::MOBILE_IOS_STUB);
    assert_eq!(code_for_platform("iPhone"), codes::MOBILE_IOS_STUB);
    assert_eq!(code_for_platform("android"), codes::MOBILE_ANDROID_STUB);
    assert_eq!(code_for_platform("harmony"), codes::MOBILE_OTHER_STUB);
    let c = MobileClient::new("android");
    assert_eq!(c.unsupported_code(), codes::MOBILE_ANDROID_STUB);
    assert_eq!(c.platform(), "android");
}
