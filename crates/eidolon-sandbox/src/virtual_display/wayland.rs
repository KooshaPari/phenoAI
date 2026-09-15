//! Wayland compositor isolation scaffolding (Weston / headless compositors).
//!
//! # Honesty
//!
//! Probes detect compositor binaries on `PATH`. Live isolated compositor sessions
//! are **not** wired — fail-loud [`crate::codes::SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED`].

use super::probe;
use super::{platform_unsupported, VirtualDisplayConfig};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

fn wayland_unsupported(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED,
        format!(
            "VirtualDisplayManager::{method} — Wayland compositor isolation not wired \
             ({detail}); compositor probe={}; see docs/guides/virtual-display.md",
            probe::wayland_compositor_ready()
        ),
    )
}

/// Whether a compositor binary appears on `PATH`.
pub fn compositor_tools_ready() -> bool {
    probe::wayland_compositor_ready()
}

/// Planned headless Weston argv (hermetic documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandCompositorPlan {
    pub binary: String,
    pub socket_name: String,
    pub width: u32,
    pub height: u32,
}

/// Build a compositor launch plan (hermetic — does not spawn).
pub fn plan_wayland_compositor(config: &VirtualDisplayConfig) -> Result<WaylandCompositorPlan> {
    if !super::platform_is_linux() {
        return Err(platform_unsupported("plan_wayland_compositor"));
    }
    let Some(path) = probe::resolve_wayland_compositor_cli() else {
        return Err(wayland_unsupported(
            "plan_wayland_compositor",
            "no weston/mutter/cage binary on PATH (EIDOLON_WESTON override)",
        ));
    };
    let binary = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("weston")
        .to_string();
    Ok(WaylandCompositorPlan {
        binary,
        socket_name: format!("eidolon-wayland-{}", config.display_num),
        width: config.width,
        height: config.height,
    })
}

/// Start an isolated Wayland compositor — fail-loud until wired.
pub fn start_wayland_compositor(_config: &VirtualDisplayConfig) -> Result<()> {
    if !super::platform_is_linux() {
        return Err(platform_unsupported("start_wayland_compositor"));
    }
    Err(wayland_unsupported(
        "start_wayland_compositor",
        "live headless compositor session not implemented — use eidolon-desktop Wayland \
         portal path on host sessions or Xvfb for sandbox X11",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;

    #[test]
    fn start_wayland_fail_loud() {
        let err = start_wayland_compositor(&VirtualDisplayConfig::default()).unwrap_err();
        if super::super::platform_is_linux() {
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED)
            );
        } else {
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED)
            );
        }
    }
}
