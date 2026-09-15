//! Eidolon ↔ kmobile absorption bridge.
//!
//! Per findings/2026-06-17-eidolon-absorption.md (user-confirmed: Eidolon is
//! the canonical absorb target), this module provides the Eidolon-native
//! DeviceManager that supersedes kmobile-core's `DevicePort` trait.
//!
//! Migration path (2-phase):
//!
//!   Phase A (this PR): Add Eidolon-native `DeviceManager` to eidolon-mobile.
//!     kmobile-core continues to use its own `DevicePort`; consumers can
//!     already migrate Eidolon-first.
//!
//!   Phase B (follow-up): Re-point kmobile-core's `DevicePort` to a
//!     re-export of `eidolon_mobile::DeviceManager`. The kmobile-core port
//!     trait shape is preserved (same async fn signatures) so adapters
//!     like kmobile-cli/ kmobile-mcp compile unchanged.
//!
//! Device shape parity:
//!
//!   kmobile-core::ports::device::DeviceInfo  ==  eidolon_mobile::DeviceInfo
//!   kmobile-core::ports::device::DevicePort  ==  eidolon_mobile::DeviceManager
//!
//! The implementations of the four operations (list/connect/install/deploy/test)
//! live in Eidolon now. kmobile's iOS/Android bridges are absorbed in a
//! separate PR (XCTest/UiAutomator — heavy work, defer to heavy-runner).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Device information — canonical Eidolon shape.
///
/// Replaces kmobile-core::ports::device::DeviceInfo. Same fields, Eidolon
/// re-export. kmobile-core ports are forwarded via `From`/`Into`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: String,
    /// Eidolon extension: which modality of the device. kmobile was
    /// mobile-only; Eidolon unifies mobile/desktop/sandbox under one trait.
    pub modality: Modality,
    /// Eidolon extension: optional kind (e.g. "ios-simulator", "android-real",
    /// "macos", "linux-x11"). kmobile was platform-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Eidolon extension: arbitrary transport hints (e.g. "xcrun", "adb",
    /// "uia", "xcuitest").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    #[default]
    Mobile,
    Desktop,
    Sandbox,
    Browser,
    Vm,
    Container,
}

impl Modality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Modality::Mobile => "mobile",
            Modality::Desktop => "desktop",
            Modality::Sandbox => "sandbox",
            Modality::Browser => "browser",
            Modality::Vm => "vm",
            Modality::Container => "container",
        }
    }
}

/// DeviceManager — Eidolon-native port that supersedes kmobile-core's
/// `DevicePort`. Same async fn signatures; Eidolon adds modality + kind
/// + transport hints.
#[async_trait]
pub trait DeviceManager: Send + Sync {
    /// List all reachable devices across every modality.
    async fn list_devices(&self) -> anyhow::Result<Vec<DeviceInfo>>;

    /// Filter to a specific modality (mobile/desktop/sandbox/etc).
    async fn list_devices_by_modality(
        &self,
        modality: Modality,
    ) -> anyhow::Result<Vec<DeviceInfo>> {
        let all = self.list_devices().await?;
        Ok(all.into_iter().filter(|d| d.modality == modality).collect())
    }

    /// Connect to a device by ID (no-op for stateless connections).
    async fn connect_device(&self, id: &str) -> anyhow::Result<()>;

    /// Install an app on a device. Format of `app` depends on modality.
    async fn install_app(&self, id: &str, app: &str) -> anyhow::Result<()>;

    /// Deploy a project to a device.
    async fn deploy_project(&self, id: &str, project: Option<&str>) -> anyhow::Result<()>;

    /// Run tests on a device. `suite` is optional.
    async fn run_device_tests(
        &self,
        id: &str,
        suite: Option<&str>,
    ) -> anyhow::Result<TestRunReport>;

    /// Eidolon extension: open a long-lived automation session on a device.
    /// Returns a session id used by all subsequent calls.
    async fn open_session(&self, id: &str) -> anyhow::Result<String> {
        // Default no-op for stateless managers. Real implementations
        // (mobile XCTest, Android UiAutomator) override.
        let _ = id;
        Ok(uuid::Uuid::new_v4().to_string())
    }

    /// Eidolon extension: close a session opened with `open_session`.
    async fn close_session(&self, session: &str) -> anyhow::Result<()> {
        let _ = session;
        Ok(())
    }
}

/// Test-run report — minimal shape; consumers add their own fields via
/// the `metadata` map.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRunReport {
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub duration_ms: u64,
    pub metadata: HashMap<String, String>,
}

/// Eidolon-native in-memory device manager — useful for tests and as a
/// reference implementation. Real production managers wrap iOS/Android/
/// desktop backends (XCTest, UiAutomator, CoreGraphics, X11, Wayland).
pub struct InMemoryDeviceManager {
    devices: Vec<DeviceInfo>,
}

impl InMemoryDeviceManager {
    pub fn new(devices: Vec<DeviceInfo>) -> Self {
        Self { devices }
    }

    pub fn empty() -> Self {
        Self {
            devices: Vec::new(),
        }
    }
}

impl Default for InMemoryDeviceManager {
    fn default() -> Self {
        Self::empty()
    }
}

#[async_trait]
impl DeviceManager for InMemoryDeviceManager {
    async fn list_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        Ok(self.devices.clone())
    }

    async fn connect_device(&self, id: &str) -> anyhow::Result<()> {
        if self.devices.iter().any(|d| d.id == id) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("device not found: {}", id))
        }
    }

    async fn install_app(&self, id: &str, app: &str) -> anyhow::Result<()> {
        log::info!("install_app device={} app={}", id, app);
        Ok(())
    }

    async fn deploy_project(&self, id: &str, project: Option<&str>) -> anyhow::Result<()> {
        log::info!("deploy_project device={} project={:?}", id, project);
        Ok(())
    }

    async fn run_device_tests(
        &self,
        id: &str,
        suite: Option<&str>,
    ) -> anyhow::Result<TestRunReport> {
        log::info!("run_device_tests device={} suite={:?}", id, suite);
        Ok(TestRunReport::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<DeviceInfo> {
        vec![
            DeviceInfo {
                id: "ios-sim-1".into(),
                name: "iPhone 15 Pro".into(),
                platform: "ios".into(),
                modality: Modality::Mobile,
                kind: Some("ios-simulator".into()),
                transport: Some("xcrun".into()),
            },
            DeviceInfo {
                id: "mac-local".into(),
                name: "MacBook Pro".into(),
                platform: "macos".into(),
                modality: Modality::Desktop,
                kind: Some("macos".into()),
                transport: Some("core-graphics".into()),
            },
        ]
    }

    #[tokio::test]
    async fn list_devices_returns_all() {
        let m = InMemoryDeviceManager::new(sample());
        let devs = m.list_devices().await.unwrap();
        assert_eq!(devs.len(), 2);
    }

    #[tokio::test]
    async fn list_devices_by_modality_filters_correctly() {
        let m = InMemoryDeviceManager::new(sample());
        let mobile = m.list_devices_by_modality(Modality::Mobile).await.unwrap();
        assert_eq!(mobile.len(), 1);
        assert_eq!(mobile[0].id, "ios-sim-1");
        let desktop = m.list_devices_by_modality(Modality::Desktop).await.unwrap();
        assert_eq!(desktop.len(), 1);
        assert_eq!(desktop[0].id, "mac-local");
    }

    #[tokio::test]
    async fn connect_device_succeeds_for_known_id() {
        let m = InMemoryDeviceManager::new(sample());
        m.connect_device("ios-sim-1").await.unwrap();
    }

    #[tokio::test]
    async fn connect_device_fails_for_unknown_id() {
        let m = InMemoryDeviceManager::new(sample());
        let err = m.connect_device("nope").await.unwrap_err();
        assert!(err.to_string().contains("nope"));
    }

    #[tokio::test]
    async fn run_device_tests_returns_default_report() {
        let m = InMemoryDeviceManager::new(sample());
        let r = m
            .run_device_tests("ios-sim-1", Some("smoke"))
            .await
            .unwrap();
        assert_eq!(r.passed, 0);
    }

    #[tokio::test]
    async fn open_close_session_round_trip() {
        let m = InMemoryDeviceManager::new(sample());
        let s = m.open_session("ios-sim-1").await.unwrap();
        assert!(!s.is_empty());
        m.close_session(&s).await.unwrap();
    }

    #[tokio::test]
    async fn modality_serde_roundtrip() {
        let m = Modality::Mobile;
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, "\"mobile\"");
        let back: Modality = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }

    #[tokio::test]
    async fn device_info_serde_roundtrip() {
        let d = sample().remove(0);
        let s = serde_json::to_string(&d).unwrap();
        let back: DeviceInfo = serde_json::from_str(&s).unwrap();
        assert_eq!(back, d);
    }

    // -- backfill: cover install_app, deploy_project, TestRunReport, empty
    //    manager, and the Modality::as_str / debug / clone paths.

    #[tokio::test]
    async fn install_app_returns_ok() {
        let m = InMemoryDeviceManager::new(sample());
        m.install_app("ios-sim-1", "com.example.MyApp")
            .await
            .expect("install_app should succeed on any device id");
    }

    #[tokio::test]
    async fn deploy_project_returns_ok_with_and_without_project() {
        let m = InMemoryDeviceManager::new(sample());
        m.deploy_project("ios-sim-1", Some("MyProject"))
            .await
            .expect("deploy with project name");
        m.deploy_project("ios-sim-1", None)
            .await
            .expect("deploy without project name");
    }

    #[tokio::test]
    async fn empty_manager_list_devices_returns_zero() {
        let m = InMemoryDeviceManager::empty();
        let devs = m.list_devices().await.unwrap();
        assert!(devs.is_empty());
    }

    #[tokio::test]
    async fn default_manager_is_empty() {
        let m = InMemoryDeviceManager::default();
        let devs = m.list_devices().await.unwrap();
        assert!(devs.is_empty());
    }

    #[tokio::test]
    async fn list_devices_by_modality_empty_manager() {
        let m = InMemoryDeviceManager::empty();
        let devs = m.list_devices_by_modality(Modality::Mobile).await.unwrap();
        assert!(devs.is_empty());
    }

    #[tokio::test]
    async fn run_device_tests_without_suite() {
        let m = InMemoryDeviceManager::new(sample());
        let r = m.run_device_tests("ios-sim-1", None).await.unwrap();
        // Default report: all zero counts, no metadata.
        assert_eq!(r.passed, 0);
        assert_eq!(r.failed, 0);
        assert_eq!(r.skipped, 0);
        assert_eq!(r.duration_ms, 0);
        assert!(r.metadata.is_empty());
    }

    #[test]
    fn test_run_report_clone_and_debug() {
        let r = TestRunReport {
            passed: 3,
            failed: 1,
            skipped: 0,
            duration_ms: 42,
            metadata: [("key".into(), "val".into())].into(),
        };
        let cloned = r.clone();
        assert_eq!(cloned.passed, 3);
        assert_eq!(cloned.failed, 1);
        assert_eq!(cloned.duration_ms, 42);
        let dbg = format!("{r:?}");
        assert!(dbg.contains("passed"), "Debug: {dbg}");
    }

    #[test]
    fn test_run_report_serde_round_trip() {
        let r = TestRunReport {
            passed: 5,
            failed: 2,
            skipped: 1,
            duration_ms: 100,
            metadata: Default::default(),
        };
        let json = serde_json::to_string(&r).expect("serialize");
        let decoded: TestRunReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.passed, 5);
        assert_eq!(decoded.failed, 2);
    }

    #[test]
    fn modality_as_str_all_variants() {
        assert_eq!(Modality::Mobile.as_str(), "mobile");
        assert_eq!(Modality::Desktop.as_str(), "desktop");
        assert_eq!(Modality::Sandbox.as_str(), "sandbox");
        assert_eq!(Modality::Browser.as_str(), "browser");
        assert_eq!(Modality::Vm.as_str(), "vm");
        assert_eq!(Modality::Container.as_str(), "container");
    }

    #[test]
    fn modality_clone_and_debug() {
        for m in [
            Modality::Mobile,
            Modality::Desktop,
            Modality::Sandbox,
            Modality::Browser,
            Modality::Vm,
            Modality::Container,
        ] {
            let c = m; // Copy
            assert_eq!(m, c);
            let s = format!("{m:?}");
            assert!(
                !s.is_empty(),
                "Debug should produce non-empty output for {m:?}"
            );
        }
    }

    #[test]
    fn modality_default_is_mobile() {
        assert_eq!(Modality::default(), Modality::Mobile);
    }
}
