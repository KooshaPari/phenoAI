//! VNC isolation scaffolding (x11vnc / TigerVNC attach).
//!
//! # Honesty
//!
//! Probes are always-on. Live VNC attach is **not** wired yet — callers receive
//! fail-loud [`crate::codes::SANDBOX_VNC_UNSUPPORTED`]. Do not return empty
//! success when a VNC port was requested.

use super::probe;
use super::{platform_unsupported, VirtualDisplayConfig, VirtualDisplayHandle};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

fn vnc_unsupported(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_VNC_UNSUPPORTED,
        format!(
            "VirtualDisplayManager::{method} — VNC isolation not wired ({detail}); \
             Xvfb path is live on Linux; x11vnc/Xvnc probes: x11vnc={}, xvnc={}; \
             see docs/guides/virtual-display.md",
            probe::x11vnc_ready(),
            probe::xvnc_ready()
        ),
    )
}

/// Whether a VNC server binary is discoverable on the host.
pub fn vnc_tools_ready() -> bool {
    probe::vnc_server_ready()
}

/// Planned VNC attach parameters (documentation / future wiring).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VncAttachPlan {
    pub display: String,
    pub listen_host: String,
    pub rfb_port: u16,
    pub tool: String,
}

/// Build a VNC attach plan for an existing X display (hermetic).
pub fn plan_vnc_attach(xvfb: &VirtualDisplayHandle, rfb_port: u16) -> Result<VncAttachPlan> {
    if !super::platform_is_linux() {
        return Err(platform_unsupported("plan_vnc_attach"));
    }
    if xvfb.kind != super::VirtualDisplayKind::Xvfb {
        return Err(PhenoError::BadRequest(
            "plan_vnc_attach requires an Xvfb VirtualDisplayHandle".into(),
        ));
    }
    let tool = if probe::x11vnc_ready() {
        "x11vnc".into()
    } else if probe::xvnc_ready() {
        "Xvnc".into()
    } else {
        return Err(vnc_unsupported(
            "plan_vnc_attach",
            "no x11vnc or Xvnc binary on PATH (EIDOLON_X11VNC / EIDOLON_XVNC overrides)",
        ));
    };
    Ok(VncAttachPlan {
        display: xvfb.display.clone(),
        listen_host: "127.0.0.1".into(),
        rfb_port,
        tool,
    })
}

/// Start VNC isolation for an Xvfb session — fail-loud until wired.
pub fn start_vnc(_config: &VirtualDisplayConfig, _xvfb: &VirtualDisplayHandle) -> Result<()> {
    if !super::platform_is_linux() {
        return Err(platform_unsupported("start_vnc"));
    }
    Err(vnc_unsupported(
        "start_vnc",
        "live x11vnc attach not implemented — use Xvfb DISPLAY for in-process X11 clients",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;

    #[test]
    fn start_vnc_fail_loud() {
        let handle = VirtualDisplayHandle {
            kind: super::super::VirtualDisplayKind::Xvfb,
            display: ":99".into(),
            width: 1280,
            height: 720,
            color_depth: 24,
            vnc_port: None,
            wayland_socket: None,
        };
        let err = start_vnc(&VirtualDisplayConfig::default(), &handle).unwrap_err();
        if super::super::platform_is_linux() {
            assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_VNC_UNSUPPORTED));
        } else {
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED)
            );
        }
    }
}
