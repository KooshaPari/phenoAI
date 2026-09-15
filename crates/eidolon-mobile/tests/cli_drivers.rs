//! Feature-gated CLI driver tests (`mobile-ios` / `mobile-android` / `mobile-uia2`).
//!
//! Default `cargo test -p eidolon-mobile --locked` stays green without devices.
//! Hermetic probes run when tools are on PATH. Destructive actions stay gated.
//! Optional: `IOS_MOBILE_INTEGRATION=1` / `ANDROID_MOBILE_INTEGRATION=1` /
//! `ANDROID_UIA2_INTEGRATION=1`.

use eidolon_core::error::PhenoError;
use eidolon_core::traits::MobileAutomator;
use eidolon_mobile::codes;
use eidolon_mobile::{
    android_probe, ios_probe, AndroidStub, IosStub, MissingXcuiBridge, XcuiBridge,
};

fn assert_stub_code(err: PhenoError, expected: &str) {
    assert_eq!(err.unsupported_code(), Some(expected));
    assert_eq!(err.status_code(), 501);
}

#[tokio::test]
async fn stubs_remain_fail_loud_without_feature_clients() {
    assert_stub_code(
        IosStub::new().tap(0, 0).await.unwrap_err(),
        codes::MOBILE_IOS_STUB,
    );
    assert_stub_code(
        AndroidStub::new().tap(0, 0).await.unwrap_err(),
        codes::MOBILE_ANDROID_STUB,
    );
}

#[test]
fn xcui_bridge_missing_is_fail_loud() {
    let bridge = MissingXcuiBridge;
    assert!(!bridge.ready());
    assert_eq!(bridge.backend_name(), "missing");
    let err = bridge.tap("udid", 0, 0).unwrap_err();
    assert_eq!(
        err.unsupported_code(),
        Some(codes::MOBILE_IOS_XCUI_UNAVAILABLE)
    );
}

#[test]
fn probe_tools_are_boolean_consistent() {
    assert_eq!(
        ios_probe::xcrun_ready(),
        ios_probe::resolve_xcrun().is_some()
    );
    assert_eq!(
        android_probe::adb_ready(),
        android_probe::resolve_adb().is_some()
    );
}

#[cfg(feature = "mobile-ios")]
mod ios_feature {
    use super::*;
    use eidolon_core::traits::MobileAutomator;
    use eidolon_mobile::{
        cli, BundleXcuiBridge, InstrumentDiscovery, IosXcTestDriver, MobileDeviceDiscovery,
        XcrunIosDriver,
    };

    #[tokio::test]
    async fn try_new_matches_xcrun_cli() {
        match XcrunIosDriver::try_new() {
            Ok(driver) => {
                assert!(driver.xcrun_ready());
                assert!(ios_probe::xcrun_ready());
                assert!(!driver.cli_version().is_empty());
                driver.hermetic_probe().expect("hermetic probe");
                let devices = driver.list_devices().expect("list_devices");
                // Empty is ok — never invent devices.
                let _ = devices.len();
                // Destructive actions gated without env.
                if !cli::actions_allowed() {
                    let err = driver.tap(1, 2).await.unwrap_err();
                    assert_eq!(err.unsupported_code(), Some(codes::MOBILE_ACTIONS_GATED));
                }
            }
            Err(err) => {
                assert!(!ios_probe::xcrun_cli_ready());
                assert_stub_code(err, codes::MOBILE_IOS_STUB);
            }
        }
    }

    #[test]
    fn instrument_discovery_ios() {
        let disc = InstrumentDiscovery::ios_only();
        match disc.list_connected() {
            Ok(devices) => {
                assert!(disc.instruments_ready() || devices.is_empty());
                for d in &devices {
                    assert_eq!(d.modality, eidolon_mobile::Modality::Mobile);
                }
            }
            Err(err) => {
                assert!(
                    err.unsupported_code() == Some(codes::MOBILE_DISCOVERY_UNAVAILABLE)
                        || err.unsupported_code() == Some(codes::MOBILE_IOS_STUB)
                );
            }
        }
    }

    #[tokio::test]
    async fn gated_tap_without_xcui_bundle_is_xcui_unavailable() {
        if cli::actions_allowed() {
            // Do not mutate global env when another test already enabled actions.
            return;
        }
        let Ok(driver) = XcrunIosDriver::try_new() else {
            return;
        };
        // Force missing bridge regardless of host AppleScript env.
        let driver = driver
            .with_device_id("00000000-0000-0000-0000-000000000000")
            .with_xcui_bridge(Box::new(MissingXcuiBridge));
        std::env::set_var(cli::ACTIONS_ALLOW_ENV, "1");
        let err = driver.tap(10, 20).await.unwrap_err();
        std::env::remove_var(cli::ACTIONS_ALLOW_ENV);
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_IOS_XCUI_UNAVAILABLE)
        );
    }

    #[test]
    fn bundle_xcui_argv_contract() {
        let argv = BundleXcuiBridge::build_argv(
            "swipe",
            "UDID",
            &[
                ("--x1", "1".into()),
                ("--y1", "2".into()),
                ("--x2", "3".into()),
                ("--y2", "4".into()),
            ],
        );
        assert_eq!(argv[0], "swipe");
        assert!(argv.contains(&"--udid".into()));
        assert!(argv.contains(&"UDID".into()));
    }

    #[test]
    fn helper_parse_matches_bridge_argv() {
        use eidolon_mobile::{parse_argv, HelperCommand};
        let argv = BundleXcuiBridge::build_argv(
            "tap",
            "SIM",
            &[("--x", "5".into()), ("--y", "6".into())],
        );
        assert_eq!(
            parse_argv(&argv).unwrap(),
            HelperCommand::Tap {
                udid: "SIM".into(),
                x: 5,
                y: 6,
            }
        );
    }

    #[tokio::test]
    async fn integration_env_documents_xcui_gap_or_runs_helper() {
        if std::env::var("IOS_MOBILE_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let mut driver = XcrunIosDriver::try_new()
            .expect("IOS_MOBILE_INTEGRATION=1 requires reachable xcrun");
        driver.hermetic_probe().expect("hermetic");
        let devices = driver.list_devices().expect("list");
        if let Some(d) = devices.first() {
            driver = driver.with_device_id(d.id.clone());
        }
        driver.refresh_xcui_bridge();
        std::env::set_var(cli::ACTIONS_ALLOW_ENV, "1");
        let err = driver.tap(0, 0).await;
        std::env::remove_var(cli::ACTIONS_ALLOW_ENV);
        match err {
            Ok(()) => {
                // Helper or AppleScript succeeded — only expected when configured.
                assert!(driver.xcui_ready());
            }
            Err(e) => {
                let code = e.unsupported_code();
                assert!(
                    code == Some(codes::MOBILE_IOS_XCUI_UNAVAILABLE)
                        || code == Some(codes::MOBILE_IOS_STUB)
                        || matches!(e, PhenoError::BadRequest(_)),
                    "unexpected err={e:?}"
                );
            }
        }
    }
}

#[cfg(feature = "mobile-android")]
mod android_feature {
    use super::*;
    use eidolon_core::traits::MobileAutomator;
    use eidolon_mobile::{
        cli, AdbAndroidDriver, AndroidUiAutomatorDriver, InstrumentDiscovery,
        MobileDeviceDiscovery,
    };

    #[tokio::test]
    async fn try_new_matches_adb_cli() {
        match AdbAndroidDriver::try_new() {
            Ok(driver) => {
                assert!(driver.adb_ready());
                assert!(android_probe::adb_ready());
                assert!(!driver.cli_version().is_empty());
                driver.hermetic_probe().expect("hermetic probe");
                let devices = driver.list_devices().expect("list_devices");
                let _ = devices.len();
                if !cli::actions_allowed() {
                    let err = driver.tap(1, 2).await.unwrap_err();
                    assert_eq!(err.unsupported_code(), Some(codes::MOBILE_ACTIONS_GATED));
                }
            }
            Err(err) => {
                assert!(!android_probe::adb_cli_ready());
                assert_stub_code(err, codes::MOBILE_ANDROID_STUB);
            }
        }
    }

    #[test]
    fn instrument_discovery_android() {
        let disc = InstrumentDiscovery::android_only();
        match disc.list_connected() {
            Ok(devices) => {
                for d in &devices {
                    assert_eq!(d.platform, "android");
                }
            }
            Err(err) => {
                assert!(
                    err.unsupported_code() == Some(codes::MOBILE_DISCOVERY_UNAVAILABLE)
                        || err.unsupported_code() == Some(codes::MOBILE_ANDROID_STUB)
                );
            }
        }
    }

    #[tokio::test]
    async fn integration_env_requires_device_for_actions() {
        if std::env::var("ANDROID_MOBILE_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let driver = AdbAndroidDriver::try_new()
            .expect("ANDROID_MOBILE_INTEGRATION=1 requires reachable adb");
        let devices = driver.list_devices().expect("list");
        assert!(
            !devices.is_empty(),
            "ANDROID_MOBILE_INTEGRATION=1 requires at least one adb device"
        );
        std::env::set_var(cli::ACTIONS_ALLOW_ENV, "1");
        let driver = driver.with_device_id(devices[0].id.clone());
        // Viewport is read-only (ungated).
        let _ = driver.get_viewport().await;
        // Gated input is real — may succeed or fail from device; never silent Ok without adb.
        let tap = driver.tap(1, 1).await;
        std::env::remove_var(cli::ACTIONS_ALLOW_ENV);
        // If tap failed, it must not be ACTIONS_GATED (gate was on).
        if let Err(e) = tap {
            assert_ne!(e.unsupported_code(), Some(codes::MOBILE_ACTIONS_GATED));
        }
    }

    #[test]
    fn uia2_handle_shares_adb_and_gates_install() {
        let Ok(driver) = AdbAndroidDriver::try_new() else {
            return;
        };
        let uia2 = driver.uia2();
        assert_eq!(uia2.port(), cli::env_uia2_port());
        // Without actions gate, install fails loud (gated or missing APK).
        if !cli::actions_allowed() {
            let err = uia2.install_from_env().unwrap_err();
            let code = err.unsupported_code();
            assert!(
                code == Some(codes::MOBILE_ACTIONS_GATED)
                    || code == Some(codes::MOBILE_UIA2_UNAVAILABLE),
                "unexpected err={err:?}"
            );
        }
        // Packages probe: Ok(false) or Ok(true) or adb error — never invent true without probe.
        match driver.uia2_packages_ready() {
            Ok(true) => {
                // Packages installed; process may or may not be running.
                let _ = driver.uia2_server_ready();
            }
            Ok(false) => {
                assert!(!driver.uia2_server_ready());
            }
            Err(e) => {
                assert!(!driver.uia2_server_ready());
                let code = e.unsupported_code();
                assert!(
                    code == Some(codes::MOBILE_UIA2_UNAVAILABLE)
                        || code == Some(codes::MOBILE_ANDROID_STUB),
                    "unexpected err={e:?}"
                );
            }
        }
    }
}

#[cfg(feature = "mobile-uia2")]
mod uia2_feature {
    use super::*;
    use eidolon_mobile::{cli, AdbAndroidDriver, AndroidUiAutomatorDriver, Uia2ApkPaths};

    #[test]
    fn feature_documents_uia2_path() {
        // Feature compiles AdbAndroidDriver + Uia2Server + Uia2HttpClient wiring.
        assert_eq!(
            codes::MOBILE_UIA2_UNAVAILABLE,
            "EIDOLON_MOBILE_UIA2_UNAVAILABLE"
        );
        let _ = eidolon_mobile::Uia2HttpClient::with_base_url("http://127.0.0.1:6790");
        let appium = eidolon_mobile::AppiumSessionClient::for_appium("http://127.0.0.1:4723");
        assert_eq!(
            appium.wire_mode(),
            eidolon_mobile::SessionWireMode::AppiumW3c
        );
        let w3c = appium.find_element_body("id", "x");
        assert_eq!(w3c["using"], "id");
        assert_eq!(w3c["value"], "x");
        // Extended W3C verbs compile on the same client.
        let body = eidolon_mobile::Uia2HttpClient::pointer_tap_actions(1, 2);
        assert_eq!(body["actions"][0]["type"], "pointer");
        let err = eidolon_mobile::Uia2HttpClient::unsupported("vendorOnly");
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_UIA2_UNAVAILABLE)
        );
        if std::env::var(cli::UIA2_APK_ENV).is_err() {
            // Empty roots → hermetic fail; live resolve may use durable cache.
            let hermetic = eidolon_mobile::uia2_assets::resolve_with_roots(None, None);
            assert_eq!(
                hermetic.unwrap_err().unsupported_code(),
                Some(codes::MOBILE_UIA2_UNAVAILABLE)
            );
            match Uia2ApkPaths::resolve() {
                Ok(paths) => {
                    assert!(paths.server.is_file() && paths.test.is_file());
                }
                Err(err) => {
                    assert_eq!(
                        err.unsupported_code(),
                        Some(codes::MOBILE_UIA2_UNAVAILABLE)
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn integration_uia2_probe_or_install() {
        if std::env::var("ANDROID_UIA2_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let driver = AdbAndroidDriver::try_new()
            .expect("ANDROID_UIA2_INTEGRATION=1 requires reachable adb");
        let devices = driver.list_devices().expect("list");
        assert!(
            !devices.is_empty(),
            "ANDROID_UIA2_INTEGRATION=1 requires at least one adb device"
        );
        let driver = driver.with_device_id(devices[0].id.clone());
        let uia2 = driver.uia2();

        // Probe packages (ungated).
        let status = uia2.packages_installed().expect("pm list packages");
        if !status.both_installed() {
            // Resolved/vendored APKs or env — fail loud only if still unresolved.
            std::env::set_var(cli::ACTIONS_ALLOW_ENV, "1");
            let install = uia2.install_from_env();
            std::env::remove_var(cli::ACTIONS_ALLOW_ENV);
            match install {
                Ok(()) => {
                    let after = uia2.packages_installed().expect("re-probe");
                    assert!(after.both_installed());
                }
                Err(e) => {
                    let code = e.unsupported_code();
                    assert!(
                        code == Some(codes::MOBILE_UIA2_UNAVAILABLE)
                            || code == Some(codes::MOBILE_ACTIONS_GATED),
                        "install miss must be UIA2_UNAVAILABLE (or gated), got {e:?}"
                    );
                }
            }
            return;
        }

        // Packages present — readiness is process-based; never invent.
        let _ = driver.uia2_server_ready();
        assert!(driver.uiautomator_ready());
        assert!(driver.adb_ready());

        // HTTP client: fail-loud when server not ready; when ready, status probe.
        match driver.uia2_http() {
            Ok(client) => {
                // Server process claimed ready — HTTP may still fail if port not forwarded.
                let _ = client.status();
            }
            Err(e) => {
                assert_eq!(
                    e.unsupported_code(),
                    Some(codes::MOBILE_UIA2_UNAVAILABLE),
                    "uia2_http without ready server must be UIA2_UNAVAILABLE, got {e:?}"
                );
            }
        }
    }
}

#[cfg(feature = "mobile-appium")]
mod appium_feature {
    use super::*;
    use eidolon_mobile::{
        appium_tools_ready, require_appium_ready, AppiumProbe, APPIUM_URL_ENV, DEFAULT_APPIUM_URL,
    };
    use std::time::Duration;

    #[test]
    fn probe_is_hermetic_and_documents_code() {
        assert_eq!(
            codes::MOBILE_APPIUM_UNAVAILABLE,
            "EIDOLON_MOBILE_APPIUM_UNAVAILABLE"
        );
        assert_eq!(DEFAULT_APPIUM_URL, "http://127.0.0.1:4723");
        assert_eq!(APPIUM_URL_ENV, "EIDOLON_APPIUM_URL");
        let probe = AppiumProbe::discover();
        assert_eq!(probe.tools_present(), appium_tools_ready());
        if !probe.tools_present() {
            let err = require_appium_ready().unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::MOBILE_APPIUM_UNAVAILABLE)
            );
            let err = probe.http_status(Duration::from_millis(20)).unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::MOBILE_APPIUM_UNAVAILABLE)
            );
        }
    }
}
