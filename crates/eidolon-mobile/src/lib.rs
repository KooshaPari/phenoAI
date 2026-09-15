//! Eidolon Mobile — iOS and Android automation.
//!
//! # Honesty (A+ T1 CLI drivers + actions + UIA2 hooks)
//!
//! - **[`MobileClient`]**: **fail-loud stub** that returns
//!   [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError) with
//!   documented [`codes`] (`EIDOLON_MOBILE_IOS_STUB` /
//!   `EIDOLON_MOBILE_ANDROID_STUB`). Prefer feature drivers for live work.
//! - **[`ios`] / [`android`]**: XCTest / UiAutomator trait hooks + fail-loud
//!   stubs; probes always on. Features `mobile-ios` / `mobile-android` enable
//!   [`XcrunIosDriver`] / [`AdbAndroidDriver`] (hermetic version probe +
//!   `list_devices`; destructive actions need
//!   [`cli::ACTIONS_ALLOW_ENV`]`=1`).
//! - **Android input**: gated `adb shell input tap|swipe|text` + screencap are
//!   **real** when tools + device + gate are present.
//! - **Android UIA2 / Appium session**: [`Uia2Server`] Appium-shaped
//!   install/start/stop via adb; [`Uia2HttpClient`] /
//!   [`AppiumSessionClient`] W3C/Appium HTTP verbs (`findElement(s)`, click,
//!   windows, contexts, actions, screenshot, timeouts) via ureq to forwarded
//!   `:6790` (or Appium `:4723`) when ready; APKs via env → `assets/uia2` →
//!   durable SHA-256 cache (`eidolon-fetch-uia2` / [`Uia2ApkPaths::ensure`]);
//!   feature `mobile-uia2`; optional [`appium_probe`] behind `mobile-appium`
//!   (`APPIUM_HOME` / `appium` / `EIDOLON_APPIUM_URL`, hermetic when absent);
//!   missing APK/server/HTTP → [`codes::MOBILE_UIA2_UNAVAILABLE`] /
//!   [`codes::MOBILE_APPIUM_UNAVAILABLE`]. **Appium-compatible session client
//!   landed**; optional Inspector/dashboard via `mobile-appium-desktop`;
//!   full Electron Appium Desktop fork remains out of scope.
//! - **iOS input**: screenshot via `simctl io` (real); tap/swipe/text/viewport
//!   via [`XcuiBridge`] — [`cli::IOS_XCUI_BUNDLE_ENV`] (wins), in-tree XCUITest
//!   runner (`native/ios/EidolonXcuiHelper`) when built, Rust
//!   `eidolon-xcui-helper` fallback (`mobile-xcui-helper`), or best-effort
//!   AppleScript when [`cli::IOS_ALLOW_APPLESCRIPT_ENV`]`=1`; else
//!   [`codes::MOBILE_IOS_XCUI_UNAVAILABLE`].

//! - **[`discovery`]**: fail-loud stub; feature backends →
//!   [`InstrumentDiscovery`].
//! - **[`kmobile_bridge`]**: in-memory `DeviceManager` port shape (tests)
//! - **[`native`]**: lower-level adapter traits + CLI adapters behind features
//!
//! See `docs/EXTRACTION_PLAN.md`. Do **not** unarchive kmobile / mobile-cli /
//! mobile-mcp for routine work.

use eidolon_core::error::PhenoError;
use eidolon_core::traits::MobileAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};

pub mod android;
pub mod cli;
pub mod codes;
pub mod discovery;
pub mod ios;
pub mod kmobile_bridge;
pub mod native;

pub use android::{AndroidStub, AndroidUiAutomatorDriver};
pub use discovery::{DiscoveryStub, MobileDeviceDiscovery};
pub use ios::{
    discover_bundled_helper, discover_preferred_helper, discover_xcode_helper, parse_argv,
    resolve_xcui_bridge, xcode_project_dir, xcui_xcode_integration_enabled, AppleScriptXcuiBridge,
    BundleXcuiBridge, HelperCommand, IosStub, IosXcTestDriver, MissingXcuiBridge, XcuiBridge,
    HELPER_BIN_NAME, XCODE_HELPER_BIN_NAME, XCODE_PROJECT_DIR_ENV, XCUI_XCODE_INTEGRATION_ENV,
};
pub use kmobile_bridge::{
    DeviceInfo, DeviceManager, InMemoryDeviceManager, Modality, TestRunReport,
};

#[cfg(feature = "mobile-ios")]
pub use ios::XcrunIosDriver;
#[cfg(feature = "mobile-android")]
pub use android::AdbAndroidDriver;
#[cfg(any(feature = "mobile-ios", feature = "mobile-android"))]
pub use discovery::InstrumentDiscovery;

pub use android::probe as android_probe;
pub use android::uia2_assets;
pub use android::uia2_server;
pub use android::{
    parse_package_status, Uia2ApkPaths, Uia2PackageStatus, Uia2Server, DEFAULT_DEVICE_PORT,
    DEFAULT_RUNNER, DEFAULT_SERVER_PACKAGE, DEFAULT_TEST_PACKAGE,
};
#[cfg(feature = "mobile-uia2")]
pub use android::{
    AppiumSessionClient, SessionWireMode, Uia2Element, Uia2HttpClient, DEFAULT_HTTP_HOST,
};
#[cfg(feature = "mobile-appium")]
pub use android::{
    appium_cli_ready, appium_tools_ready, env_appium_url, require_appium_ready,
    resolve_appium_cli, resolve_appium_home, AppiumProbe, APPIUM_HOME_ENV, APPIUM_PATH_ENV,
    APPIUM_URL_ENV, DEFAULT_APPIUM_URL,
};
#[cfg(feature = "mobile-appium")]
pub use android::appium_probe;
pub use ios::probe as ios_probe;

/// Resolve the stable [`codes`] token for a platform label.
pub fn code_for_platform(platform: &str) -> &'static str {
    match platform.to_ascii_lowercase().as_str() {
        "ios" | "iphone" | "ipad" | "tvos" | "watchos" => codes::MOBILE_IOS_STUB,
        "android" => codes::MOBILE_ANDROID_STUB,
        _ => codes::MOBILE_OTHER_STUB,
    }
}

/// Mobile automation implementer (**fail-loud stub**).
///
/// Prefer [`XcrunIosDriver`] / [`AdbAndroidDriver`] behind features for live
/// CLI-backed automation.
pub struct MobileClient {
    platform: String,
    code: &'static str,
}

impl MobileClient {
    /// Build a stub labeled with `platform` (e.g. `"ios"`, `"android"`).
    ///
    /// The error code is chosen from the **label**, so callers can assert
    /// `EIDOLON_MOBILE_IOS_STUB` vs `EIDOLON_MOBILE_ANDROID_STUB`.
    pub fn new(platform: &str) -> Self {
        Self {
            platform: platform.to_string(),
            code: code_for_platform(platform),
        }
    }

    /// Declared target label (e.g. `"ios"`, `"android"`). Informative only.
    pub fn platform(&self) -> &str {
        &self.platform
    }

    /// Stable [`codes`] token this stub emits on action failures.
    pub fn unsupported_code(&self) -> &'static str {
        self.code
    }

    fn unsupported(&self, method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            self.code,
            format!(
                "eidolon-mobile::MobileClient::{method} is not implemented \
                 (label={:?}; enable `mobile-ios` / `mobile-android` for \
                 XcrunIosDriver / AdbAndroidDriver — see \
                 docs/EXTRACTION_PLAN.md; do not unarchive kmobile routinely)",
                self.platform
            ),
        )
    }
}

#[async_trait::async_trait]
impl MobileAutomator for MobileClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        Err(self.unsupported("get_viewport"))
    }

    async fn screenshot(&self, _path: &str) -> Result<()> {
        Err(self.unsupported("screenshot"))
    }

    async fn tap(&self, _x: i32, _y: i32) -> Result<()> {
        Err(self.unsupported("tap"))
    }

    async fn swipe(&self, _x1: i32, _y1: i32, _x2: i32, _y2: i32) -> Result<()> {
        Err(self.unsupported("swipe"))
    }

    async fn input_text(&self, _text: &str) -> Result<()> {
        Err(self.unsupported("input_text"))
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded mobile event (stub): {:?}", event);
        Ok(())
    }
}
