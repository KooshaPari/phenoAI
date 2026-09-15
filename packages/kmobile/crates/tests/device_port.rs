//! Integration tests for the `DevicePort` hexagonal port.
//!
//! `DevicePort` is the trait that CLI, MCP, and Desktop adapters consume
//! to discover, connect to, install on, deploy to, and test physical
//! devices.  The trait is the only thing exported by the core —
//! concrete adapters live in other crates.
//!
//! These tests verify the **port contract** (DTO shape, async
//! signature, error type, serde) and exercise it through a
//! purpose-built in-memory mock adapter, since the production
//! adapter is platform-specific (iOS / Android) and not available
//! in CI for this crate.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use kmobile_core::{DeviceInfo, DevicePort, KMobileError};

// ──────────────────────────────────────────────────────────────────────
// Test-only mock adapter
// ──────────────────────────────────────────────────────────────────────

/// Recording, in-memory mock of [`DevicePort`].
///
/// The production adapter talks to `idevice_id`, ADB, or platform
/// SDKs; this mock is just enough to verify the trait contract
/// and any future domain code that depends on the trait object.
#[derive(Debug, Default, Clone)]
struct MockDevicePort {
    devices: Arc<Mutex<Vec<DeviceInfo>>>,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl MockDevicePort {
    fn new() -> Self {
        Self::default()
    }

    fn record(&self, call: &'static str) {
        self.calls.lock().expect("calls lock").push(call);
    }

    fn call_log(&self) -> Vec<&'static str> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn seed(&self, devices: Vec<DeviceInfo>) {
        *self.devices.lock().expect("devices lock") = devices;
    }
}

#[async_trait]
impl DevicePort for MockDevicePort {
    async fn list_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        self.record("list_devices");
        Ok(self.devices.lock().expect("devices lock").clone())
    }

    async fn connect_device(&self, id: &str) -> anyhow::Result<()> {
        self.record("connect_device");
        let devices = self.devices.lock().expect("devices lock");
        if devices.iter().any(|d| d.id == id) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(KMobileError::DeviceNotFound(
                id.to_string()
            )))
        }
    }

    async fn install_app(&self, id: &str, _app: &str) -> anyhow::Result<()> {
        self.record("install_app");
        let devices = self.devices.lock().expect("devices lock");
        if devices.iter().any(|d| d.id == id) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(KMobileError::DeviceNotFound(
                id.to_string()
            )))
        }
    }

    async fn deploy_project(&self, id: &str, _project: Option<&str>) -> anyhow::Result<()> {
        self.record("deploy_project");
        let devices = self.devices.lock().expect("devices lock");
        if devices.iter().any(|d| d.id == id) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(KMobileError::DeviceNotFound(
                id.to_string()
            )))
        }
    }

    async fn run_device_tests(&self, id: &str, _suite: Option<&str>) -> anyhow::Result<()> {
        self.record("run_device_tests");
        let devices = self.devices.lock().expect("devices lock");
        if devices.iter().any(|d| d.id == id) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(KMobileError::DeviceNotFound(
                id.to_string()
            )))
        }
    }
}

// ──────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────

/// FR-KMOBILE-PORT-DEVICE-000 — `list_devices` on an empty mock
/// returns an empty list and is awaited correctly.
#[tokio::test]
async fn device_port_list_devices_empty() {
    let port = MockDevicePort::new();

    let devices = port.list_devices().await.expect("list");

    assert!(devices.is_empty());
    assert_eq!(port.call_log(), vec!["list_devices"]);
}

/// FR-KMOBILE-PORT-DEVICE-001 — full lifecycle
/// (list → connect → install → deploy → run-tests) preserves the
/// call order and returns the expected values.
#[tokio::test]
async fn device_port_full_lifecycle() {
    let port = MockDevicePort::new();
    port.seed(vec![
        DeviceInfo {
            id: "iphone-15-pro".into(),
            name: "Koosha's iPhone 15 Pro".into(),
            platform: "ios".into(),
        },
        DeviceInfo {
            id: "pixel-8".into(),
            name: "Pixel 8".into(),
            platform: "android".into(),
        },
    ]);

    let devices = port.list_devices().await.expect("list");
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].id, "iphone-15-pro");
    assert_eq!(devices[1].platform, "android");

    port.connect_device("iphone-15-pro").await.expect("connect");
    port.install_app("iphone-15-pro", "/tmp/MyApp.app")
        .await
        .expect("install");
    port.deploy_project("iphone-15-pro", Some("kmobile-app"))
        .await
        .expect("deploy");
    port.run_device_tests("iphone-15-pro", Some("ui"))
        .await
        .expect("run tests");

    assert_eq!(
        port.call_log(),
        vec![
            "list_devices",
            "connect_device",
            "install_app",
            "deploy_project",
            "run_device_tests",
        ]
    );
}

/// FR-KMOBILE-PORT-DEVICE-002 — the port surfaces a domain error
/// (not a panic) when targeting an unknown device.
#[tokio::test]
async fn device_port_unknown_device_errors() {
    let port = MockDevicePort::new();
    port.seed(vec![DeviceInfo {
        id: "real".into(),
        name: "Real".into(),
        platform: "ios".into(),
    }]);

    let err = port.connect_device("ghost").await.unwrap_err();
    // anyhow::Error downcast: the inner KMobileError should be
    // DeviceNotFound.
    let downcast = err.downcast_ref::<KMobileError>();
    assert!(matches!(downcast, Some(KMobileError::DeviceNotFound(_))));
}

/// FR-KMOBILE-PORT-DEVICE-003 — `DeviceInfo` round-trips through
/// JSON so the port can be transported over MCP / HTTP / config.
#[tokio::test]
async fn device_info_serde_roundtrip() {
    let original = DeviceInfo {
        id: "ipad-air-5".into(),
        name: "iPad Air (5th gen)".into(),
        platform: "ios".into(),
    };

    let json = serde_json::to_string(&original).expect("serialize");
    let recovered: DeviceInfo = serde_json::from_str(&json).expect("deserialize");

    // `DeviceInfo` does not derive `PartialEq`; compare field by
    // field so a future field addition is flagged explicitly.
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.name, original.name);
    assert_eq!(recovered.platform, original.platform);

    // Also exercise a deserialized struct holding a list, since
    // list_devices returns Vec<DeviceInfo>.
    let wrapped = format!("[{json}]");
    let list: Vec<DeviceInfo> = serde_json::from_str(&wrapped).expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "ipad-air-5");
}

/// FR-KMOBILE-PORT-DEVICE-004 — the trait is dyn-compatible and
/// can be stored behind an `Arc<dyn DevicePort>`, which is the
/// shape higher-level crates (CLI / MCP / GUI) use to inject the
/// adapter at startup.
#[tokio::test]
async fn device_port_is_dyn_compatible() {
    let port: Arc<dyn DevicePort> = Arc::new(MockDevicePort::new());

    // The trait is `Send + Sync`, so the `dyn DevicePort` is
    // also `Send + Sync` — verify by sending across an await.
    let port_clone: Arc<dyn DevicePort> = Arc::clone(&port);
    let handle = tokio::spawn(async move { port_clone.list_devices().await });

    let result = handle.await.expect("join").expect("list");
    assert!(result.is_empty());
}
