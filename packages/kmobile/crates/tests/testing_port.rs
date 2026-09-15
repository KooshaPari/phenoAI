//! Integration tests for the `TestingPort` hexagonal port.
//!
//! `TestingPort` is the trait that drives test automation
//! (run / record / replay / per-device).  The production adapter
//! lives in the `kmobile` CLI crate; these tests verify the
//! trait contract via an in-memory mock from a separate test
//! binary.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use kmobile_core::{TestResult, TestingPort};

// ──────────────────────────────────────────────────────────────────────
// Test-only mock adapter
// ──────────────────────────────────────────────────────────────────────

/// In-memory mock of [`TestingPort`] that records every call and
/// returns synthetic results.
#[derive(Debug, Default, Clone)]
struct MockTestingPort {
    recorded: Arc<Mutex<Vec<String>>>,
    calls: Arc<Mutex<Vec<MockTestCall>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MockTestCall {
    Run {
        suite: Option<String>,
        device: Option<String>,
    },
    Record(String),
    Replay(String),
    RunDevice {
        device: String,
        suite: Option<String>,
    },
}

impl MockTestingPort {
    fn new() -> Self {
        Self::default()
    }

    fn call_log(&self) -> Vec<MockTestCall> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait]
impl TestingPort for MockTestingPort {
    async fn run_tests(
        &self,
        suite: Option<&str>,
        device: Option<&str>,
    ) -> anyhow::Result<TestResult> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockTestCall::Run {
                suite: suite.map(str::to_string),
                device: device.map(str::to_string),
            });
        Ok(TestResult {
            suite: suite.unwrap_or("default").to_string(),
            passed: 12,
            failed: 0,
        })
    }

    async fn record_test(&self, output: &str) -> anyhow::Result<()> {
        self.recorded
            .lock()
            .expect("recorded lock")
            .push(output.into());
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockTestCall::Record(output.into()));
        Ok(())
    }

    async fn replay_test(&self, file: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockTestCall::Replay(file.into()));
        Ok(())
    }

    async fn run_device_tests(&self, id: &str, suite: Option<&str>) -> anyhow::Result<TestResult> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockTestCall::RunDevice {
                device: id.into(),
                suite: suite.map(str::to_string),
            });
        Ok(TestResult {
            suite: suite.unwrap_or("default").to_string(),
            passed: 7,
            failed: 1,
        })
    }
}

// ──────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────

/// FR-KMOBILE-PORT-TEST-000 — `run_tests` with no arguments
/// returns a default suite result and records the call with
/// `suite = None, device = None`.
#[tokio::test]
async fn testing_port_run_tests_default() {
    let port = MockTestingPort::new();

    let result = port.run_tests(None, None).await.expect("run");

    assert_eq!(result.suite, "default");
    assert_eq!(result.passed, 12);
    assert_eq!(result.failed, 0);

    assert_eq!(
        port.call_log(),
        vec![MockTestCall::Run {
            suite: None,
            device: None,
        }]
    );
}

/// FR-KMOBILE-PORT-TEST-001 — `run_tests` with both `suite` and
/// `device` forwards the arguments to the adapter and uses the
/// suite name in the result.
#[tokio::test]
async fn testing_port_run_tests_with_args() {
    let port = MockTestingPort::new();

    let result = port
        .run_tests(Some("ui"), Some("iphone-15-pro"))
        .await
        .expect("run");

    assert_eq!(result.suite, "ui");

    assert_eq!(
        port.call_log(),
        vec![MockTestCall::Run {
            suite: Some("ui".into()),
            device: Some("iphone-15-pro".into()),
        }]
    );
}

/// FR-KMOBILE-PORT-TEST-002 — `record_test` and `replay_test`
/// round-trip: a recorded test can be replayed by file path, and
/// both calls are recorded in the call log.
#[tokio::test]
async fn testing_port_record_and_replay() {
    let port = MockTestingPort::new();

    port.record_test("ui: tap login button")
        .await
        .expect("record");
    port.record_test("ui: verify dashboard")
        .await
        .expect("record");
    port.replay_test("/tmp/recording.json")
        .await
        .expect("replay");

    assert_eq!(
        port.call_log(),
        vec![
            MockTestCall::Record("ui: tap login button".into()),
            MockTestCall::Record("ui: verify dashboard".into()),
            MockTestCall::Replay("/tmp/recording.json".into()),
        ]
    );
}

/// FR-KMOBILE-PORT-TEST-003 — `run_device_tests` targets a
/// specific device and returns a per-device result.
#[tokio::test]
async fn testing_port_run_device_tests() {
    let port = MockTestingPort::new();

    let result = port
        .run_device_tests("iphone-15-pro", Some("ui"))
        .await
        .expect("run device tests");

    assert_eq!(result.suite, "ui");
    assert_eq!(result.passed, 7);
    assert_eq!(result.failed, 1);

    assert_eq!(
        port.call_log(),
        vec![MockTestCall::RunDevice {
            device: "iphone-15-pro".into(),
            suite: Some("ui".into()),
        }]
    );
}

/// FR-KMOBILE-PORT-TEST-004 — `TestResult` round-trips through
/// JSON so test reports can be persisted and shipped over MCP.
#[tokio::test]
async fn testing_port_test_result_serde_roundtrip() {
    let original = TestResult {
        suite: "ui".into(),
        passed: 42,
        failed: 3,
    };

    let json = serde_json::to_string(&original).expect("serialize");
    let recovered: TestResult = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(recovered.suite, original.suite);
    assert_eq!(recovered.passed, original.passed);
    assert_eq!(recovered.failed, original.failed);
}

/// FR-KMOBILE-PORT-TEST-005 — the trait is dyn-compatible and
/// can be shared across awaits via `Arc<dyn TestingPort>`.
#[tokio::test]
async fn testing_port_is_dyn_compatible() {
    let port: Arc<dyn TestingPort> = Arc::new(MockTestingPort::new());

    let p1 = Arc::clone(&port);
    let p2 = Arc::clone(&port);
    let h1 = tokio::spawn(async move { p1.run_tests(None, None).await });
    let h2 = tokio::spawn(async move { p2.run_tests(Some("a"), None).await });

    let r1 = h1.await.expect("join 1").expect("run 1");
    let r2 = h2.await.expect("join 2").expect("run 2");

    assert_eq!(r1.suite, "default");
    assert_eq!(r2.suite, "a");
}
