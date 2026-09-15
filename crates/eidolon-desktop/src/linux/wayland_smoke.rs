//! Pure Wayland inject/restore live smoke gates (hermetic on all hosts).
//!
//! Live pointer/text/restore exercises require Linux + `desktop-linux` +
//! `WAYLAND_DISPLAY` + `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
//! Optional restore persistence: [`crate::WAYLAND_RESTORE_ENV`]`=1`.
//!
//! Portal miss/deny → [`crate::codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`].
//! Device/stream gap → [`crate::codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`].
//! Restore store I/O → [`crate::codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO`].

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Live Wayland inject/restore smoke gate (`EIDOLON_DESKTOP_WAYLAND_SMOKE=1`).
///
/// When set, [`require_wayland_smoke`] runs live portal inject/restore paths on
/// Linux + `desktop-linux` + `WAYLAND_DISPLAY`, and fails loud on every other host.
pub const WAYLAND_SMOKE_ENV: &str = "EIDOLON_DESKTOP_WAYLAND_SMOKE";

/// `true` when live Wayland inject smoke is explicitly requested.
pub fn wayland_smoke_requested() -> bool {
    std::env::var(WAYLAND_SMOKE_ENV).ok().as_deref() == Some("1")
}

/// `true` when a Wayland compositor socket is visible in the environment.
pub fn wayland_session_present() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Fail-loud gate for the Wayland inject smoke path.
///
/// - Smoke env unset → [`codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`] with
///   clear "set SMOKE=1".
/// - Smoke env set off Linux / without `desktop-linux` / without `WAYLAND_DISPLAY`
///   → fail-loud [`codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`].
/// - Linux + feature + session → `Ok(())` (caller still needs
///   `EIDOLON_DESKTOP_ALLOW_ACTIONS=1` for pointer/text inject).
pub fn require_wayland_smoke() -> Result<()> {
    if !wayland_smoke_requested() {
        return Err(PhenoError::unsupported_platform(
            codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
            format!(
                "Wayland inject smoke gated — set {WAYLAND_SMOKE_ENV}=1 \
                 (and EIDOLON_DESKTOP_ALLOW_ACTIONS=1 for live pointer/text). \
                 Also need WAYLAND_DISPLAY on Linux + feature `desktop-linux`. \
                 Off-Linux hosts always fail the host gate; see \
                 docs/guides/linux-live-smokes.md"
            ),
        ));
    }
    require_wayland_smoke_host()
}

/// Assert the current target can run live Wayland portal inject (no smoke env check).
pub fn require_wayland_smoke_host() -> Result<()> {
    #[cfg(all(target_os = "linux", feature = "desktop-linux"))]
    {
        if !wayland_session_present() {
            return Err(PhenoError::unsupported_platform(
                codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
                format!(
                    "Wayland inject smoke requires WAYLAND_DISPLAY on Linux + \
                     feature `desktop-linux`; unset DISPLAY for pure-Wayland \
                     portal inject. See docs/guides/linux-live-smokes.md"
                ),
            ));
        }
        Ok(())
    }
    #[cfg(not(all(target_os = "linux", feature = "desktop-linux")))]
    {
        Err(PhenoError::unsupported_platform(
            codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
            format!(
                "Wayland portal inject smoke requires target_os=linux + \
                 feature `desktop-linux`; this host cannot run live RemoteDesktop \
                 inject. See docs/guides/linux-live-smokes.md"
            ),
        ))
    }
}

/// Acceptable fail-loud codes for live Wayland inject when portal/session is absent.
pub fn wayland_inject_miss_codes() -> &'static [&'static str] {
    &[
        codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
        codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED,
        codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_smoke_gate_requires_env() {
        if wayland_smoke_requested() {
            return;
        }
        let err = require_wayland_smoke().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE)
        );
    }

    #[test]
    fn wayland_host_gate_matches_target() {
        let result = require_wayland_smoke_host();
        #[cfg(all(target_os = "linux", feature = "desktop-linux"))]
        {
            if wayland_session_present() {
                assert!(result.is_ok());
            } else {
                let err = result.unwrap_err();
                assert_eq!(
                    err.unsupported_code(),
                    Some(codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE)
                );
            }
        }
        #[cfg(not(all(target_os = "linux", feature = "desktop-linux")))]
        {
            let err = result.unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE)
            );
            assert_eq!(err.status_code(), 501);
        }
    }
}
