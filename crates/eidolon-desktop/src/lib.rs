//! Eidolon Desktop — macOS, Windows, Linux automation.
//!
//! # Honesty (A+ T0 / T1 / T2 slice)
//!
//! - **macOS**: [`DesktopClient`] is a real Core Graphics / `screencapture`
//!   implementer (`macos::MacOSClient`).
//! - **Windows**: feature `desktop-windows` / `desktop-windows-dxgi` on
//!   `target_os = "windows"` → [`windows::WindowsClient`] (`SendInput` +
//!   DXGI Desktop Duplication preferred, GDI BMP fallback). Feature off /
//!   other targets → fail-loud stub with [`codes::DESKTOP_WIN_STUB`].
//!   Destructive actions need `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`. Capture miss
//!   → [`codes::DESKTOP_WIN_CAPTURE_UNAVAILABLE`] (`EIDOLON_DESKTOP_WIN_CAPTURE`
//!   selects `auto`/`dxgi`/`gdi`; live smoke via
//!   `EIDOLON_DESKTOP_WIN_CAPTURE_SMOKE=1` — fail-loud off-Windows).
//! - **Linux**: feature `desktop-linux` on `target_os = "linux"` →
//!   [`linux::LinuxClient`] (X11 XTEST + GetImage BMP when `DISPLAY` is set;
//!   pure Wayland xdg-desktop-portal Screenshot + RemoteDesktop/Screencast
//!   inject via ashpd when only `WAYLAND_DISPLAY` is set). Feature off /
//!   other targets → fail-loud stub with [`codes::DESKTOP_LINUX_STUB`].
//!   Portal miss/deny → [`codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`];
//!   started session without Pointer/Keyboard/stream →
//!   [`codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`]. Opt-in restore-token
//!   persistence (`EIDOLON_DESKTOP_WAYLAND_RESTORE=1`) skips re-prompt when a
//!   stored token is accepted; store I/O →
//!   [`codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO`]. Destructive actions
//!   need `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`. Live inject smoke via
//!   `EIDOLON_DESKTOP_WAYLAND_SMOKE=1` — fail-loud off-Linux / without
//!   `WAYLAND_DISPLAY` (hermetic gates in `linux::wayland_smoke`).
//! - **Linux AT-SPI**: feature `desktop-linux-atspi` on `target_os = "linux"` →
//!   [`linux::atspi::AtspiClient`] (a11y tree list / find-by-role / Action
//!   activate via the `atspi` crate). Feature off / other targets →
//!   [`linux::atspi::AtspiStub`] with [`codes::DESKTOP_LINUX_ATSPI_STUB`].
//!   Bus/registry miss → [`codes::DESKTOP_LINUX_ATSPI_UNAVAILABLE`]. Activate
//!   needs `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`. **Not** full Appium Desktop GUI
//!   or W3C WebDriver wire (mobile Appium path is separate).
//! - **Recording (Phase E)**: [`recording`] trait + probe + argv builders
//!   always on; [`recording::FfmpegRecorder`] / [`recording::RecordingPipeline`]
//!   behind feature `desktop-recording` (system `ffmpeg`, no blobs).
//!   [`RecordingStub`] remains fail-loud by default.
//! - **Security (Phase F)**: [`security_hooks`] — [`PolicySecurityGate`]
//!   allow-list + rate limit; core validators via trait defaults.
//!   Optional [`security_vault`] (`desktop-security-vault`) +
//!   [`security_oauth`] (`desktop-security-oauth`). Feature-off →
//!   [`codes::DESKTOP_SECURITY_UNAVAILABLE`]. [`SecurityHooksStub`] remains
//!   the unconfigured default. Win/Linux drivers also use the mobile-style
//!   env gate above.
//!
//! See `docs/EXTRACTION_PLAN.md` and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`. Do **not** unarchive
//! KDesktopVirt for routine work.

pub mod codes;
pub mod linux;
pub mod recording;
pub mod security_hooks;
#[cfg(feature = "desktop-security-oauth")]
pub mod security_oauth;
#[cfg(feature = "desktop-security-vault")]
pub mod security_vault;
pub mod windows;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(any(
    target_os = "macos",
    all(target_os = "windows", feature = "desktop-windows"),
    all(target_os = "linux", feature = "desktop-linux")
)))]
mod stub;

#[cfg(target_os = "macos")]
pub use macos::MacOSClient as DesktopClient;

#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
pub use windows::WindowsClient as DesktopClient;

#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
pub use linux::LinuxClient as DesktopClient;

#[cfg(not(any(
    target_os = "macos",
    all(target_os = "windows", feature = "desktop-windows"),
    all(target_os = "linux", feature = "desktop-linux")
)))]
pub use stub::DesktopClient;

#[cfg(all(target_os = "linux", feature = "desktop-linux-atspi"))]
pub use linux::atspi::AtspiClient;
pub use linux::atspi::{AtspiNode, AtspiStub, LinuxAtspiAutomator};
#[cfg(feature = "desktop-linux")]
pub use linux::wayland_restore_token::{WAYLAND_RESTORE_ENV, WAYLAND_RESTORE_TOKEN_PATH_ENV};
pub use linux::wayland_smoke::{
    require_wayland_smoke, require_wayland_smoke_host, wayland_inject_miss_codes,
    wayland_session_present, wayland_smoke_requested, WAYLAND_SMOKE_ENV,
};
#[cfg(all(target_os = "linux", feature = "desktop-linux"))]
pub use linux::LinuxClient;
pub use linux::{LinuxDesktopDriver, LinuxStub};
pub use recording::{
    ffmpeg_capture_args, ffmpeg_encode_args, ffmpeg_gif_args, DesktopRecorder, QualityProfile,
    RecordingStub, VideoFormat,
};
#[cfg(feature = "desktop-recording")]
pub use recording::{FfmpegRecorder, RecordingPipeline, RecordingResult};
pub use security_hooks::{
    caps, validate_capability_name, DesktopSecurityGate, PolicySecurityGate, SecurityHooksStub,
    CAPABILITY_MAX_LEN,
};
#[cfg(feature = "desktop-security-oauth")]
pub use security_oauth::{
    OAuthPkceFlow, OAUTH_AUTHORIZE_URL_ENV, OAUTH_CLIENT_ID_ENV, OAUTH_REDIRECT_URI_ENV,
};
#[cfg(feature = "desktop-security-vault")]
pub use security_vault::{
    blob_fingerprint, default_vault_path, hash_password, open, open_from_path, seal, seal_to_path,
    verify_password, VAULT_PATH_ENV,
};
#[cfg(all(target_os = "windows", feature = "desktop-windows"))]
pub use windows::WindowsClient;
pub use windows::{
    actions_allowed, capture_mode_from_env, capture_plan, capture_smoke_requested,
    parse_capture_mode, require_actions_allowed, require_windows_capture_host,
    require_windows_capture_smoke, WinCaptureBackend, WinCaptureMode, WindowsDesktopDriver,
    WindowsStub, ACTIONS_ALLOW_ENV, CAPTURE_MODE_ENV, CAPTURE_SMOKE_ENV,
};
