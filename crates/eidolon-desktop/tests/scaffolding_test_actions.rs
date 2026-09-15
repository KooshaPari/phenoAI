//! Live-platform integration tests for Windows/Linux/Wayland/AT-SPI drivers.
//!
//! These tests exercise real desktop backends and are gated on feature +
//! target_os + env flags. They compile on all platforms but the `#[cfg]`
//! attributes keep the bodies inactive where the platform is unavailable.

use eidolon_core::error::PhenoError;
use eidolon_core::{PointerInput, TextInput};
use eidolon_desktop::codes;

fn assert_code(err: PhenoError, expected: &str) {
    assert_eq!(err.unsupported_code(), Some(expected));
    assert_eq!(err.status_code(), 501);
}

/// Windows-only live capture smoke. Requires both smoke + actions env.
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
mod windows_capture_smoke {
    use std::path::PathBuf;

    use eidolon_desktop::{
        actions_allowed, capture_smoke_requested, require_windows_capture_smoke, WindowsClient,
    };

    use super::*;

    #[tokio::test]
    async fn live_screenshot_when_smoke_env() {
        if !capture_smoke_requested() || !actions_allowed() {
            if !capture_smoke_requested() {
                assert_code(
                    require_windows_capture_smoke().unwrap_err(),
                    codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE,
                );
            }
            return;
        }
        require_windows_capture_smoke().expect("windows capture host");
        let client = WindowsClient::new().expect("construct");
        assert!(client.capture_ready());
        let dir = std::env::temp_dir().join("eidolon-win-capture-smoke");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("smoke.bmp");
        client
            .screenshot(path.to_str().expect("utf8 path"))
            .await
            .expect("live DXGI/GDI screenshot");
        assert!(path.is_file());
        let bytes = std::fs::read(&path).expect("read bmp");
        assert_eq!(&bytes[0..2], b"BM");
        let bi_height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
        assert!(bi_height < 0, "top-down BMP requires negative biHeight");
        let _ = std::fs::remove_file(&path);
        let _ = PathBuf::from(&dir);
    }
}

/// Windows-only live-surface smoke (feature + OS). Does not inject input
/// unless `EIDOLON_DESKTOP_ALLOW_ACTIONS=1` — viewport stays ungated.
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
mod windows_driver {
    use eidolon_desktop::{actions_allowed, WindowsClient};

    use super::*;

    #[tokio::test]
    async fn send_input_ready_and_viewport() {
        let client = WindowsClient::new().expect("construct");
        assert!(client.send_input_ready());
        assert!(client.capture_ready());
        let vp = client.get_viewport().await.expect("viewport");
        assert!(vp.width > 0 && vp.height > 0);
    }

    #[tokio::test]
    async fn pointer_gated_without_env() {
        if actions_allowed() {
            return;
        }
        let client = WindowsClient::new().expect("construct");
        assert_code(
            client
                .pointer(&PointerInput::click(1, 1))
                .await
                .unwrap_err(),
            codes::DESKTOP_ACTIONS_GATED,
        );
        assert_code(
            client.text(&TextInput::keystroke("x")).await.unwrap_err(),
            codes::DESKTOP_ACTIONS_GATED,
        );
        assert_code(
            client
                .screenshot("C:\\Windows\\Temp\\eidolon-test.bmp")
                .await
                .unwrap_err(),
            codes::DESKTOP_ACTIONS_GATED,
        );
    }
}

/// Linux-only live-surface smoke (feature + OS). Does not inject input
/// unless `EIDOLON_DESKTOP_ALLOW_ACTIONS=1` — viewport stays ungated when
/// `DISPLAY` is available.
#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
mod linux_driver {
    use eidolon_core::traits::DesktopAutomator;
    use eidolon_desktop::{actions_allowed, LinuxClient};

    use super::*;

    #[tokio::test]
    async fn x11_and_wayland_ready_flags() {
        let client = LinuxClient::new().expect("construct");
        assert!(client.x11_ready());
        assert!(client.wayland_ready());
    }

    #[tokio::test]
    async fn pointer_gated_without_env() {
        if actions_allowed() {
            return;
        }
        let client = LinuxClient::new().expect("construct");
        assert_code(
            client
                .pointer(&PointerInput::click(1, 1))
                .await
                .unwrap_err(),
            codes::DESKTOP_ACTIONS_GATED,
        );
        assert_code(
            client.text(&TextInput::keystroke("x")).await.unwrap_err(),
            codes::DESKTOP_ACTIONS_GATED,
        );
        assert_code(
            client
                .screenshot("/tmp/eidolon-test.bmp")
                .await
                .unwrap_err(),
            codes::DESKTOP_ACTIONS_GATED,
        );
    }

    #[tokio::test]
    async fn viewport_when_display_present() {
        if std::env::var_os("DISPLAY").is_none() {
            let client = LinuxClient::new().expect("construct");
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                if !actions_allowed() {
                    assert_code(
                        client
                            .pointer(&PointerInput::click(1, 1))
                            .await
                            .unwrap_err(),
                        codes::DESKTOP_ACTIONS_GATED,
                    );
                } else {
                    match client.pointer(&PointerInput::click(1, 1)).await {
                        Ok(()) => {}
                        Err(err) => {
                            let code = err.unsupported_code();
                            assert!(
                                code == Some(codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE)
                                    || code == Some(codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED),
                                "unexpected Wayland pointer error: {err:?}"
                            );
                        }
                    }
                }
                match client.get_viewport().await {
                    Ok(vp) => assert!(vp.width > 0 && vp.height > 0),
                    Err(err) => {
                        assert_eq!(
                            err.unsupported_code(),
                            Some(codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE)
                        );
                    }
                }
            } else {
                let err = client.get_viewport().await.unwrap_err();
                assert!(matches!(err, PhenoError::Platform(_)));
            }
            return;
        }
        let client = LinuxClient::new().expect("construct");
        let vp = client.get_viewport().await.expect("viewport");
        assert!(vp.width > 0 && vp.height > 0);
    }
}

/// Pure-Wayland live inject/restore smoke (Linux + `desktop-linux` only).
/// Requires `WAYLAND_DISPLAY`, `EIDOLON_DESKTOP_WAYLAND_SMOKE=1`, and
/// `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`. Optional restore:
/// `EIDOLON_DESKTOP_WAYLAND_RESTORE=1`.
#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
mod wayland_inject_smoke {
    use eidolon_desktop::{
        require_wayland_smoke, wayland_inject_miss_codes, wayland_smoke_requested, LinuxClient,
    };

    use super::*;

    fn assert_wayland_inject_miss(err: PhenoError) {
        let code = err
            .unsupported_code()
            .expect("Wayland inject miss must be UnsupportedPlatform");
        assert!(
            wayland_inject_miss_codes().contains(&code),
            "unexpected Wayland inject error code {code}: {err:?}"
        );
    }

    #[tokio::test]
    async fn live_pointer_text_or_portal_deny_when_smoke_env() {
        if !wayland_smoke_requested() || !eidolon_desktop::actions_allowed() {
            if !wayland_smoke_requested() {
                assert_code(
                    require_wayland_smoke().unwrap_err(),
                    codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
                );
            }
            return;
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            assert_code(
                require_wayland_smoke().unwrap_err(),
                codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
            );
            return;
        }
        require_wayland_smoke().expect("Wayland smoke host gate");

        let client = LinuxClient::new().expect("construct");
        match client.pointer(&PointerInput::click(1, 1)).await {
            Ok(()) => {}
            Err(err) => assert_wayland_inject_miss(err),
        }
        match client.text(&TextInput::keystroke("x")).await {
            Ok(()) => {}
            Err(err) => assert_wayland_inject_miss(err),
        }
    }

    #[tokio::test]
    async fn live_restore_path_when_restore_env() {
        use eidolon_desktop::linux::wayland_restore_token::restore_enabled;

        if !wayland_smoke_requested() || !eidolon_desktop::actions_allowed() || !restore_enabled() {
            return;
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return;
        }
        let client = LinuxClient::new().expect("construct");
        match client.pointer(&PointerInput::click(2, 2)).await {
            Ok(()) => {}
            Err(err) => assert_wayland_inject_miss(err),
        }
    }
}

/// Linux AT-SPI live-surface smoke (feature + OS). Activate stays gated
/// unless `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
#[cfg(all(target_os = "linux", feature = "desktop-linux-atspi"))]
mod linux_atspi {
    use eidolon_desktop::{actions_allowed, AtspiClient};

    use super::*;

    #[tokio::test]
    async fn connect_ready_or_unavailable() {
        match AtspiClient::connect().await {
            Ok(client) => {
                assert!(client.atspi_ready());
                let _ = client.list_nodes(4).await.expect("list after connect");
            }
            Err(err) => {
                assert_eq!(
                    err.unsupported_code(),
                    Some(codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE)
                );
            }
        }
    }

    #[tokio::test]
    async fn activate_gated_without_env() {
        if actions_allowed() {
            return;
        }
        let Ok(client) = AtspiClient::connect().await else {
            return;
        };
        let node = eidolon_desktop::AtspiNode {
            role: "push button".into(),
            name: "OK".into(),
            path: "/org/a11y/atspi/accessible/null".into(),
            bus_name: "org.a11y.atspi.Registry".into(),
            description: String::new(),
            states: vec![],
        };
        assert_code(
            client.activate(&node).await.unwrap_err(),
            codes::DESKTOP_ACTIONS_GATED,
        );
    }
}
