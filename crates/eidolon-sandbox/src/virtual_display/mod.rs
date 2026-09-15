//! Virtual display manager (PlayCua / KDesktopVirt extract — A+ track).
//!
//! # Status
//!
//! - Always-on: [`probe`] helpers, [`VirtualDisplayConfig`], hermetic
//!   [`xvfb::plan_xvfb`], fail-loud [`VirtualDisplayStub`].
//! - Feature `sandbox-virtual-display`: live Linux **Xvfb** spawn when
//!   [`XVFB_INTEGRATION_ENV`]=`1`. Wraps system `Xvfb` (or `EIDOLON_XVFB`).
//! - **VNC** (x11vnc / TigerVNC): probes only; attach →
//!   [`codes::SANDBOX_VNC_UNSUPPORTED`] (no silent success).
//! - **Wayland compositor isolation** (Weston / mutter / cage): probes only;
//!   start → [`codes::SANDBOX_WAYLAND_COMPOSITOR_UNSUPPORTED`].
//! - macOS / Windows: fail-loud [`codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED`].
//!
//! Compose with [`PlayCuaDispatcher`](crate::playcua_dispatcher::PlayCuaDispatcher)
//! by exporting `DISPLAY` / `WAYLAND_DISPLAY` from a started session.
//!
//! See `docs/guides/virtual-display.md` and `docs/EXTRACTION_PLAN.md`.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::process::Child;
use std::sync::Mutex;

pub mod probe;
pub mod vnc;
pub mod wayland;
pub mod xvfb;

/// Env gate for live Xvfb spawn (`XVFB_INTEGRATION=1`).
pub const XVFB_INTEGRATION_ENV: &str = "XVFB_INTEGRATION";

/// Default X display number (`:99`) when unset.
pub const DEFAULT_DISPLAY_NUM_ENV: &str = "EIDOLON_VIRTUAL_DISPLAY_NUM";

/// Default virtual display resolution (1280×720×24).
pub const DEFAULT_WIDTH: u32 = 1280;
pub const DEFAULT_HEIGHT: u32 = 720;
pub const DEFAULT_COLOR_DEPTH: u8 = 24;
pub const DEFAULT_DISPLAY_NUM: u32 = 99;

/// Which backend produced a [`VirtualDisplayHandle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualDisplayKind {
    Xvfb,
    Vnc,
    WaylandCompositor,
}

/// Session configuration for a virtual display stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualDisplayConfig {
    pub width: u32,
    pub height: u32,
    pub color_depth: u8,
    pub display_num: u32,
    /// Optional RFB port when callers request VNC (attach not wired — fail-loud).
    pub vnc_port: Option<u16>,
    /// Request Wayland compositor isolation (not wired — fail-loud).
    pub wayland_isolation: bool,
}

impl Default for VirtualDisplayConfig {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            color_depth: DEFAULT_COLOR_DEPTH,
            display_num: display_num_from_env().unwrap_or(DEFAULT_DISPLAY_NUM),
            vnc_port: None,
            wayland_isolation: false,
        }
    }
}

impl VirtualDisplayConfig {
    pub fn display_string(&self) -> String {
        format!(":{}", self.display_num)
    }

    /// Validate geometry and display number.
    pub fn validate(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(PhenoError::BadRequest(
                "virtual display width/height must be non-zero".into(),
            ));
        }
        if !(8..=32).contains(&self.color_depth) {
            return Err(PhenoError::BadRequest(
                "virtual display color_depth must be 8..=32".into(),
            ));
        }
        if self.display_num > 999 {
            return Err(PhenoError::BadRequest(
                "virtual display display_num must be <= 999".into(),
            ));
        }
        Ok(())
    }
}

/// Handle describing a started (or planned) virtual display session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualDisplayHandle {
    pub kind: VirtualDisplayKind,
    pub display: String,
    pub width: u32,
    pub height: u32,
    pub color_depth: u8,
    pub vnc_port: Option<u16>,
    pub wayland_socket: Option<String>,
}

impl VirtualDisplayHandle {
    /// `DISPLAY` value for X11 clients (`:99`).
    pub fn display_env(&self) -> (&'static str, String) {
        ("DISPLAY", self.display.clone())
    }

    /// `WAYLAND_DISPLAY` when a compositor socket is active.
    pub fn wayland_env(&self) -> Option<(&'static str, String)> {
        self.wayland_socket
            .as_ref()
            .map(|s| ("WAYLAND_DISPLAY", s.clone()))
    }
}

struct ActiveSession {
    handle: VirtualDisplayHandle,
    child: Child,
}

/// Virtual display manager — coordinates Xvfb / VNC / Wayland isolation.
pub struct VirtualDisplayManager {
    config: VirtualDisplayConfig,
    active: Mutex<Option<ActiveSession>>,
}

impl VirtualDisplayManager {
    pub fn new(config: VirtualDisplayConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            active: Mutex::new(None),
        })
    }

    pub fn with_defaults() -> Result<Self> {
        Self::new(VirtualDisplayConfig::default())
    }

    pub fn config(&self) -> &VirtualDisplayConfig {
        &self.config
    }

    /// Whether virtual display tooling is supported on this host OS.
    pub fn platform_supported(&self) -> bool {
        platform_is_linux()
    }

    pub fn xvfb_ready(&self) -> bool {
        platform_is_linux() && probe::xvfb_ready()
    }

    pub fn vnc_tools_ready(&self) -> bool {
        platform_is_linux() && vnc::vnc_tools_ready()
    }

    pub fn wayland_compositor_ready(&self) -> bool {
        platform_is_linux() && wayland::compositor_tools_ready()
    }

    /// Hermetic Xvfb argv plan (no spawn).
    pub fn plan_xvfb(&self) -> Result<xvfb::XvfbLaunchPlan> {
        xvfb::plan_xvfb(&self.config)
    }

    /// Start the configured virtual display stack.
    pub fn start(&self) -> Result<VirtualDisplayHandle> {
        if !platform_is_linux() {
            return Err(platform_unsupported("start"));
        }
        if self.config.wayland_isolation {
            wayland::start_wayland_compositor(&self.config)?;
        }
        if self.config.vnc_port.is_some() {
            // Fail loud before Xvfb when VNC was requested — never pretend RFB is up.
            return Err(vnc::start_vnc(
                &self.config,
                &VirtualDisplayHandle {
                    kind: VirtualDisplayKind::Xvfb,
                    display: self.config.display_string(),
                    width: self.config.width,
                    height: self.config.height,
                    color_depth: self.config.color_depth,
                    vnc_port: self.config.vnc_port,
                    wayland_socket: None,
                },
            )
            .unwrap_err());
        }
        let (handle, child) = xvfb::spawn_xvfb(&self.config)?;
        let mut guard = self
            .active
            .lock()
            .map_err(|_| PhenoError::Internal("virtual display session lock poisoned".into()))?;
        if guard.is_some() {
            let _ = xvfb::stop_xvfb_child(child);
            return Err(PhenoError::BadRequest(
                "virtual display session already active — stop first".into(),
            ));
        }
        *guard = Some(ActiveSession {
            handle: handle.clone(),
            child,
        });
        Ok(handle)
    }

    /// Stop the active session, if any.
    pub fn stop(&self) -> Result<()> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| PhenoError::Internal("virtual display session lock poisoned".into()))?;
        if let Some(session) = guard.take() {
            xvfb::stop_xvfb_child(session.child)?;
        }
        Ok(())
    }

    /// Borrow the active handle without stopping.
    pub fn active_handle(&self) -> Result<Option<VirtualDisplayHandle>> {
        let guard = self
            .active
            .lock()
            .map_err(|_| PhenoError::Internal("virtual display session lock poisoned".into()))?;
        Ok(guard.as_ref().map(|s| s.handle.clone()))
    }
}

/// Fail-loud stub when callers need a trait-shaped placeholder.
#[derive(Debug, Default, Clone)]
pub struct VirtualDisplayStub;

impl VirtualDisplayStub {
    pub fn new() -> Self {
        Self
    }

    pub fn start(&self) -> Result<VirtualDisplayHandle> {
        Err(platform_unsupported("VirtualDisplayStub::start"))
    }

    pub fn stop(&self) -> Result<()> {
        Err(platform_unsupported("VirtualDisplayStub::stop"))
    }
}

pub(crate) fn platform_is_linux() -> bool {
    cfg!(target_os = "linux")
}

pub(crate) fn platform_unsupported(method: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED,
        format!(
            "VirtualDisplayManager::{method} unsupported on this platform — virtual \
             display isolation requires Linux + Xvfb (feature `sandbox-virtual-display`, \
             {XVFB_INTEGRATION_ENV}=1 for live spawn); macOS/Windows fail-loud; \
             docs/guides/virtual-display.md"
        ),
    )
}

pub(crate) fn xvfb_missing(method: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_XVFB_MISSING,
        format!(
            "VirtualDisplayManager::{method} — Xvfb not found on PATH (set {}/{}); \
             install `xvfb` package or set EIDOLON_XVFB",
            probe::XVFB_PATH_ENV,
            probe::XVFB_CANDIDATES.join("|")
        ),
    )
}

fn display_num_from_env() -> Option<u32> {
    std::env::var(DEFAULT_DISPLAY_NUM_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;

    #[test]
    fn default_config_validates() {
        let cfg = VirtualDisplayConfig::default();
        cfg.validate().unwrap();
        assert_eq!(cfg.display_string(), ":99");
    }

    #[test]
    fn stub_fail_loud() {
        let stub = VirtualDisplayStub::new();
        let err = stub.start().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED)
        );
    }

    #[test]
    fn manager_start_fail_loud_on_unsupported_platform_or_gated() {
        let mgr = VirtualDisplayManager::with_defaults().unwrap();
        let err = mgr.start().unwrap_err();
        if !platform_is_linux() {
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED)
            );
        } else if !probe::xvfb_ready() {
            assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_XVFB_MISSING));
        } else {
            assert!(
                err.unsupported_code() == Some(codes::SANDBOX_VIRTUAL_DISPLAY_STUB)
                    || err.unsupported_code() == Some(codes::SANDBOX_XVFB_MISSING)
            );
        }
    }

    #[test]
    fn vnc_request_fail_loud_before_spawn() {
        let mut cfg = VirtualDisplayConfig::default();
        cfg.vnc_port = Some(5900);
        let mgr = VirtualDisplayManager::new(cfg).unwrap();
        let err = mgr.start().unwrap_err();
        if platform_is_linux() {
            assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_VNC_UNSUPPORTED));
        } else {
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_VIRTUAL_DISPLAY_UNSUPPORTED)
            );
        }
    }
}
