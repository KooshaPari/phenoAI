//! Documented machine-readable error codes for `eidolon-mobile`.
//!
//! These codes appear in [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError)
//! and are stable for callers / agents to match on. Prefer matching on the code
//! string rather than parsing the human message.
//!
//! Do **not** unarchive kmobile / mobile-cli / mobile-mcp for routine work —
//! fresh CLI ports live behind `mobile-ios` / `mobile-android` (see
//! `docs/EXTRACTION_PLAN.md`).

/// iOS XCTest / `xcrun` path is unavailable or still a fail-loud stub.
///
/// Emitted when feature `mobile-ios` is off, `xcrun`/`xcodebuild`/`simctl` are
/// missing, or an iOS action is not yet hermetically wired. With `mobile-ios`
/// + reachable tools, prefer [`crate::XcrunIosDriver`] for list/probe;
/// destructive taps still need [`crate::cli::ACTIONS_ALLOW_ENV`].
pub const MOBILE_IOS_STUB: &str = "EIDOLON_MOBILE_IOS_STUB";

/// Android UiAutomator / `adb` path is unavailable or still a fail-loud stub.
///
/// Emitted when feature `mobile-android` is off, `adb` is missing, or an
/// Android action is not yet hermetically wired. With `mobile-android` +
/// reachable `adb`, prefer [`crate::AdbAndroidDriver`].
pub const MOBILE_ANDROID_STUB: &str = "EIDOLON_MOBILE_ANDROID_STUB";

/// Unknown / other mobile platform label (not ios or android).
pub const MOBILE_OTHER_STUB: &str = "EIDOLON_MOBILE_OTHER_STUB";

/// Device discovery / listing cannot reach live instruments (`xcrun` / `adb`).
///
/// With `mobile-ios` / `mobile-android` and present CLIs, prefer
/// [`crate::InstrumentDiscovery`]. [`crate::InMemoryDeviceManager`] remains
/// the hermetic test shape.
pub const MOBILE_DISCOVERY_UNAVAILABLE: &str = "EIDOLON_MOBILE_DISCOVERY_UNAVAILABLE";

/// Destructive mobile action blocked until `EIDOLON_MOBILE_ALLOW_ACTIONS=1`.
///
/// List-devices and hermetic version probes stay ungated. Tap / swipe /
/// text / live screenshot writes require the explicit env gate.
pub const MOBILE_ACTIONS_GATED: &str = "EIDOLON_MOBILE_ACTIONS_GATED";

/// iOS XCUI-style tap/swipe/text/viewport needs a configured bridge.
///
/// Emitted when `EIDOLON_MOBILE_ALLOW_ACTIONS=1` but neither
/// [`crate::cli::IOS_XCUI_BUNDLE_ENV`], a discovered bundled
/// `eidolon-xcui-helper` / in-tree XCUITest runner, nor [`crate::cli::IOS_ALLOW_APPLESCRIPT_ENV`]`=1`
/// provides a usable backend. Never pretends XCUI succeeded without
/// tools/project. Full XCUI still needs an Xcode test host.
pub const MOBILE_IOS_XCUI_UNAVAILABLE: &str = "EIDOLON_MOBILE_IOS_XCUI_UNAVAILABLE";

/// Android UiAutomator2 server / instrumentation / HTTP session path is unavailable.
///
/// Emitted when feature `mobile-uia2` is off, APKs cannot be resolved
/// (env override, checkout `assets/uia2`, or durable SHA-256-verified cache),
/// packages are not installed on device, the instrumentation server is not
/// running, or the Appium-compatible HTTP client (`Uia2HttpClient` /
/// `AppiumSessionClient`) cannot reach `:6790` / `:4723` / gets an Appium
/// error body. Never pretends UIA2 succeeded without APK/server/HTTP. Fetch:
/// `eidolon-fetch-uia2` / `Uia2ApkPaths::ensure`.
pub const MOBILE_UIA2_UNAVAILABLE: &str = "EIDOLON_MOBILE_UIA2_UNAVAILABLE";

/// Appium **server** tools / HTTP `/status` path is unavailable.
///
/// Emitted when feature `mobile-appium` callers require a live Appium server
/// but `appium` CLI / `APPIUM_HOME` / `EIDOLON_APPIUM_URL` are absent or
/// `GET /status` fails. Discovery itself is hermetic when tools are missing.
/// Not Electron Appium Desktop GUI.
pub const MOBILE_APPIUM_UNAVAILABLE: &str = "EIDOLON_MOBILE_APPIUM_UNAVAILABLE";
