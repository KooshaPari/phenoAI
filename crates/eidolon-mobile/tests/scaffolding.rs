//! Cross-target scaffolding tests for iOS/Android/discovery hooks.
//!
//! These compile and run on **all** platforms so CI/local stays green while
//! asserting fail-loud contracts for T1 mobile stubs + probe consistency.

use eidolon_core::error::PhenoError;
use eidolon_core::traits::MobileAutomator;
use eidolon_mobile::codes;
use eidolon_mobile::{
    android_probe, ios_probe, AndroidStub, AndroidUiAutomatorDriver, DiscoveryStub, IosStub,
    IosXcTestDriver, MobileDeviceDiscovery,
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
async fn ios_stub_fail_loud() {
    let stub = IosStub::new();
    assert!(!stub.xctest_ready());
    assert!(!stub.xcrun_ready());

    assert_code(
        stub.get_viewport().await.unwrap_err(),
        codes::MOBILE_IOS_STUB,
    );
    assert_code(
        stub.screenshot("/tmp/ios.png").await.unwrap_err(),
        codes::MOBILE_IOS_STUB,
    );
    assert_code(stub.tap(0, 0).await.unwrap_err(), codes::MOBILE_IOS_STUB);
    assert_code(
        stub.swipe(0, 0, 10, 10).await.unwrap_err(),
        codes::MOBILE_IOS_STUB,
    );
    assert_code(
        stub.input_text("x").await.unwrap_err(),
        codes::MOBILE_IOS_STUB,
    );
}

#[tokio::test]
async fn android_stub_fail_loud() {
    let stub = AndroidStub::new();
    assert!(!stub.uiautomator_ready());
    assert!(!stub.adb_ready());
    assert!(!stub.uia2_server_ready());

    assert_code(
        stub.get_viewport().await.unwrap_err(),
        codes::MOBILE_ANDROID_STUB,
    );
    assert_code(
        stub.screenshot("/tmp/and.png").await.unwrap_err(),
        codes::MOBILE_ANDROID_STUB,
    );
    assert_code(stub.tap(1, 2).await.unwrap_err(), codes::MOBILE_ANDROID_STUB);
}

#[test]
fn discovery_stub_fail_loud() {
    let stub = DiscoveryStub::new();
    assert!(!stub.instruments_ready());
    assert_code(
        stub.list_connected().unwrap_err(),
        codes::MOBILE_DISCOVERY_UNAVAILABLE,
    );
}

#[test]
fn documented_codes_are_stable() {
    assert_eq!(codes::MOBILE_IOS_STUB, "EIDOLON_MOBILE_IOS_STUB");
    assert_eq!(codes::MOBILE_ANDROID_STUB, "EIDOLON_MOBILE_ANDROID_STUB");
    assert_eq!(codes::MOBILE_OTHER_STUB, "EIDOLON_MOBILE_OTHER_STUB");
    assert_eq!(
        codes::MOBILE_DISCOVERY_UNAVAILABLE,
        "EIDOLON_MOBILE_DISCOVERY_UNAVAILABLE"
    );
    assert_eq!(codes::MOBILE_ACTIONS_GATED, "EIDOLON_MOBILE_ACTIONS_GATED");
    assert_eq!(
        codes::MOBILE_IOS_XCUI_UNAVAILABLE,
        "EIDOLON_MOBILE_IOS_XCUI_UNAVAILABLE"
    );
    assert_eq!(
        codes::MOBILE_UIA2_UNAVAILABLE,
        "EIDOLON_MOBILE_UIA2_UNAVAILABLE"
    );
    assert_eq!(
        codes::MOBILE_APPIUM_UNAVAILABLE,
        "EIDOLON_MOBILE_APPIUM_UNAVAILABLE"
    );
}

#[test]
fn ios_probe_consistency() {
    let ready = ios_probe::xcrun_ready();
    assert_eq!(ready, ios_probe::resolve_xcrun().is_some());
    if ios_probe::xcrun_cli_ready() {
        assert!(ready);
        assert!(ios_probe::xcrun_version_line().is_some());
    }
}

#[test]
fn android_probe_consistency() {
    let ready = android_probe::adb_ready();
    assert_eq!(ready, android_probe::resolve_adb().is_some());
    if android_probe::adb_cli_ready() {
        assert!(ready);
        assert!(android_probe::adb_version_line().is_some());
    }
}

#[test]
fn uia2_hermetic_parsers_and_missing_apk() {
    use eidolon_mobile::{
        parse_package_status, uia2_server, Uia2ApkPaths, DEFAULT_SERVER_PACKAGE,
    };

    let status = parse_package_status(&format!(
        "package:{DEFAULT_SERVER_PACKAGE}\npackage:io.appium.uiautomator2.server.test\n"
    ));
    assert!(status.both_installed());

    let args = uia2_server::forward_port_args(6790, 6790);
    assert_eq!(args, vec!["forward", "tcp:6790", "tcp:6790"]);

    if std::env::var(eidolon_mobile::cli::UIA2_APK_ENV).is_err() {
        // Hermetic: empty roots → fail loud. Real resolve may hit durable cache.
        let err = eidolon_mobile::uia2_assets::resolve_with_roots(None, None).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_UIA2_UNAVAILABLE)
        );
        match Uia2ApkPaths::resolve() {
            Ok(paths) => {
                assert!(paths.server.is_file());
                assert!(paths.test.is_file());
            }
            Err(e) => {
                assert_eq!(
                    e.unsupported_code(),
                    Some(codes::MOBILE_UIA2_UNAVAILABLE)
                );
            }
        }
    }
}
