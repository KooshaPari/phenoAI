//! Hermetic virtual display manager tests (all platforms).
//!
//! macOS/Windows: fail-loud `EIDOLON_SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED`.
//! Linux without Xvfb: `EIDOLON_SANDBOX_XVFB_MISSING`. Linux with Xvfb but
//! without integration env: `EIDOLON_SANDBOX_VIRTUAL_DISPLAY_STUB`.
//! Live spawn: `XVFB_INTEGRATION=1` + feature `sandbox-virtual-display`
//! (see `tests/virtual_display_live.rs` when added on Linux CI).

use eidolon_core::error::PhenoError;
use eidolon_sandbox::codes;
use eidolon_sandbox::{
    virtual_display_probe, VirtualDisplayConfig, VirtualDisplayManager, VirtualDisplayStub,
    XVFB_INTEGRATION_ENV,
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
fn probe_helpers_are_boolean() {
    let _ = virtual_display_probe::xvfb_ready();
    let _ = virtual_display_probe::x11vnc_ready();
    let _ = virtual_display_probe::xvnc_ready();
    let _ = virtual_display_probe::vnc_server_ready();
    let _ = virtual_display_probe::wayland_compositor_ready();
}

#[test]
fn documented_virtual_display_codes_are_stable() {
    assert_eq!(
        codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED,
        "EIDOLON_SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_VIRTUAL_DISPLAY_STUB,
        "EIDOLON_SANDBOX_VIRTUAL_DISPLAY_STUB"
    );
    assert_eq!(codes::SANDBOX_XVFB_MISSING, "EIDOLON_SANDBOX_XVFB_MISSING");
    assert_eq!(
        codes::SANDBOX_XVFB_SPAWN_IO,
        "EIDOLON_SANDBOX_XVFB_SPAWN_IO"
    );
    assert_eq!(
        codes::SANDBOX_VNC_UNSUPPORTED,
        "EIDOLON_SANDBOX_VNC_UNSUPPORTED"
    );
    assert_eq!(
        codes::SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED,
        "EIDOLON_SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED"
    );
}

#[test]
fn stub_fail_loud() {
    let stub = VirtualDisplayStub::new();
    assert_code(
        stub.start().unwrap_err(),
        codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED,
    );
    assert_code(
        stub.stop().unwrap_err(),
        codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED,
    );
}

#[tokio::test]
async fn manager_start_fail_loud_contract() {
    let mgr = VirtualDisplayManager::with_defaults().unwrap();
    assert_eq!(mgr.platform_supported(), cfg!(target_os = "linux"));

    if !cfg!(target_os = "linux") {
        assert_code(
            mgr.start().unwrap_err(),
            codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED,
        );
        return;
    }

    if !virtual_display_probe::xvfb_ready() {
        assert_code(mgr.start().unwrap_err(), codes::SANDBOX_XVFB_MISSING);
        return;
    }

    let saved = std::env::var(XVFB_INTEGRATION_ENV).ok();
    std::env::remove_var(XVFB_INTEGRATION_ENV);
    assert_code(
        mgr.start().unwrap_err(),
        codes::SANDBOX_VIRTUAL_DISPLAY_STUB,
    );
    if let Some(v) = saved {
        std::env::set_var(XVFB_INTEGRATION_ENV, v);
    }
}

#[test]
fn vnc_port_request_fail_loud() {
    let mut cfg = VirtualDisplayConfig::default();
    cfg.vnc_port = Some(5900);
    let mgr = VirtualDisplayManager::new(cfg).unwrap();
    let err = mgr.start().unwrap_err();
    if cfg!(target_os = "linux") {
        assert_code(err, codes::SANDBOX_VNC_UNSUPPORTED);
    } else {
        assert_code(err, codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED);
    }
}

#[test]
fn wayland_isolation_request_fail_loud() {
    let mut cfg = VirtualDisplayConfig::default();
    cfg.wayland_isolation = true;
    let mgr = VirtualDisplayManager::new(cfg).unwrap();
    let err = mgr.start().unwrap_err();
    if cfg!(target_os = "linux") {
        assert_code(err, codes::SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED);
    } else {
        assert_code(err, codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED);
    }
}

#[test]
fn plan_xvfb_hermetic_on_linux_when_tool_present() {
    if !cfg!(target_os = "linux") {
        let mgr = VirtualDisplayManager::with_defaults().unwrap();
        assert_code(
            mgr.plan_xvfb().unwrap_err(),
            codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED,
        );
        return;
    }
    let mgr = VirtualDisplayManager::with_defaults().unwrap();
    match mgr.plan_xvfb() {
        Ok(plan) => {
            assert!(plan.display.starts_with(':'));
            assert!(!plan.argv.is_empty());
        }
        Err(e) => assert_code(e, codes::SANDBOX_XVFB_MISSING),
    }
}
