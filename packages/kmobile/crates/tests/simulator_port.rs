//! Integration tests for the `SimulatorPort` hexagonal port.
//!
//! `SimulatorPort` is the trait that CLI / MCP / Desktop adapters
//! use to discover, start, stop, reset, and install on iOS / Android
//! simulators.  The production adapter (simctl / AVD) lives in
//! another crate; these tests verify the port contract via an
//! in-memory mock.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use kmobile_core::{KMobileError, SimulatorInfo, SimulatorPort};

// ──────────────────────────────────────────────────────────────────────
// Test-only mock adapter
// ──────────────────────────────────────────────────────────────────────

/// In-memory mock of [`SimulatorPort`] that records every call and
/// tracks simulator state.
#[derive(Debug, Default, Clone)]
struct MockSimulatorPort {
    simulators: Arc<Mutex<Vec<SimulatorInfo>>>,
    running: Arc<Mutex<Vec<String>>>,
    calls: Arc<Mutex<Vec<MockSimCall>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MockSimCall {
    List,
    Start(String),
    Stop(String),
    Reset(String),
    Install(String),
}

impl MockSimulatorPort {
    fn new() -> Self {
        Self::default()
    }

    fn seed(&self, sims: Vec<SimulatorInfo>) {
        *self.simulators.lock().expect("simulators lock") = sims;
    }

    fn call_log(&self) -> Vec<MockSimCall> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn is_running(&self, id: &str) -> bool {
        self.running
            .lock()
            .expect("running lock")
            .iter()
            .any(|sid| sid == id)
    }
}

#[async_trait]
impl SimulatorPort for MockSimulatorPort {
    async fn list_simulators(&self) -> anyhow::Result<Vec<SimulatorInfo>> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockSimCall::List);
        Ok(self.simulators.lock().expect("simulators lock").clone())
    }

    async fn start_simulator(&self, id: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockSimCall::Start(id.into()));
        let sims = self.simulators.lock().expect("simulators lock");
        if !sims.iter().any(|s| s.id == id) {
            return Err(anyhow::anyhow!(KMobileError::SimulatorNotFound(
                id.to_string()
            )));
        }
        drop(sims);
        if !self.is_running(id) {
            self.running
                .lock()
                .expect("running lock")
                .push(id.to_string());
        }
        Ok(())
    }

    async fn stop_simulator(&self, id: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockSimCall::Stop(id.into()));
        let sims = self.simulators.lock().expect("simulators lock");
        if !sims.iter().any(|s| s.id == id) {
            return Err(anyhow::anyhow!(KMobileError::SimulatorNotFound(
                id.to_string()
            )));
        }
        drop(sims);
        self.running
            .lock()
            .expect("running lock")
            .retain(|s| s != id);
        Ok(())
    }

    async fn reset_simulator(&self, id: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockSimCall::Reset(id.into()));
        let sims = self.simulators.lock().expect("simulators lock");
        if !sims.iter().any(|s| s.id == id) {
            return Err(anyhow::anyhow!(KMobileError::SimulatorNotFound(
                id.to_string()
            )));
        }
        Ok(())
    }

    async fn install_app(&self, id: &str, _app: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockSimCall::Install(id.into()));
        let sims = self.simulators.lock().expect("simulators lock");
        if !sims.iter().any(|s| s.id == id) {
            return Err(anyhow::anyhow!(KMobileError::SimulatorNotFound(
                id.to_string()
            )));
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────

/// FR-KMOBILE-PORT-SIM-000 — `list_simulators` returns the
/// pre-seeded simulators in their original order.
#[tokio::test]
async fn simulator_port_lists_seeded_devices() {
    let port = MockSimulatorPort::new();
    port.seed(vec![
        SimulatorInfo {
            id: "ios-17-iphone15".into(),
            name: "iPhone 15".into(),
            platform: "ios".into(),
        },
        SimulatorInfo {
            id: "android-34-pixel_7".into(),
            name: "Pixel 7 API 34".into(),
            platform: "android".into(),
        },
    ]);

    let list = port.list_simulators().await.expect("list");

    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "ios-17-iphone15");
    assert_eq!(list[1].platform, "android");
    assert_eq!(port.call_log(), vec![MockSimCall::List]);
}

/// FR-KMOBILE-PORT-SIM-001 — start → stop → reset preserves the
/// call sequence and updates the running set accordingly.
#[tokio::test]
async fn simulator_port_start_stop_reset_lifecycle() {
    let port = MockSimulatorPort::new();
    port.seed(vec![SimulatorInfo {
        id: "ios-17-iphone15".into(),
        name: "iPhone 15".into(),
        platform: "ios".into(),
    }]);

    port.start_simulator("ios-17-iphone15")
        .await
        .expect("start");
    assert!(port.is_running("ios-17-iphone15"));

    port.stop_simulator("ios-17-iphone15").await.expect("stop");
    assert!(!port.is_running("ios-17-iphone15"));

    port.start_simulator("ios-17-iphone15")
        .await
        .expect("restart");
    port.reset_simulator("ios-17-iphone15")
        .await
        .expect("reset");

    assert_eq!(
        port.call_log(),
        vec![
            MockSimCall::Start("ios-17-iphone15".into()),
            MockSimCall::Stop("ios-17-iphone15".into()),
            MockSimCall::Start("ios-17-iphone15".into()),
            MockSimCall::Reset("ios-17-iphone15".into()),
        ]
    );
}

/// FR-KMOBILE-PORT-SIM-002 — installing an app targets the
/// requested simulator id and surfaces `SimulatorNotFound` when
/// the id is unknown.
#[tokio::test]
async fn simulator_port_install_app_targets_id() {
    let port = MockSimulatorPort::new();
    port.seed(vec![SimulatorInfo {
        id: "ios-17-iphone15".into(),
        name: "iPhone 15".into(),
        platform: "ios".into(),
    }]);

    port.install_app("ios-17-iphone15", "/tmp/MyApp.app")
        .await
        .expect("install");

    let err = port.install_app("ghost", "/tmp/x.app").await.unwrap_err();
    let downcast = err.downcast_ref::<KMobileError>();
    assert!(matches!(downcast, Some(KMobileError::SimulatorNotFound(_))));

    assert_eq!(
        port.call_log(),
        vec![
            MockSimCall::Install("ios-17-iphone15".into()),
            MockSimCall::Install("ghost".into()),
        ]
    );
}

/// FR-KMOBILE-PORT-SIM-003 — `SimulatorInfo` DTO is `Send + Sync`
/// and round-trips through JSON, so the port can be transported
/// over MCP / HTTP / config files.
#[tokio::test]
async fn simulator_info_serde_roundtrip() {
    let original = SimulatorInfo {
        id: "android-34-pixel_7".into(),
        name: "Pixel 7 API 34".into(),
        platform: "android".into(),
    };

    // Compile-time check: DTO is Send + Sync (the port stores
    // Vec<SimulatorInfo> in adapter state).
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SimulatorInfo>();

    let json = serde_json::to_string(&original).expect("serialize");
    let recovered: SimulatorInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(recovered.id, original.id);
    assert_eq!(recovered.name, original.name);
    assert_eq!(recovered.platform, original.platform);
}
