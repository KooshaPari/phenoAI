//! Linux desktop driver (A+ T1).
//!
//! # Status
//!
//! - **Stub** ([`LinuxStub`]): always available; fail-loud
//!   [`codes::DESKTOP_LINUX_STUB`](crate::codes::DESKTOP_LINUX_STUB).
//! - **Real driver** ([`LinuxClient`]): behind feature `desktop-linux` on
//!   `target_os = "linux"` — X11 via `x11rb` (XTEST + `GetImage` → BMP) when
//!   `DISPLAY` is set; pure Wayland via xdg-desktop-portal Screenshot +
//!   RemoteDesktop/Screencast inject (`ashpd`) when only `WAYLAND_DISPLAY` is
//!   set. Destructive actions require [`crate::ACTIONS_ALLOW_ENV`]`=1`.
//! - **AT-SPI** ([`atspi`]): sibling a11y helpers behind `desktop-linux-atspi`
//!   — list/find/activate; not full Appium Desktop.
//!
//! # Session types
//!
//! | Session | Support |
//! |---|---|
//! | Native X11 (`DISPLAY` set) | ✅ pointer / text / screenshot |
//! | XWayland (`DISPLAY` + `WAYLAND_DISPLAY`) | ✅ via X11 path |
//! | Pure Wayland (`WAYLAND_DISPLAY` only) | ✅ portal Screenshot + viewport; ✅ RemoteDesktop+Screencast absolute pointer / keysym text; opt-in restore-token (`EIDOLON_DESKTOP_WAYLAND_RESTORE=1`); portal missing/deny → [`codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`](crate::codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE); device/stream gap → [`codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`](crate::codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED); restore store I/O → [`codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO`](crate::codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO) |
//!
//! Do **not** unarchive KDesktopVirt — wrap X11 via `x11rb`, Wayland via `ashpd`.
//!
//! See `docs/EXTRACTION_PLAN.md` Phase 2 and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`.

pub mod atspi;

mod stub;

#[cfg(feature = "desktop-linux")]
pub mod wayland_restore_token;

/// Hermetic Wayland inject/restore smoke gates (all targets).
pub mod wayland_smoke;

#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
mod driver;
#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
mod wayland;
#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
mod wayland_input;

pub use stub::LinuxStub;

#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
pub use driver::LinuxClient;

use eidolon_core::traits::DesktopAutomator;

/// Trait hooks for the native Linux driver.
///
/// Real [`LinuxClient`] (feature `desktop-linux` on Linux) returns `true`
/// from both [`LinuxDesktopDriver::x11_ready`] and
/// [`LinuxDesktopDriver::wayland_ready`] (X11 + portal Screenshot +
/// RemoteDesktop/Screencast inject). Portal miss →
/// [`crate::codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`]; device/stream
/// gap → [`crate::codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`]. AT-SPI
/// readiness lives on [`atspi::LinuxAtspiAutomator`] (additive sibling).
#[async_trait::async_trait]
pub trait LinuxDesktopDriver: DesktopAutomator {
    /// Whether an X11 (or XWayland) backend is wired.
    fn x11_ready(&self) -> bool {
        false
    }

    /// Whether a pure Wayland backend is wired (portal Screenshot + input).
    fn wayland_ready(&self) -> bool {
        false
    }

    /// Whether an AT-SPI a11y client is wired on this driver.
    ///
    /// Default `false` — use [`atspi::AtspiClient`] /
    /// [`atspi::LinuxAtspiAutomator`] for the real path.
    fn atspi_ready(&self) -> bool {
        false
    }
}

#[async_trait::async_trait]
impl LinuxDesktopDriver for LinuxStub {}

#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
#[async_trait::async_trait]
impl LinuxDesktopDriver for LinuxClient {
    fn x11_ready(&self) -> bool {
        true
    }

    fn wayland_ready(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-001
    #[test]
    fn stub_reports_not_ready() {
        let stub = LinuxStub::new();
        assert!(!stub.x11_ready());
        assert!(!stub.wayland_ready());
        assert!(!stub.atspi_ready());
    }
}
