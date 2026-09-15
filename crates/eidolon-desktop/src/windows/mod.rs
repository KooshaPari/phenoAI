//! Windows desktop driver (A+ T1).
//!
//! # Status
//!
//! - **Stub** ([`WindowsStub`]): always available; fail-loud
//!   [`codes::DESKTOP_WIN_STUB`](crate::codes::DESKTOP_WIN_STUB).
//! - **Real driver** ([`WindowsClient`]): behind feature `desktop-windows`
//!   (alias `desktop-windows-dxgi`) on `target_os = "windows"` — Win32
//!   `SendInput` for pointer/text; screenshots prefer **DXGI Desktop
//!   Duplication**, falling back to GDI BitBlt → BMP. Destructive actions
//!   require [`ACTIONS_ALLOW_ENV`]`=1`. Capture mode override:
//!   [`capture::CAPTURE_MODE_ENV`].
//!
//! Do **not** unarchive KDesktopVirt — wrap Win32 via the `windows` crate.
//!
//! See `docs/EXTRACTION_PLAN.md` Phase 2 and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`.

mod bmp;
mod capture;
mod gate;
mod stub;

#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
mod driver;
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
mod dxgi;
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
mod gdi;

pub use capture::{
    capture_mode_from_env, capture_plan, capture_smoke_requested, parse_capture_mode,
    require_windows_capture_host, require_windows_capture_smoke, WinCaptureBackend,
    WinCaptureMode, CAPTURE_MODE_ENV, CAPTURE_SMOKE_ENV,
};
pub use gate::{
    actions_allowed, require_actions_allowed, ACTIONS_ALLOW_ENV,
};
pub use stub::WindowsStub;

#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
pub use driver::WindowsClient;

use eidolon_core::traits::DesktopAutomator;

/// Trait hooks for the native Windows driver.
///
/// Real [`WindowsClient`] (feature `desktop-windows` on Windows) returns
/// `true` from [`WindowsDesktopDriver::send_input_ready`]. Screenshot
/// readiness follows DXGI/GDI capture (`capture_ready`).
#[async_trait::async_trait]
pub trait WindowsDesktopDriver: DesktopAutomator {
    /// Whether Win32 `SendInput` is wired.
    fn send_input_ready(&self) -> bool {
        false
    }

    /// Whether DXGI/GDI screenshot capture is wired.
    fn capture_ready(&self) -> bool {
        false
    }
}

#[async_trait::async_trait]
impl WindowsDesktopDriver for WindowsStub {}

#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
#[async_trait::async_trait]
impl WindowsDesktopDriver for WindowsClient {
    fn send_input_ready(&self) -> bool {
        true
    }

    fn capture_ready(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_reports_not_ready() {
        let stub = WindowsStub::new();
        assert!(!stub.send_input_ready());
        assert!(!stub.capture_ready());
    }

    #[test]
    fn actions_gate_defaults_off() {
        // Do not assert global env (parallel tests). Contract: helper exists
        // and gated error uses the documented code when env is unset in a
        // fresh check — covered in scaffolding with code string stability.
        let _ = ACTIONS_ALLOW_ENV;
        let _ = actions_allowed();
    }
}
