//! Integration tests for the `ProjectPort` hexagonal port.
//!
//! `ProjectPort` is the trait that drives the project lifecycle:
//! init, build, clean, status.  These tests verify the contract
//! through an in-memory mock and round-trip the `ProjectStatus`
//! DTO through serde.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use kmobile_core::{KMobileError, ProjectPort, ProjectStatus};

// ──────────────────────────────────────────────────────────────────────
// Test-only mock adapter
// ──────────────────────────────────────────────────────────────────────

/// In-memory mock of [`ProjectPort`] that tracks lifecycle state.
#[derive(Debug, Clone)]
struct MockProjectPort {
    initialized: Arc<Mutex<Option<String>>>,
    build_count: Arc<Mutex<usize>>,
    clean_count: Arc<Mutex<usize>>,
    last_target: Arc<Mutex<Option<String>>>,
    status: Arc<Mutex<ProjectStatus>>,
    calls: Arc<Mutex<Vec<MockProjCall>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MockProjCall {
    Init(String, Option<String>),
    Build(Option<String>),
    Clean,
    Status,
}

impl Default for MockProjectPort {
    fn default() -> Self {
        Self {
            initialized: Arc::default(),
            build_count: Arc::default(),
            clean_count: Arc::default(),
            last_target: Arc::default(),
            status: Arc::new(Mutex::new(ProjectStatus {
                name: "(none)".into(),
                state: "uninitialized".into(),
            })),
            calls: Arc::default(),
        }
    }
}

impl MockProjectPort {
    fn new() -> Self {
        Self::default()
    }

    fn call_log(&self) -> Vec<MockProjCall> {
        self.calls.lock().expect("calls lock").clone()
    }
}

#[async_trait]
impl ProjectPort for MockProjectPort {
    async fn init_project(&self, name: &str, template: Option<&str>) -> anyhow::Result<()> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockProjCall::Init(
                name.into(),
                template.map(str::to_string),
            ));
        *self.initialized.lock().expect("initialized lock") = Some(name.into());
        *self.status.lock().expect("status lock") = ProjectStatus {
            name: name.into(),
            state: "ready".into(),
        };
        Ok(())
    }

    async fn build_project(&self, target: Option<&str>) -> anyhow::Result<()> {
        *self.build_count.lock().expect("build count") += 1;
        *self.last_target.lock().expect("last target") = target.map(str::to_string);
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockProjCall::Build(target.map(str::to_string)));
        let initialized = self.initialized.lock().expect("initialized lock").clone();
        match initialized {
            Some(_) => Ok(()),
            None => Err(anyhow::anyhow!(KMobileError::ProjectNotFound(
                "no project initialized".into()
            ))),
        }
    }

    async fn clean_project(&self) -> anyhow::Result<()> {
        *self.clean_count.lock().expect("clean count") += 1;
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockProjCall::Clean);
        Ok(())
    }

    async fn get_project_status(&self) -> anyhow::Result<ProjectStatus> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(MockProjCall::Status);
        Ok(self.status.lock().expect("status lock").clone())
    }
}

// ──────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────

/// FR-KMOBILE-PORT-PROJ-000 — full lifecycle
/// (init → build → status → clean → status) preserves the call
/// order, updates the stored state, and exposes the status.
#[tokio::test]
async fn project_port_full_lifecycle() {
    let port = MockProjectPort::new();

    port.init_project("kmobile-app", Some("ios"))
        .await
        .expect("init");
    port.build_project(Some("ios")).await.expect("build ios");
    port.build_project(Some("android"))
        .await
        .expect("build android");

    let status = port.get_project_status().await.expect("status");
    assert_eq!(status.name, "kmobile-app");
    assert_eq!(status.state, "ready");

    port.clean_project().await.expect("clean");
    let status_after = port.get_project_status().await.expect("status");
    assert_eq!(status_after.state, "ready");

    assert_eq!(
        port.call_log(),
        vec![
            MockProjCall::Init("kmobile-app".into(), Some("ios".into())),
            MockProjCall::Build(Some("ios".into())),
            MockProjCall::Build(Some("android".into())),
            MockProjCall::Status,
            MockProjCall::Clean,
            MockProjCall::Status,
        ]
    );
}

/// FR-KMOBILE-PORT-PROJ-001 — building before initializing
/// surfaces `ProjectNotFound` so the CLI can present a useful
/// error to the user.
#[tokio::test]
async fn project_port_build_before_init_errors() {
    let port = MockProjectPort::new();

    let err = port.build_project(None).await.unwrap_err();
    let downcast = err.downcast_ref::<KMobileError>();
    assert!(matches!(downcast, Some(KMobileError::ProjectNotFound(_))));
}

/// FR-KMOBILE-PORT-PROJ-002 — `ProjectStatus` round-trips
/// through JSON, so the port can be transported over MCP / HTTP
/// / saved to disk.
#[tokio::test]
async fn project_status_serde_roundtrip() {
    let original = ProjectStatus {
        name: "kmobile-app".into(),
        state: "building".into(),
    };

    let json = serde_json::to_string(&original).expect("serialize");
    let recovered: ProjectStatus = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(recovered.name, original.name);
    assert_eq!(recovered.state, original.state);
}

/// FR-KMOBILE-PORT-PROJ-003 — the trait is `Send + Sync` and can
/// be shared across awaits via `Arc<dyn ProjectPort>`, which is
/// the shape the CLI and MCP servers use.
#[tokio::test]
async fn project_port_is_dyn_compatible() {
    let port: Arc<dyn ProjectPort> = Arc::new(MockProjectPort::new());

    // Spawn two tasks that share the port; both must be able to
    // call into the trait concurrently.
    let p1 = Arc::clone(&port);
    let p2 = Arc::clone(&port);
    let h1 = tokio::spawn(async move { p1.get_project_status().await });
    let h2 = tokio::spawn(async move { p2.init_project("p2", None).await });

    let s1 = h1.await.expect("join 1").expect("status");
    h2.await.expect("join 2").expect("init");

    assert_eq!(s1.name, "(none)");
}
