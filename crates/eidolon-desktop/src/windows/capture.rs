//! Windows screenshot backend selection (DXGI preferred, GDI fallback).
//!
//! Hermetic preference parsing runs on all hosts. Live DXGI/GDI capture is
//! compiled only with `cfg(all(target_os = "windows", feature = "desktop-windows"))`.

use crate::codes;
use eidolon_core::error::PhenoError;
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
use eidolon_core::Result;
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
use std::path::Path;

/// Env override for Windows screenshot backend selection.
///
/// Values (case-insensitive): `auto` (default), `dxgi`, `gdi`.
pub const CAPTURE_MODE_ENV: &str = "EIDOLON_DESKTOP_WIN_CAPTURE";

/// Live Windows capture smoke gate (`EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1`).
///
/// When set, [`require_windows_capture_smoke`] runs a live DXGI/GDI path on
/// Windows + `desktop-windows`, and fails loud on every other host.
pub const CAPTURE_SMOKE_ENV: &str = "EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE";

/// Which capture backend to attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinCaptureMode {
    /// Prefer DXGI Desktop Duplication; fall back to GDI BitBlt on miss.
    Auto,
    /// DXGI only — miss returns [`codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE`].
    Dxgi,
    /// GDI BitBlt only.
    Gdi,
}

/// Which backend actually produced a frame (for logging / tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinCaptureBackend {
    Dxgi,
    Gdi,
}

/// Parse `EIDOLON_DESKTOP_WIN_CAPTURE` (or an injected string for tests).
pub fn parse_capture_mode(raw: Option<&str>) -> WinCaptureMode {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => WinCaptureMode::Auto,
        Some(s) if s.eq_ignore_ascii_case("auto") => WinCaptureMode::Auto,
        Some(s) if s.eq_ignore_ascii_case("dxgi") => WinCaptureMode::Dxgi,
        Some(s) if s.eq_ignore_ascii_case("gdi") => WinCaptureMode::Gdi,
        Some(_) => WinCaptureMode::Auto,
    }
}

/// Read capture mode from the process environment.
pub fn capture_mode_from_env() -> WinCaptureMode {
    parse_capture_mode(std::env::var(CAPTURE_MODE_ENV).ok().as_deref())
}

/// Decide ordered backends for a mode (pure; hermetic).
pub fn capture_plan(mode: WinCaptureMode) -> &'static [WinCaptureBackend] {
    match mode {
        WinCaptureMode::Auto => &[WinCaptureBackend::Dxgi, WinCaptureBackend::Gdi],
        WinCaptureMode::Dxgi => &[WinCaptureBackend::Dxgi],
        WinCaptureMode::Gdi => &[WinCaptureBackend::Gdi],
    }
}

/// `true` when live Windows capture smoke is explicitly requested.
pub fn capture_smoke_requested() -> bool {
    std::env::var(CAPTURE_SMOKE_ENV).ok().as_deref() == Some("1")
}

/// Fail-loud gate for the Windows capture smoke path.
///
/// - Smoke env unset → [`codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE`] with clear
///   "set SMOKE=1".
/// - Smoke env set off Windows / without `desktop-windows` → fail-loud
///   [`codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE`].
/// - Windows + feature → `Ok(())` (caller still needs
///   `EIDOLON_DESKTOP_ALLOW_ACTIONS=1` for `WindowsClient::screenshot`).
pub fn require_windows_capture_smoke() -> eidolon_core::Result<()> {
    if !capture_smoke_requested() {
        return Err(PhenoError::unsupported_platform(
            codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE,
            format!(
                "Windows capture smoke gated — set {CAPTURE_SMOKE_ENV}=1 \
                 (and EIDOLON_DESKTOP_ALLOW_ACTIONS=1 for live screenshot). \
                 Off-Windows hosts always fail the host gate; see \
                 docs/guides/windows-desktop-capture.md"
            ),
        ));
    }
    require_windows_capture_host()
}

/// Assert the current target can run live DXGI/GDI capture (no smoke env check).
pub fn require_windows_capture_host() -> eidolon_core::Result<()> {
    #[cfg(all(target_os = "windows", feature = "desktop-windows"))]
    {
        Ok(())
    }
    #[cfg(not(all(target_os = "windows", feature = "desktop-windows")))]
    {
        Err(PhenoError::unsupported_platform(
            codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE,
            format!(
                "Windows DXGI/GDI capture smoke requires target_os=windows + \
                 feature `desktop-windows` (alias `desktop-windows-dxgi`); \
                 this host cannot run live Desktop Duplication. Override mode \
                 with {CAPTURE_MODE_ENV}=auto|dxgi|gdi on Windows only. \
                 See docs/guides/windows-desktop-capture.md"
            ),
        ))
    }
}

/// Fail-loud when no capture backend could produce a BMP.
#[cfg_attr(
    not(all(target_os = "windows", feature = "desktop-windows")),
    allow(dead_code)
)]
pub(crate) fn capture_unavailable(detail: impl Into<String>) -> PhenoError {
    let detail = detail.into();
    PhenoError::unsupported_platform(
        codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE,
        format!(
            "Windows screenshot capture unavailable — {detail}. \
             Prefer DXGI Desktop Duplication (`desktop-windows` / \
             `desktop-windows-dxgi`); GDI BitBlt is the fallback. \
             Override with {CAPTURE_MODE_ENV}=dxgi|gdi|auto."
        ),
    )
}

/// Capture primary display → BMP using the configured preference plan.
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
pub(crate) fn capture_primary_bmp(path: &Path) -> Result<WinCaptureBackend> {
    ensure_parent(path)?;
    let mode = capture_mode_from_env();
    let plan = capture_plan(mode);
    let mut last_dxgi: Option<String> = None;

    for backend in plan {
        match backend {
            WinCaptureBackend::Dxgi => match super::dxgi::capture_bmp(path) {
                Ok(()) => {
                    log::info!(
                        "Screenshot (DXGI Desktop Duplication BMP) saved to {}",
                        path.display()
                    );
                    return Ok(WinCaptureBackend::Dxgi);
                }
                Err(e) => {
                    let msg = e.to_string();
                    log::warn!(
                        "DXGI Desktop Duplication capture missed ({msg}); \
                         code-context={}",
                        codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE
                    );
                    last_dxgi = Some(msg);
                    if matches!(mode, WinCaptureMode::Dxgi) {
                        return Err(capture_unavailable(format!(
                            "DXGI-only mode failed: {}",
                            last_dxgi.as_deref().unwrap_or("unknown")
                        )));
                    }
                }
            },
            WinCaptureBackend::Gdi => match super::gdi::capture_bmp(path) {
                Ok(()) => {
                    if last_dxgi.is_some() {
                        log::info!(
                            "Screenshot (GDI BMP fallback after DXGI miss) saved to {}",
                            path.display()
                        );
                    } else {
                        log::info!("Screenshot (GDI BMP) saved to {}", path.display());
                    }
                    return Ok(WinCaptureBackend::Gdi);
                }
                Err(e) => {
                    return Err(capture_unavailable(format!(
                        "GDI BitBlt failed after plan {:?}: {e} (prior DXGI: {})",
                        plan,
                        last_dxgi.as_deref().unwrap_or("n/a")
                    )));
                }
            },
        }
    }

    Err(capture_unavailable(format!(
        "empty capture plan for mode {mode:?} (DXGI last: {})",
        last_dxgi.as_deref().unwrap_or("n/a")
    )))
}

#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(PhenoError::Platform(format!(
                "screenshot parent directory does not exist: {}",
                parent.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_to_auto() {
        assert_eq!(parse_capture_mode(None), WinCaptureMode::Auto);
        assert_eq!(parse_capture_mode(Some("")), WinCaptureMode::Auto);
        assert_eq!(parse_capture_mode(Some("  ")), WinCaptureMode::Auto);
        assert_eq!(parse_capture_mode(Some("nope")), WinCaptureMode::Auto);
    }

    #[test]
    fn parse_known_values() {
        assert_eq!(parse_capture_mode(Some("auto")), WinCaptureMode::Auto);
        assert_eq!(parse_capture_mode(Some("DXGI")), WinCaptureMode::Dxgi);
        assert_eq!(parse_capture_mode(Some("gdi")), WinCaptureMode::Gdi);
    }

    #[test]
    fn auto_plan_prefers_dxgi_then_gdi() {
        assert_eq!(
            capture_plan(WinCaptureMode::Auto),
            &[WinCaptureBackend::Dxgi, WinCaptureBackend::Gdi]
        );
        assert_eq!(
            capture_plan(WinCaptureMode::Dxgi),
            &[WinCaptureBackend::Dxgi]
        );
        assert_eq!(capture_plan(WinCaptureMode::Gdi), &[WinCaptureBackend::Gdi]);
    }

    #[test]
    fn capture_unavailable_uses_stable_code() {
        let err = capture_unavailable("unit-test");
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE)
        );
        assert_eq!(err.status_code(), 501);
    }

    #[test]
    fn capture_mode_env_name_stable() {
        assert_eq!(CAPTURE_MODE_ENV, "EIDOLON_DESKTOP_WIN_CAPTURE");
        assert_eq!(CAPTURE_SMOKE_ENV, "EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE");
    }

    #[test]
    fn capture_host_gate_matches_target() {
        let result = require_windows_capture_host();
        #[cfg(all(target_os = "windows", feature = "desktop-windows"))]
        {
            assert!(result.is_ok());
        }
        #[cfg(not(all(target_os = "windows", feature = "desktop-windows")))]
        {
            let err = result.unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE)
            );
            assert_eq!(err.status_code(), 501);
        }
    }

    #[test]
    fn capture_smoke_gate_requires_env() {
        // Do not mutate global SMOKE env (parallel tests). When unset, gate fails.
        if capture_smoke_requested() {
            return;
        }
        let err = require_windows_capture_smoke().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE)
        );
        assert!(err.to_string().contains(CAPTURE_SMOKE_ENV));
    }

    /// Invoked by scaffolding subprocess test `windows_capture_smoke_env_fails_loud_off_windows`
    /// with `EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1`. Ignored by default so plain
    /// `cargo test` runs skip it; the subprocess harness passes `--include-ignored`.
    #[test]
    #[ignore = "subprocess harness only (requires EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1)"]
    fn capture_smoke_gate_fails_loud_off_windows_with_env() {
        if !capture_smoke_requested() {
            panic!(
                "run with {CAPTURE_SMOKE_ENV}=1 (subprocess harness only)"
            );
        }
        #[cfg(not(all(target_os = "windows", feature = "desktop-windows")))]
        {
            let err = require_windows_capture_smoke().unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE)
            );
            assert_eq!(err.status_code(), 501);
            assert!(err.to_string().contains("target_os=windows"));
        }
        #[cfg(all(target_os = "windows", feature = "desktop-windows"))]
        {
            assert!(require_windows_capture_smoke().is_ok());
        }
    }
}
