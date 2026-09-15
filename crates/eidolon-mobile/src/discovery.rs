//! Mobile device discovery hooks + live instrument listing.
//!
//! # Status
//!
//! - Always-on: [`MobileDeviceDiscovery`] trait + fail-loud [`DiscoveryStub`].
//! - Feature `mobile-ios` / `mobile-android`: [`InstrumentDiscovery`] lists
//!   devices via `simctl` / `adb` when tools are present; missing tools →
//!   [`codes::MOBILE_DISCOVERY_UNAVAILABLE`] (never invents devices).
//!
//! The in-memory [`crate::InMemoryDeviceManager`] remains the hermetic
//! test/reference shape.

use crate::codes;
use crate::{DeviceInfo, Modality};
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Trait hooks for live iOS/Android device discovery.
pub trait MobileDeviceDiscovery: Send + Sync {
    /// List reachable devices via platform instruments.
    fn list_connected(&self) -> Result<Vec<DeviceInfo>>;

    /// Whether discovery backends (xcrun / adb) are wired and ready.
    fn instruments_ready(&self) -> bool {
        false
    }
}

/// Fail-loud discovery stub — never invents devices.
#[derive(Debug, Default, Clone)]
pub struct DiscoveryStub;

impl DiscoveryStub {
    pub fn new() -> Self {
        Self
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::MOBILE_DISCOVERY_UNAVAILABLE,
            format!(
                "MobileDeviceDiscovery::{method} not implemented — enable \
                 feature `mobile-ios` and/or `mobile-android` with system \
                 xcrun/adb, or use InstrumentDiscovery \
                 (docs/EXTRACTION_PLAN.md; InMemoryDeviceManager is for tests)"
            ),
        )
    }
}

impl MobileDeviceDiscovery for DiscoveryStub {
    fn list_connected(&self) -> Result<Vec<DeviceInfo>> {
        Err(Self::unsupported("list_connected"))
    }
}

/// Helper: document the expected modality for mobile discovery results.
pub fn mobile_modality() -> Modality {
    Modality::Mobile
}

/// Live discovery over system `xcrun simctl` / `adb` (feature-gated).
///
/// Construct always succeeds; [`list_connected`] fails loud when no enabled
/// backend has a reachable CLI. Empty device lists are success with `[]`.
#[derive(Debug, Default, Clone)]
#[cfg(any(feature = "mobile-ios", feature = "mobile-android"))]
pub struct InstrumentDiscovery {
    include_ios: bool,
    include_android: bool,
}

#[cfg(any(feature = "mobile-ios", feature = "mobile-android"))]
impl InstrumentDiscovery {
    /// Discover on every compiled backend (`mobile-ios` / `mobile-android`).
    pub fn new() -> Self {
        Self {
            include_ios: cfg!(feature = "mobile-ios"),
            include_android: cfg!(feature = "mobile-android"),
        }
    }

    /// Restrict to iOS instruments only.
    pub fn ios_only() -> Self {
        Self {
            include_ios: cfg!(feature = "mobile-ios"),
            include_android: false,
        }
    }

    /// Restrict to Android instruments only.
    pub fn android_only() -> Self {
        Self {
            include_ios: false,
            include_android: cfg!(feature = "mobile-android"),
        }
    }
}

#[cfg(any(feature = "mobile-ios", feature = "mobile-android"))]
impl MobileDeviceDiscovery for InstrumentDiscovery {
    fn list_connected(&self) -> Result<Vec<DeviceInfo>> {
        let mut out = Vec::new();
        let mut attempted = 0usize;
        let mut succeeded = 0usize;
        let mut last_err: Option<PhenoError> = None;

        #[cfg(feature = "mobile-ios")]
        if self.include_ios {
            attempted += 1;
            match crate::XcrunIosDriver::try_new().and_then(|d| d.list_devices()) {
                Ok(mut devices) => {
                    succeeded += 1;
                    out.append(&mut devices);
                }
                Err(e) => last_err = Some(e),
            }
        }

        #[cfg(feature = "mobile-android")]
        if self.include_android {
            attempted += 1;
            match crate::AdbAndroidDriver::try_new().and_then(|d| d.list_devices()) {
                Ok(mut devices) => {
                    succeeded += 1;
                    out.append(&mut devices);
                }
                Err(e) => last_err = Some(e),
            }
        }

        // At least one backend listed successfully (possibly empty) → Ok.
        if succeeded > 0 {
            return Ok(out);
        }

        if attempted == 0 {
            return Err(PhenoError::unsupported_platform(
                codes::MOBILE_DISCOVERY_UNAVAILABLE,
                "InstrumentDiscovery has no enabled backends — enable \
                 mobile-ios and/or mobile-android",
            ));
        }

        Err(last_err.unwrap_or_else(|| {
            PhenoError::unsupported_platform(
                codes::MOBILE_DISCOVERY_UNAVAILABLE,
                "InstrumentDiscovery: no reachable xcrun/adb instruments \
                 (docs/EXTRACTION_PLAN.md)",
            )
        }))
    }

    fn instruments_ready(&self) -> bool {
        let mut ready = false;
        #[cfg(feature = "mobile-ios")]
        if self.include_ios {
            ready |= crate::ios::probe::xcrun_cli_ready();
        }
        #[cfg(feature = "mobile-android")]
        if self.include_android {
            ready |= crate::android::probe::adb_cli_ready();
        }
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_fail_loud() {
        let stub = DiscoveryStub::new();
        assert!(!stub.instruments_ready());
        let err = stub.list_connected().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_DISCOVERY_UNAVAILABLE)
        );
    }

    #[test]
    fn mobile_modality_is_mobile() {
        assert_eq!(mobile_modality(), Modality::Mobile);
    }
}
