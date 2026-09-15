//! Native platform adapters for iOS and Android.
//!
//! Lower-level XCTest / UiAutomator adapter traits used by extraction work.
//! Prefer the public [`crate::IosXcTestDriver`] / [`crate::AndroidUiAutomatorDriver`]
//! hooks for `MobileAutomator`-shaped work.
//!
//! Fail-loud stubs: [`StubIosTestAdapter`] / [`StubAndroidTestAdapter`].
//! Feature `mobile-ios`: [`XcrunIosTestAdapter`]. Feature `mobile-android`:
//! [`AdbAndroidTestAdapter`].

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// iOS XCTest framework adapter trait.
pub trait IosTestAdapter {
    /// Execute XCTest and capture results.
    fn run_test(&self, suite: &str) -> Result<String>;

    /// Get current viewport from XCTest introspection.
    fn get_viewport(&self) -> Result<(u32, u32)>;
}

/// Android UiAutomator framework adapter trait.
pub trait AndroidTestAdapter {
    /// Execute UiAutomator command.
    fn execute(&self, cmd: &str) -> Result<String>;

    /// Get device viewport via dumpsys / wm.
    fn get_viewport(&self) -> Result<(u32, u32)>;
}

/// Fail-loud iOS XCTest stub.
pub struct StubIosTestAdapter;

impl StubIosTestAdapter {
    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::MOBILE_IOS_STUB,
            format!(
                "native::IosTestAdapter::{method} is not yet implemented — \
                 enable `mobile-ios` + XcrunIosTestAdapter \
                 (docs/EXTRACTION_PLAN.md)"
            ),
        )
    }
}

impl IosTestAdapter for StubIosTestAdapter {
    fn run_test(&self, _suite: &str) -> Result<String> {
        Err(Self::unsupported("run_test"))
    }

    fn get_viewport(&self) -> Result<(u32, u32)> {
        Err(Self::unsupported("get_viewport"))
    }
}

/// Fail-loud Android UiAutomator stub.
pub struct StubAndroidTestAdapter;

impl StubAndroidTestAdapter {
    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::MOBILE_ANDROID_STUB,
            format!(
                "native::AndroidTestAdapter::{method} is not yet implemented — \
                 enable `mobile-android` + AdbAndroidTestAdapter \
                 (docs/EXTRACTION_PLAN.md)"
            ),
        )
    }
}

impl AndroidTestAdapter for StubAndroidTestAdapter {
    fn execute(&self, _cmd: &str) -> Result<String> {
        Err(Self::unsupported("execute"))
    }

    fn get_viewport(&self) -> Result<(u32, u32)> {
        Err(Self::unsupported("get_viewport"))
    }
}

/// `xcrun` / `xcodebuild`-backed native iOS adapter (`mobile-ios`).
#[cfg(feature = "mobile-ios")]
pub struct XcrunIosTestAdapter {
    driver: crate::XcrunIosDriver,
}

#[cfg(feature = "mobile-ios")]
impl XcrunIosTestAdapter {
    pub fn try_new() -> Result<Self> {
        Ok(Self {
            driver: crate::XcrunIosDriver::try_new()?,
        })
    }
}

#[cfg(feature = "mobile-ios")]
impl IosTestAdapter for XcrunIosTestAdapter {
    fn run_test(&self, suite: &str) -> Result<String> {
        // suite format: "project.xcodeproj::SchemeName" or fail-loud.
        let Some((project, scheme)) = suite.split_once("::") else {
            return Err(PhenoError::BadRequest(
                "run_test suite must be `project.xcodeproj::Scheme` \
                 (XCTest project path required; no silent default)"
                    .into(),
            ));
        };
        self.driver
            .run_xcodebuild_test(project.trim(), scheme.trim(), None)
    }

    fn get_viewport(&self) -> Result<(u32, u32)> {
        Err(PhenoError::unsupported_platform(
            codes::MOBILE_IOS_STUB,
            "native::XcrunIosTestAdapter::get_viewport requires XCUI \
             introspection — not hermetic yet (docs/EXTRACTION_PLAN.md)",
        ))
    }
}

/// `adb`-backed native Android adapter (`mobile-android`).
#[cfg(feature = "mobile-android")]
pub struct AdbAndroidTestAdapter {
    driver: crate::AdbAndroidDriver,
}

#[cfg(feature = "mobile-android")]
impl AdbAndroidTestAdapter {
    pub fn try_new() -> Result<Self> {
        Ok(Self {
            driver: crate::AdbAndroidDriver::try_new()?,
        })
    }
}

#[cfg(feature = "mobile-android")]
impl AndroidTestAdapter for AdbAndroidTestAdapter {
    fn execute(&self, cmd: &str) -> Result<String> {
        let args: Vec<&str> = cmd.split_whitespace().collect();
        if args.is_empty() {
            return Err(PhenoError::BadRequest(
                "uiautomator execute cmd must be non-empty".into(),
            ));
        }
        self.driver.uiautomator_execute(&args)
    }

    fn get_viewport(&self) -> Result<(u32, u32)> {
        // Synchronous bridge: shell wm size via list path.
        let output = std::process::Command::new(
            crate::android::probe::resolve_adb().ok_or_else(|| {
                PhenoError::unsupported_platform(
                    codes::MOBILE_ANDROID_STUB,
                    "adb missing for get_viewport",
                )
            })?,
        )
        .args(["shell", "wm", "size"])
        .output()
        .map_err(|e| {
            PhenoError::unsupported_platform(
                codes::MOBILE_ANDROID_STUB,
                format!("wm size failed: {e}"),
            )
        })?;
        if !output.status.success() {
            return Err(PhenoError::unsupported_platform(
                codes::MOBILE_ANDROID_STUB,
                "wm size unsuccessful",
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            let rest = line
                .strip_prefix("Physical size:")
                .or_else(|| line.strip_prefix("Override size:"));
            if let Some(dims) = rest {
                let mut parts = dims.trim().split('x');
                if let (Some(w), Some(h)) = (parts.next(), parts.next()) {
                    if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                        return Ok((w, h));
                    }
                }
            }
        }
        Err(PhenoError::unsupported_platform(
            codes::MOBILE_ANDROID_STUB,
            format!("unparsed wm size: {stdout}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_ios_run_test_fails_loud() {
        let err = StubIosTestAdapter.run_test("Smoke").unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_IOS_STUB));
        assert_eq!(err.status_code(), 501);
        let msg = err.to_string();
        assert!(msg.contains("run_test"), "err = {msg}");
        assert!(msg.contains("EXTRACTION_PLAN"), "err = {msg}");
    }

    #[test]
    fn stub_android_execute_fails_loud() {
        let err = StubAndroidTestAdapter.execute("dump").unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_ANDROID_STUB));
        assert_eq!(err.status_code(), 501);
        let msg = err.to_string();
        assert!(msg.contains("execute"), "err = {msg}");
    }
}
