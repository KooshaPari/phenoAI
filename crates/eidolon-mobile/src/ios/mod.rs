//! iOS XCTest driver hooks + system `xcrun` / `xcodebuild` probes.
//!
//! # Status
//!
//! - Always-on: [`IosXcTestDriver`] trait, fail-loud [`IosStub`], [`probe`]
//!   helpers, and [`XcuiBridge`] (fail-loud [`MissingXcuiBridge`] until a
//!   helper is discovered / configured, or AppleScript is allowed).
//! - Always-on: [`xcui_helper`] library (argv contract + best-effort macOS
//!   execute + Xcode project discovery). Binary `eidolon-xcui-helper` behind
//!   feature `mobile-xcui-helper`. In-tree XCUITest host under
//!   `native/ios/EidolonXcuiHelper/` (preferred when built).
//! - Feature `mobile-ios`: [`XcrunIosDriver`] — live when tools answer version
//!   probes; lists simulators via `simctl`; screenshot via `simctl io` when
//!   gated; tap/swipe/text/viewport via [`XcuiBridge`] (env helper, XCUITest
//!   runner, Rust fallback, or best-effort AppleScript). No kmobile unarchive.
//!
//! See `docs/EXTRACTION_PLAN.md` and `docs/guides/ios-xcui-helper.md`.
//! Do **not** unarchive kmobile routinely.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::traits::MobileAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};

pub mod xcui_bridge;
pub mod xcui_helper;
pub use xcui_bridge::{
    resolve_xcui_bridge, AppleScriptXcuiBridge, BundleXcuiBridge, MissingXcuiBridge, XcuiBridge,
};
pub use xcui_helper::{
    discover_bundled_helper, discover_preferred_helper, discover_xcode_helper, parse_argv,
    xcode_project_dir, xcui_xcode_integration_enabled, HelperCommand, HELPER_BIN_NAME,
    XCODE_HELPER_BIN_NAME, XCODE_PROJECT_DIR_ENV, XCUI_XCODE_INTEGRATION_ENV,
};

#[cfg(feature = "mobile-ios")]
mod xcrun_backend;
#[cfg(feature = "mobile-ios")]
pub use xcrun_backend::XcrunIosDriver;

/// Trait hooks for a native iOS XCTest / `xcrun` driver.
///
/// Prefer [`XcrunIosDriver`] when the `mobile-ios` feature is enabled and
/// [`probe::xcrun_cli_ready`] is true. Otherwise use [`IosStub`] (fail-loud).
#[async_trait::async_trait]
pub trait IosXcTestDriver: MobileAutomator {
    /// Whether an XCTest / XCUITest bridge is wired (xcodebuild reachable).
    fn xctest_ready(&self) -> bool {
        false
    }

    /// Whether `xcrun` / simulator control is available.
    fn xcrun_ready(&self) -> bool {
        false
    }
}

/// Probe helpers for system Xcode CLIs (no bundled binaries).
pub mod probe {
    use crate::cli::{resolve_override, which_bin};
    use std::path::PathBuf;
    use std::process::Command;

    /// Env override for `xcrun` (`EIDOLON_XCRUN`).
    pub const XCRUN_PATH_ENV: &str = "EIDOLON_XCRUN";
    /// Env override for `xcodebuild` (`EIDOLON_XCODEBUILD`).
    pub const XCODEBUILD_PATH_ENV: &str = "EIDOLON_XCODEBUILD";

    /// Resolve `xcrun` via `EIDOLON_XCRUN` or `PATH`.
    pub fn resolve_xcrun() -> Option<PathBuf> {
        resolve_override(XCRUN_PATH_ENV).or_else(|| which_bin("xcrun"))
    }

    /// Resolve `xcodebuild` via `EIDOLON_XCODEBUILD` or `PATH`.
    pub fn resolve_xcodebuild() -> Option<PathBuf> {
        resolve_override(XCODEBUILD_PATH_ENV).or_else(|| which_bin("xcodebuild"))
    }

    /// Resolve `simctl` via `xcrun --find simctl` or `PATH`.
    pub fn resolve_simctl() -> Option<PathBuf> {
        if let Some(xcrun) = resolve_xcrun() {
            let output = Command::new(&xcrun).args(["--find", "simctl"]).output().ok()?;
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let p = PathBuf::from(&path);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        which_bin("simctl")
    }

    /// `true` when [`resolve_xcrun`] finds a binary.
    pub fn xcrun_ready() -> bool {
        resolve_xcrun().is_some()
    }

    /// First line of `xcrun --version`, if available.
    pub fn xcrun_version_line() -> Option<String> {
        let bin = resolve_xcrun()?;
        let output = Command::new(&bin).arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    /// `xcrun --version` succeeds.
    pub fn xcrun_cli_ready() -> bool {
        xcrun_version_line().is_some()
    }

    /// First line of `xcodebuild -version`, if available.
    pub fn xcodebuild_version_line() -> Option<String> {
        let bin = resolve_xcodebuild()?;
        let output = Command::new(&bin).arg("-version").output().ok()?;
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

    /// `xcodebuild -version` succeeds (XCTest toolchain present).
    pub fn xcodebuild_cli_ready() -> bool {
        xcodebuild_version_line().is_some()
    }

    /// `simctl` is resolvable (via xcrun or PATH).
    pub fn simctl_ready() -> bool {
        resolve_simctl().is_some()
    }
}

/// Fail-loud iOS XCTest driver stub — default when `mobile-ios` is off or
/// no Xcode CLI is available.
#[derive(Debug, Default, Clone)]
pub struct IosStub;

impl IosStub {
    pub fn new() -> Self {
        Self
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::MOBILE_IOS_STUB,
            format!(
                "IosXcTestDriver::{method} not implemented — enable feature \
                 `mobile-ios` and install Xcode CLIs (`xcrun` / `xcodebuild`; \
                 EIDOLON_XCRUN / EIDOLON_XCODEBUILD overrides), or use \
                 XcrunIosDriver (docs/EXTRACTION_PLAN.md; do not unarchive \
                 kmobile routinely)"
            ),
        )
    }
}

#[async_trait::async_trait]
impl MobileAutomator for IosStub {
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
        log::debug!("Recorded event (ios stub): {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl IosXcTestDriver for IosStub {
    fn xctest_ready(&self) -> bool {
        // Stub never claims readiness even if tools are on PATH — callers must
        // use XcrunIosDriver behind `mobile-ios`.
        false
    }

    fn xcrun_ready(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_fail_loud() {
        let stub = IosStub::new();
        assert!(!stub.xctest_ready());
        assert!(!stub.xcrun_ready());
        let err = stub.tap(0, 0).await.unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_IOS_STUB));
        assert_eq!(err.status_code(), 501);
    }

    #[test]
    fn probe_xcrun_consistency() {
        let ready = probe::xcrun_ready();
        assert_eq!(ready, probe::resolve_xcrun().is_some());
        if probe::xcrun_cli_ready() {
            assert!(ready);
            assert!(probe::xcrun_version_line().is_some());
        }
    }
}
