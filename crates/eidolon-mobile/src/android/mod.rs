//! Android UiAutomator driver hooks + system `adb` probes.
//!
//! # Status
//!
//! - Always-on: [`AndroidUiAutomatorDriver`] trait, fail-loud [`AndroidStub`],
//!   and [`probe`] helpers that locate `adb` without spawning UiAutomator.
//! - Always-on: [`uia2_server`] hermetic Appium-shaped argv / package parsers
//!   + [`Uia2Server`] lifecycle; APKs via env override → checkout assets →
//!   durable SHA-256-verified cache (`uia2_assets` / `eidolon-fetch-uia2`).
//! - Feature `mobile-android`: [`AdbAndroidDriver`] — live when `adb version`
//!   answers; lists devices via `adb devices`; **real** gated `adb shell input`
//!   tap/swipe/text + screencap (`EIDOLON_MOBILE_ALLOW_ACTIONS=1`).
//! - Feature `mobile-uia2` (implies `mobile-android` + `ureq`):
//!   [`AdbAndroidDriver::uia2`] / [`AdbAndroidDriver::uia2_http`] — lifecycle
//!   plus Appium-compatible HTTP session client (findElement(s), click, windows,
//!   contexts, actions, screenshot, timeouts on `:6790`);
//!   `Uia2ApkPaths::ensure` fetches pinned Appium release APKs on cache miss.
//! - Feature `mobile-appium` (implies `mobile-uia2`): optional Appium **server**
//!   discovery (`APPIUM_HOME` / `appium` CLI / `EIDOLON_APPIUM_URL`) — hermetic
//!   when absent.
//! - Feature `mobile-appium-desktop` (implies `mobile-appium`): Appium Inspector
//!   launcher + local HTML dashboard (`eidolon-appium-desktop`).
//!
//! See `docs/EXTRACTION_PLAN.md`. Do **not** unarchive kmobile routinely.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::traits::MobileAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};

#[cfg(feature = "mobile-android")]
mod adb_backend;
#[cfg(feature = "mobile-android")]
pub use adb_backend::AdbAndroidDriver;

pub mod uia2_assets;
pub mod uia2_server;
pub use uia2_server::{
    parse_package_status, Uia2ApkPaths, Uia2PackageStatus, Uia2Server, DEFAULT_DEVICE_PORT,
    DEFAULT_RUNNER, DEFAULT_SERVER_PACKAGE, DEFAULT_TEST_PACKAGE,
};

#[cfg(feature = "mobile-uia2")]
pub mod uia2_http;
#[cfg(feature = "mobile-uia2")]
mod session_wire;
#[cfg(feature = "mobile-uia2")]
mod uia2_session_ext;
#[cfg(feature = "mobile-uia2")]
pub use uia2_http::{
    AppiumSessionClient, SessionWireMode, Uia2Element, Uia2HttpClient, DEFAULT_HTTP_HOST,
};

#[cfg(feature = "mobile-appium")]
pub mod appium_probe;
#[cfg(feature = "mobile-appium")]
pub use appium_probe::{
    appium_cli_ready, appium_tools_ready, env_appium_url, require_appium_ready,
    resolve_appium_cli, resolve_appium_home, AppiumProbe, APPIUM_HOME_ENV, APPIUM_PATH_ENV,
    APPIUM_URL_ENV, DEFAULT_APPIUM_URL,
};

#[cfg(feature = "mobile-appium-desktop")]
pub mod appium_desktop;
#[cfg(feature = "mobile-appium-desktop")]
pub use appium_desktop::{
    launch_inspector, resolve_inspector, resolve_server_url, run as run_appium_desktop,
    serve_dashboard, APPIUM_DESKTOP_BIND_ENV, APPIUM_INSPECTOR_ENV,
};

/// Trait hooks for a native Android UiAutomator / `adb` driver.
///
/// Prefer [`AdbAndroidDriver`] when the `mobile-android` feature is enabled and
/// [`probe::adb_cli_ready`] is true. Otherwise use [`AndroidStub`] (fail-loud).
#[async_trait::async_trait]
pub trait AndroidUiAutomatorDriver: MobileAutomator {
    /// Whether a UiAutomator / UiAutomator2 bridge is wired.
    ///
    /// Shell `uiautomator` (one-shot) ≠ Appium UIA2 instrumentation server.
    /// See [`uia2_server_ready`].
    fn uiautomator_ready(&self) -> bool {
        false
    }

    /// Whether `adb` device control is available.
    fn adb_ready(&self) -> bool {
        false
    }

    /// Whether a UiAutomator2 instrumentation server path is ready.
    ///
    /// Default `false`. Live drivers report based on package install / process
    /// probe — never invent readiness without evidence.
    fn uia2_server_ready(&self) -> bool {
        false
    }
}

/// Probe helpers for system Android platform-tools (no bundled binaries).
pub mod probe {
    use crate::cli::{resolve_override, which_bin};
    use std::path::PathBuf;
    use std::process::Command;

    /// Env override for `adb` (`EIDOLON_ADB`).
    pub const ADB_PATH_ENV: &str = "EIDOLON_ADB";

    /// Resolve `adb` via `EIDOLON_ADB` or `PATH`.
    pub fn resolve_adb() -> Option<PathBuf> {
        resolve_override(ADB_PATH_ENV).or_else(|| which_bin("adb"))
    }

    /// `true` when [`resolve_adb`] finds a binary.
    pub fn adb_ready() -> bool {
        resolve_adb().is_some()
    }

    /// First line of `adb version`, if available.
    pub fn adb_version_line() -> Option<String> {
        let bin = resolve_adb()?;
        let output = Command::new(&bin).arg("version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    /// `adb version` succeeds.
    pub fn adb_cli_ready() -> bool {
        adb_version_line().is_some()
    }
}

/// Fail-loud Android UiAutomator driver stub — default when `mobile-android`
/// is off or no `adb` is available.
#[derive(Debug, Default, Clone)]
pub struct AndroidStub;

impl AndroidStub {
    pub fn new() -> Self {
        Self
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::MOBILE_ANDROID_STUB,
            format!(
                "AndroidUiAutomatorDriver::{method} not implemented — enable \
                 feature `mobile-android` and install platform-tools `adb` \
                 (EIDOLON_ADB override), or use AdbAndroidDriver \
                 (docs/EXTRACTION_PLAN.md; do not unarchive kmobile routinely)"
            ),
        )
    }
}

#[async_trait::async_trait]
impl MobileAutomator for AndroidStub {
    async fn get_viewport(&self) -> Result<Viewport> {
        Err(Self::unsupported("get_viewport"))
    }

    async fn screenshot(&self, _path: &str) -> Result<()> {
        Err(Self::unsupported("screenshot"))
    }

    async fn tap(&self, _x: i32, _y: i32) -> Result<()> {
        Err(Self::unsupported("tap"))
    }

    async fn swipe(&self, _x1: i32, _y1: i32, _x2: i32, _y2: i32) -> Result<()> {
        Err(Self::unsupported("swipe"))
    }

    async fn input_text(&self, _text: &str) -> Result<()> {
        Err(Self::unsupported("input_text"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event (android stub): {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl AndroidUiAutomatorDriver for AndroidStub {
    fn uiautomator_ready(&self) -> bool {
        false
    }

    fn adb_ready(&self) -> bool {
        false
    }

    fn uia2_server_ready(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_fail_loud() {
        let stub = AndroidStub::new();
        assert!(!stub.uiautomator_ready());
        assert!(!stub.adb_ready());
        let err = stub.tap(0, 0).await.unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_ANDROID_STUB));
        assert_eq!(err.status_code(), 501);
    }

    #[test]
    fn probe_adb_consistency() {
        let ready = probe::adb_ready();
        assert_eq!(ready, probe::resolve_adb().is_some());
        if probe::adb_cli_ready() {
            assert!(ready);
            assert!(probe::adb_version_line().is_some());
        }
    }
}
