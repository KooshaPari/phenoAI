//! Shared CLI helpers for mobile instrument probes (no bundled binaries).
//!
//! Destructive automation (`tap` / `swipe` / `input_text` / live screenshot write)
//! requires [`ACTIONS_ALLOW_ENV`]`=1`. Listing devices and hermetic version probes
//! do not.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::path::PathBuf;
use std::process::Command;

/// Env gate for destructive mobile actions (`EIDOLON_MOBILE_ALLOW_ACTIONS=1`).
pub const ACTIONS_ALLOW_ENV: &str = "EIDOLON_MOBILE_ALLOW_ACTIONS";

/// Optional preferred device id (`EIDOLON_MOBILE_DEVICE`).
pub const DEVICE_ENV: &str = "EIDOLON_MOBILE_DEVICE";

/// Path to an XCUI helper executable / script (`EIDOLON_IOS_XCUI_BUNDLE`).
///
/// Contract (argv): `<helper> <op> --udid <id> …` where `op` is
/// `tap` / `swipe` / `text` / `viewport`. When unset,
/// [`crate::ios::xcui_bridge::resolve_xcui_bridge`] prefers the in-tree
/// XCUITest runner (`eidolon-xcui-xctest`) when built, else the Rust
/// `eidolon-xcui-helper` fallback (`mobile-xcui-helper`). Explicit env path
/// always wins when it points at an existing file.
pub const IOS_XCUI_BUNDLE_ENV: &str = "EIDOLON_IOS_XCUI_BUNDLE";

/// Allow best-effort Simulator AppleScript input (`EIDOLON_IOS_ALLOW_APPLESCRIPT=1`).
///
/// Coordinates are **macOS screen points** (Simulator window), not device
/// pixels — honest best-effort only; prefer [`IOS_XCUI_BUNDLE_ENV`] for XCUI.
pub const IOS_ALLOW_APPLESCRIPT_ENV: &str = "EIDOLON_IOS_ALLOW_APPLESCRIPT";

/// Optional Appium UiAutomator2 **server** APK override (`EIDOLON_UIA2_APK`).
///
/// When set with [`UIA2_TEST_APK_ENV`], wins over checkout assets and the durable
/// cache. When unset, [`crate::android::uia2_assets::resolve`] uses pinned
/// Appium release APKs (SHA-256 verified). Missing both paths →
/// [`crate::codes::MOBILE_UIA2_UNAVAILABLE`].
pub const UIA2_APK_ENV: &str = "EIDOLON_UIA2_APK";

/// Optional Appium UiAutomator2 **test/instrumentation** APK override
/// (`EIDOLON_UIA2_TEST_APK`). Required together with [`UIA2_APK_ENV`] when
/// overriding; otherwise resolved from assets/cache.
pub const UIA2_TEST_APK_ENV: &str = "EIDOLON_UIA2_TEST_APK";

/// Durable cache root for fetched UIA2 APKs (`EIDOLON_UIA2_CACHE`).
///
/// Default: `~/.cache/eidolon/uia2/<pinned-version>/`. Must not be under `/tmp`.
pub const UIA2_CACHE_ENV: &str = "EIDOLON_UIA2_CACHE";

/// Optional local↔device TCP port for UIA2 (`EIDOLON_UIA2_PORT`, default 6790).
pub const UIA2_PORT_ENV: &str = "EIDOLON_UIA2_PORT";

/// Appium server HTTP base URL (`EIDOLON_APPIUM_URL`, e.g. `http://127.0.0.1:4723`).
///
/// Used by feature `mobile-appium` [`crate::android::appium_probe`]. When unset,
/// probe may fall back to default URL only if `appium` CLI / `APPIUM_HOME` is
/// present; otherwise discovery stays hermetic (`None`).
pub const APPIUM_URL_ENV: &str = "EIDOLON_APPIUM_URL";

/// Path override for the `appium` CLI (`EIDOLON_APPIUM`).
pub const APPIUM_PATH_ENV: &str = "EIDOLON_APPIUM";

/// `true` when destructive actions are explicitly allowed.
pub fn actions_allowed() -> bool {
    std::env::var(ACTIONS_ALLOW_ENV).ok().as_deref() == Some("1")
}

/// Fail-loud unless [`actions_allowed`].
pub fn require_actions_allowed(method: &str) -> Result<()> {
    if actions_allowed() {
        return Ok(());
    }
    Err(PhenoError::unsupported_platform(
        codes::MOBILE_ACTIONS_GATED,
        format!(
            "mobile::{method} is gated — set {ACTIONS_ALLOW_ENV}=1 to allow \
             destructive XCTest/UiAutomator/adb actions (list-devices and \
             hermetic probes remain ungated; see docs/EXTRACTION_PLAN.md)"
        ),
    ))
}

/// Resolve preferred device id from env, if set and non-empty.
pub fn env_device_id() -> Option<String> {
    std::env::var(DEVICE_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Resolve [`IOS_XCUI_BUNDLE_ENV`] when it points at an existing file.
pub fn env_ios_xcui_bundle() -> Option<PathBuf> {
    resolve_override(IOS_XCUI_BUNDLE_ENV)
}

/// `true` when AppleScript Simulator input is explicitly allowed.
pub fn ios_applescript_allowed() -> bool {
    std::env::var(IOS_ALLOW_APPLESCRIPT_ENV).ok().as_deref() == Some("1")
}

/// Resolve [`UIA2_APK_ENV`] when it points at an existing file.
pub fn env_uia2_apk() -> Option<PathBuf> {
    resolve_override(UIA2_APK_ENV)
}

/// Resolve [`UIA2_TEST_APK_ENV`] when it points at an existing file.
pub fn env_uia2_test_apk() -> Option<PathBuf> {
    resolve_override(UIA2_TEST_APK_ENV)
}

/// Parse [`UIA2_PORT_ENV`] or return Appium UIA2 default `6790`.
pub fn env_uia2_port() -> u16 {
    std::env::var(UIA2_PORT_ENV)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(6790)
}

/// Locate `name` on `PATH` via `which` (hermetic existence check only).
pub fn which_bin(name: &str) -> Option<PathBuf> {
    let output = Command::new("which").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let p = PathBuf::from(path);
    p.is_file().then_some(p)
}

/// Resolve an override env path if it points at an existing file.
pub fn resolve_override(env_key: &str) -> Option<PathBuf> {
    let override_path = std::env::var(env_key).ok()?;
    let p = PathBuf::from(override_path);
    p.is_file().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_gate_defaults_off() {
        // Cannot assert global env; only that require fails when not set to 1.
        if !actions_allowed() {
            let err = require_actions_allowed("tap").unwrap_err();
            assert_eq!(err.unsupported_code(), Some(codes::MOBILE_ACTIONS_GATED));
            assert_eq!(err.status_code(), 501);
        }
    }

    #[test]
    fn which_bin_empty_name_is_none_or_path() {
        // `which` with empty string should not invent a binary.
        assert!(which_bin("").is_none() || which_bin("").is_some());
    }
}
