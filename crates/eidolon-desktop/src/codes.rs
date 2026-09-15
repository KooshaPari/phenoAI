//! Documented machine-readable error codes for `eidolon-desktop`.
//!
//! These codes appear in [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError)
//! and are stable for callers / agents to match on. Prefer matching on the code
//! string rather than parsing the human message.

/// Windows desktop driver path is a fail-loud stub (feature off / non-Windows).
///
/// Real path: feature `desktop-windows` / `desktop-windows-dxgi` on
/// `target_os = "windows"` → [`crate::windows::WindowsClient`] (`SendInput` +
/// DXGI Desktop Duplication preferred, GDI BMP fallback).
pub const DESKTOP_WIN_STUB: &str = "EIDOLON_DESKTOP_WIN_STUB";

/// Linux desktop driver path is a fail-loud stub (feature off / non-Linux).
///
/// Real path: feature `desktop-linux` on `target_os = "linux"` →
/// [`crate::linux::LinuxClient`] (X11 XTEST + GetImage BMP when `DISPLAY` is
/// set; pure Wayland portal Screenshot + RemoteDesktop/Screencast inject via
/// ashpd when only `WAYLAND_DISPLAY` is set). Portal miss →
/// [`DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`]; device/stream gap after a
/// started RemoteDesktop session → [`DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`].
pub const DESKTOP_LINUX_STUB: &str = "EIDOLON_DESKTOP_LINUX_STUB";

/// Legacy / internal: X11 helper invoked on a pure Wayland session.
///
/// Callers on pure Wayland should use the portal Screenshot / RemoteDesktop
/// paths (wired). Prefer [`DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`] when the
/// portal is missing, and [`DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`] when a
/// session starts without usable Pointer/Keyboard/Screencast stream.
pub const DESKTOP_LINUX_WAYLAND_UNSUPPORTED: &str =
    "EIDOLON_DESKTOP_LINUX_WAYLAND_UNSUPPORTED";

/// Pure Wayland: xdg-desktop-portal Screenshot / RemoteDesktop / Screencast
/// unavailable.
///
/// Returned when `WAYLAND_DISPLAY` is set, `DISPLAY` is unset, and a portal
/// request fails (no session bus, no `xdg-desktop-portal`, user cancelled,
/// etc.).
pub const DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE: &str =
    "EIDOLON_DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE";

/// Pure Wayland: RemoteDesktop session started but inject surface unusable.
///
/// Screenshot/viewport use the Screenshot portal; pointer/text use
/// RemoteDesktop (+ Screencast for absolute motion). This code means the
/// session started without Pointer/Keyboard grant or without a Screencast
/// PipeWire stream for absolute motion — not “input never wired”.
pub const DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED: &str =
    "EIDOLON_DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED";

/// Pure Wayland: restore-token store I/O failed while
/// `EIDOLON_DESKTOP_WAYLAND_RESTORE=1`.
///
/// Returned when the configured token path cannot be read, written, or
/// created (permissions, missing `HOME` / `XDG_STATE_HOME`, etc.). Does not
/// mean the portal is missing — see [`DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`].
pub const DESKTOP_LINUX_WAYLAND_RESTORE_IO: &str =
    "EIDOLON_DESKTOP_LINUX_WAYLAND_RESTORE_IO";

/// Feature `desktop-linux-atspi` off or non-Linux target.
///
/// Real path: feature `desktop-linux-atspi` on `target_os = "linux"` →
/// [`crate::linux::atspi::AtspiClient`] (a11y tree list/find/activate).
pub const DESKTOP_LINUX_ATSPI_STUB: &str = "EIDOLON_DESKTOP_LINUX_ATSPI_STUB";

/// Feature on but session bus / `org.a11y.atspi` registry unavailable.
pub const DESKTOP_LINUX_ATSPI_UNAVAILABLE: &str =
    "EIDOLON_DESKTOP_LINUX_ATSPI_UNAVAILABLE";

/// Non-macOS / non-Win / non-Linux target (e.g. FreeBSD) — no driver planned yet.
pub const DESKTOP_OTHER_STUB: &str = "EIDOLON_DESKTOP_OTHER_STUB";

/// Destructive desktop action blocked until `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
///
/// Viewport reads stay ungated. Pointer / text / live screenshot require the
/// explicit env gate (mobile-style; see [`crate::windows::ACTIONS_ALLOW_ENV`]).
pub const DESKTOP_ACTIONS_GATED: &str = "EIDOLON_DESKTOP_ACTIONS_GATED";

/// Windows screenshot capture unavailable (DXGI miss and/or GDI fallback miss).
///
/// With `desktop-windows` on Windows, screenshots prefer DXGI Desktop
/// Duplication and fall back to GDI BitBlt → BMP (`EIDOLON_DESKTOP_WIN_CAPTURE`).
/// This code is returned when the selected plan cannot produce a frame (e.g.
/// `dxgi`-only mode after `DuplicateOutput`/`AcquireNextFrame` failure, or
/// both DXGI and GDI failing). DXGI miss alone under `auto` logs this code
/// context then tries GDI.
pub const DESKTOP_WIN_CAPTURE_UNAVAILABLE: &str = "EIDOLON_DESKTOP_WIN_CAPTURE_UNAVAILABLE";

/// FFmpeg / recording pipeline not available (feature off, or system `ffmpeg` missing).
///
/// Phase E surface lives in `eidolon-desktop::recording` behind `desktop-recording`.
/// Callers should enable the feature and install system `ffmpeg` (or set
/// `EIDOLON_FFMPEG`). Do not unarchive KDesktopVirt for routine work.
pub const DESKTOP_RECORDING_UNAVAILABLE: &str = "EIDOLON_DESKTOP_RECORDING_UNAVAILABLE";

/// Desktop security surface unavailable (unconfigured stub, missing
/// `desktop-security-vault` / `desktop-security-oauth` feature, or capability
/// still refused by policy).
///
/// Prefer [`eidolon_core::security`] for sandbox id/exec/`SandboxPolicy`, and
/// [`crate::PolicySecurityGate`] for desktop capability allow-lists. Optional
/// vault/OAuth: enable `desktop-security-vault` / `desktop-security-oauth`.
pub const DESKTOP_SECURITY_UNAVAILABLE: &str = "EIDOLON_DESKTOP_SECURITY_UNAVAILABLE";
