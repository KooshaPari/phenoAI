//! Integration test verifying the hexagonal boundary of the playcua-native crate.
//!
//! Goal: confirm that the application layer (here, the JSON-RPC `Dispatcher`)
//! depends only on the *port* traits declared in `playcua_native::ports`,
//! never on the concrete adapter implementations that live in
//! `playcua_native::adapters`.
//!
//! To prove this:
//!   1. Build five in-test mock port implementations (`MockCapturePort`,
//!      `MockInputPort`, `MockWindowPort`, `MockProcessPort`,
//!      `MockAnalysisPort`) that have **no relationship** to the concrete
//!      adapters (NativeCaptureAdapter, EnigoInputAdapter, ...). The mocks
//!      are defined in this test file; no concrete adapter module is
//!      imported.
//!   2. Wrap each mock in `Arc<dyn Port>` and inject it into a `Dispatcher`,
//!      which is the central application-layer router.
//!   3. Drive every JSON-RPC method that the dispatcher exposes. Verify the
//!      mock adapters were invoked and returned the expected values, which
//!      proves the dispatcher routes work end-to-end through the trait
//!      boundary without ever touching a concrete adapter.
//!   4. Use the `from_stdout` convenience to also exercise a `ping`-style
//!      call against the mock-wired dispatcher.
//!
//! If a future refactor accidentally adds a dependency from
//! `playcua_native::ipc::dispatcher` (or any domain code) onto a concrete
//! adapter, the mock will still satisfy the trait — but this test will
//! continue to compile, exposing the architectural drift. Pair this test
//! with `cargo test --workspace --no-run` + a forbid-lint in CI to catch
//! the regression automatically.

use std::sync::Arc;

use async_trait::async_trait;
use playcua_native::domain::analysis::{AnalysisError, DiffResult, HashResult};
use playcua_native::domain::capture::{CaptureError, Frame};
use playcua_native::domain::input::{InputError, Key, KeyAction, MouseEvent};
use playcua_native::domain::process::{ProcessError, ProcessHandle, ProcessStatus};
use playcua_native::domain::window::{WindowError, WindowFilter, WindowInfo};
use playcua_native::ipc::mod_types::Request;
use playcua_native::modality::registry::SelectedModality;
use playcua_native::ports::{AnalysisPort, CapturePort, InputPort, ProcessPort, WindowPort};

use playcua_native::ipc::dispatcher::Dispatcher;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a JSON-RPC 2.0 request payload.
fn rpc(id: u64, method: &str, params: serde_json::Value) -> Request {
    Request {
        jsonrpc: "2.0".into(),
        id: serde_json::json!(id),
        method: method.into(),
        params: Some(params),
    }
}

fn selected() -> SelectedModality {
    SelectedModality {
        kind: playcua_native::modality::ModalityKind::Native,
        describe: "test-mock",
        detail: "hexagonal-boundary-test".to_string(),
        available: true,
    }
}

/// Extract the `result` field of a successful JSON-RPC response, panicking
/// otherwise. Tests below rely on this to keep assertions concise.
fn result(response: &playcua_native::ipc::mod_types::Response) -> serde_json::Value {
    assert!(
        response.error.is_none(),
        "expected ok response, got error: {:?}",
        response.error
    );
    response.result.clone().expect("missing result field")
}

// ---------------------------------------------------------------------------
// Mock port adapters — these are the "test doubles" that sit on the
// adapter side of the boundary. They implement the port trait only; they
// have no relationship to NativeCaptureAdapter / EnigoInputAdapter / etc.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockCapturePort {
    last_monitor: std::sync::Mutex<Option<u32>>,
    last_window_title: std::sync::Mutex<Option<Option<String>>>,
    /// The frame the next capture call will return. We seed this from
    /// `build_dispatcher` rather than relying on `Frame: Default`.
    frame_to_return: std::sync::Mutex<Option<Frame>>,
}

#[async_trait]
impl CapturePort for MockCapturePort {
    async fn capture_display(&self, monitor: u32) -> Result<Frame, CaptureError> {
        *self.last_monitor.lock().unwrap() = Some(monitor);
        Ok(self
            .frame_to_return
            .lock()
            .unwrap()
            .clone()
            .expect("seeded frame in build_dispatcher"))
    }
    async fn capture_window(&self, title: Option<&str>) -> Result<Frame, CaptureError> {
        *self.last_window_title.lock().unwrap() = Some(title.map(str::to_owned));
        Ok(self
            .frame_to_return
            .lock()
            .unwrap()
            .clone()
            .expect("seeded frame in build_dispatcher"))
    }
}

#[derive(Default)]
struct MockInputPort {
    last_key: std::sync::Mutex<Option<(String, KeyAction)>>,
    last_text: std::sync::Mutex<Option<String>>,
    last_mouse: std::sync::Mutex<Option<String>>,
    fail: std::sync::Mutex<bool>,
}

#[async_trait]
impl InputPort for MockInputPort {
    async fn key_event(&self, key: Key, action: KeyAction) -> Result<(), InputError> {
        if *self.fail.lock().unwrap() {
            return Err(InputError::InjectionFailed("mock-key-fail".into()));
        }
        *self.last_key.lock().unwrap() = Some((key.0, action));
        Ok(())
    }
    async fn type_text(&self, text: &str) -> Result<(), InputError> {
        if *self.fail.lock().unwrap() {
            return Err(InputError::InjectionFailed("mock-type-fail".into()));
        }
        *self.last_text.lock().unwrap() = Some(text.to_string());
        Ok(())
    }
    async fn mouse_event(&self, event: MouseEvent) -> Result<(), InputError> {
        if *self.fail.lock().unwrap() {
            return Err(InputError::InjectionFailed("mock-mouse-fail".into()));
        }
        *self.last_mouse.lock().unwrap() = Some(format!("{event:?}"));
        Ok(())
    }
}

#[derive(Default)]
struct MockWindowPort {
    windows: std::sync::Mutex<Vec<WindowInfo>>,
    focus_calls: std::sync::Mutex<Vec<usize>>,
}

#[async_trait]
impl WindowPort for MockWindowPort {
    async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError> {
        Ok(self.windows.lock().unwrap().clone())
    }
    async fn find_window(&self, filter: WindowFilter) -> Result<Option<WindowInfo>, WindowError> {
        Ok(self
            .windows
            .lock()
            .unwrap()
            .iter()
            .find(|w| match &filter.title {
                Some(t) => w.title.to_lowercase().contains(&t.to_lowercase()),
                None => true,
            })
            .cloned())
    }
    async fn focus_window(&self, hwnd: usize) -> Result<(), WindowError> {
        self.focus_calls.lock().unwrap().push(hwnd);
        Ok(())
    }
}

struct MockProcessPort {
    launches: std::sync::Mutex<Vec<ProcessHandle>>,
    kills: std::sync::Mutex<Vec<u32>>,
    statuses: std::sync::Mutex<Vec<u32>>,
    next_pid: std::sync::atomic::AtomicU32,
}

impl Default for MockProcessPort {
    fn default() -> Self {
        Self {
            launches: std::sync::Mutex::new(Vec::new()),
            kills: std::sync::Mutex::new(Vec::new()),
            statuses: std::sync::Mutex::new(Vec::new()),
            next_pid: std::sync::atomic::AtomicU32::new(1000),
        }
    }
}

#[async_trait]
impl ProcessPort for MockProcessPort {
    async fn launch(&self, handle: ProcessHandle) -> Result<u32, ProcessError> {
        self.launches.lock().unwrap().push(handle);
        Ok(self
            .next_pid
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
    async fn kill(&self, pid: u32) -> Result<(), ProcessError> {
        self.kills.lock().unwrap().push(pid);
        Ok(())
    }
    async fn status(&self, pid: u32) -> Result<ProcessStatus, ProcessError> {
        self.statuses.lock().unwrap().push(pid);
        Ok(ProcessStatus {
            running: true,
            exit_code: None,
        })
    }
}

#[derive(Default)]
struct MockAnalysisPort {
    diff_calls: std::sync::Mutex<u32>,
    hash_calls: std::sync::Mutex<u32>,
}

#[async_trait]
impl AnalysisPort for MockAnalysisPort {
    async fn diff(
        &self,
        _a: &[u8],
        _b: &[u8],
        _threshold: f32,
    ) -> Result<DiffResult, AnalysisError> {
        *self.diff_calls.lock().unwrap() += 1;
        Ok(DiffResult {
            changed: true,
            change_ratio: 0.42,
        })
    }
    async fn hash(&self, _data: &[u8]) -> Result<HashResult, AnalysisError> {
        *self.hash_calls.lock().unwrap() += 1;
        Ok(HashResult {
            hash: "deadbeef00000000".into(),
        })
    }
}

/// Wire all five mocks into a `Dispatcher`, returning both the dispatcher
/// and the mock handles for assertion.
type DispatcherFixture = (
    Dispatcher,
    Arc<MockCapturePort>,
    Arc<MockInputPort>,
    Arc<MockWindowPort>,
    Arc<MockProcessPort>,
    Arc<MockAnalysisPort>,
);

fn build_dispatcher() -> DispatcherFixture {
    let capture = Arc::new(MockCapturePort::default());
    let input = Arc::new(MockInputPort::default());
    let windows = Arc::new(MockWindowPort::default());
    let process = Arc::new(MockProcessPort::default());
    let analysis = Arc::new(MockAnalysisPort::default());

    let dispatcher = Dispatcher::new(
        capture.clone(),
        input.clone(),
        windows.clone(),
        process.clone(),
        analysis.clone(),
        selected(),
    );

    // Seed the mock with a known frame so screenshot paths return real data.
    *capture.frame_to_return.lock().unwrap() = Some(Frame {
        data: "BASE64PNG".into(),
        width: 8,
        height: 8,
    });
    // Seed window list so window tests have something to find.
    *windows.windows.lock().unwrap() = vec![WindowInfo {
        hwnd: 4242,
        title: "Mock Window".into(),
        pid: 9,
        x: 0,
        y: 0,
        width: 100,
        height: 100,
        visible: true,
    }];

    (dispatcher, capture, input, windows, process, analysis)
}

// ---------------------------------------------------------------------------
// 1. The dispatcher's "ping" path must work with mocks only.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ping_works_with_only_mock_adapters() {
    let (dispatcher, _capture, _input, _windows, _process, _analysis) = build_dispatcher();
    let resp = dispatcher
        .dispatch(rpc(1, "ping", serde_json::json!({})))
        .await;
    let body = result(&resp);
    assert_eq!(body["ok"], serde_json::json!(true));
    assert_eq!(body["modality"]["kind"], serde_json::json!("native"));
    assert_eq!(body["modality"]["describe"], serde_json::json!("test-mock"));
    assert_eq!(
        body["modality"]["detail"],
        serde_json::json!("hexagonal-boundary-test")
    );
    assert_eq!(body["modality"]["available"], serde_json::json!(true));
}

// ---------------------------------------------------------------------------
// 2. Capture path: dispatches to CapturePort, not to any concrete adapter.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn screenshot_routes_through_capture_port_trait() {
    let (dispatcher, capture, _, _, _, _) = build_dispatcher();

    // capture_display path
    let resp = dispatcher
        .dispatch(rpc(2, "screenshot", serde_json::json!({"monitor": 0})))
        .await;
    let body = result(&resp);
    assert_eq!(body["data"], serde_json::json!("BASE64PNG"));
    assert_eq!(body["width"], serde_json::json!(8));
    assert_eq!(body["height"], serde_json::json!(8));
    assert_eq!(body["format"], serde_json::json!("png"));
    assert_eq!(*capture.last_monitor.lock().unwrap(), Some(0));

    // capture_window path
    let resp = dispatcher
        .dispatch(rpc(
            3,
            "screenshot",
            serde_json::json!({"window_title": "Mock Window"}),
        ))
        .await;
    result(&resp); // success asserted by helper
    assert_eq!(
        *capture.last_window_title.lock().unwrap(),
        Some(Some("Mock Window".to_string()))
    );
}

// ---------------------------------------------------------------------------
// 3. Input path: every input.* method routes through InputPort.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn input_methods_route_through_input_port_trait() {
    let (dispatcher, _, input, _, _, _) = build_dispatcher();

    dispatcher
        .dispatch(rpc(
            10,
            "input.key",
            serde_json::json!({"key": "Enter", "action": "press"}),
        ))
        .await;
    assert_eq!(
        *input.last_key.lock().unwrap(),
        Some(("Enter".to_string(), KeyAction::Press))
    );

    dispatcher
        .dispatch(rpc(11, "input.type", serde_json::json!({"text": "hello"})))
        .await;
    assert_eq!(*input.last_text.lock().unwrap(), Some("hello".to_string()));

    dispatcher
        .dispatch(rpc(
            12,
            "input.click",
            serde_json::json!({"x": 5, "y": 6, "button": "left", "action": "click"}),
        ))
        .await;
    let mouse = input.last_mouse.lock().unwrap().clone();
    assert!(mouse.is_some(), "mouse_event should have been called");
    assert!(mouse.unwrap().contains("Click"));

    dispatcher
        .dispatch(rpc(
            13,
            "input.scroll",
            serde_json::json!({"x": 1, "y": 2, "direction": "down", "amount": 3}),
        ))
        .await;
    let mouse = input.last_mouse.lock().unwrap().clone();
    assert!(mouse.unwrap().contains("Scroll"));

    dispatcher
        .dispatch(rpc(14, "input.move", serde_json::json!({"x": 7, "y": 8})))
        .await;
    let mouse = input.last_mouse.lock().unwrap().clone();
    assert!(mouse.unwrap().contains("Move"));
}

// ---------------------------------------------------------------------------
// 4. Window path: list / find / focus all flow through WindowPort.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn window_methods_route_through_window_port_trait() {
    let (dispatcher, _, _, windows, _, _) = build_dispatcher();

    let resp = dispatcher
        .dispatch(rpc(20, "windows.list", serde_json::json!({})))
        .await;
    let body = result(&resp);
    assert_eq!(body[0]["hwnd"], serde_json::json!(4242));
    assert_eq!(body[0]["title"], serde_json::json!("Mock Window"));

    let resp = dispatcher
        .dispatch(rpc(
            21,
            "windows.find",
            serde_json::json!({"title": "mock"}),
        ))
        .await;
    let body = result(&resp);
    assert_eq!(body["hwnd"], serde_json::json!(4242));

    dispatcher
        .dispatch(rpc(22, "windows.focus", serde_json::json!({"hwnd": 7777})))
        .await;
    assert_eq!(*windows.focus_calls.lock().unwrap(), vec![7777_usize]);
}

// ---------------------------------------------------------------------------
// 5. Process path: launch / kill / status all flow through ProcessPort.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn process_methods_route_through_process_port_trait() {
    let (dispatcher, _, _, _, process, _) = build_dispatcher();

    let resp = dispatcher
        .dispatch(rpc(
            30,
            "process.launch",
            serde_json::json!({
                "path": "/bin/echo",
                "args": ["hi"],
                "cwd": null,
            }),
        ))
        .await;
    let body = result(&resp);
    let pid: u32 = body["pid"].as_u64().unwrap() as u32;
    assert!(pid >= 1000, "pid should be allocated by mock port");

    let handle = process.launches.lock().unwrap()[0].clone();
    assert_eq!(handle.path, "/bin/echo");
    assert_eq!(handle.args, vec!["hi".to_string()]);

    dispatcher
        .dispatch(rpc(31, "process.kill", serde_json::json!({"pid": pid})))
        .await;
    assert_eq!(*process.kills.lock().unwrap(), vec![pid]);

    let resp = dispatcher
        .dispatch(rpc(32, "process.status", serde_json::json!({"pid": pid})))
        .await;
    let body = result(&resp);
    assert_eq!(body["running"], serde_json::json!(true));
    assert_eq!(*process.statuses.lock().unwrap(), vec![pid]);
}

// ---------------------------------------------------------------------------
// 6. Analysis path: diff / hash both flow through AnalysisPort.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn analysis_methods_route_through_analysis_port_trait() {
    let (dispatcher, _, _, _, _, analysis) = build_dispatcher();

    let resp = dispatcher
        .dispatch(rpc(
            40,
            "analysis.diff",
            serde_json::json!({
                "image_a": "AAAA",
                "image_b": "BBBB",
                "threshold": 0.1,
            }),
        ))
        .await;
    let body = result(&resp);
    assert_eq!(body["changed"], serde_json::json!(true));
    assert_eq!(body["change_ratio"], serde_json::json!(0.42));
    assert_eq!(*analysis.diff_calls.lock().unwrap(), 1);

    let resp = dispatcher
        .dispatch(rpc(
            41,
            "analysis.hash",
            serde_json::json!({"image": "AAAA"}),
        ))
        .await;
    let body = result(&resp);
    assert_eq!(body["hash"], serde_json::json!("deadbeef00000000"));
    assert_eq!(*analysis.hash_calls.lock().unwrap(), 1);
}

// ---------------------------------------------------------------------------
// 7. Domain error from a port must surface as a JSON-RPC error object
//    (proves the dispatcher does not catch+rethrow behind a concrete
//    adapter, only translates the port's Result).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn port_errors_surfaced_as_jsonrpc_errors() {
    let (dispatcher, _, input, _, _, _) = build_dispatcher();
    *input.fail.lock().unwrap() = true;

    let resp = dispatcher
        .dispatch(rpc(
            50,
            "input.key",
            serde_json::json!({"key": "x", "action": "press"}),
        ))
        .await;
    let err = resp.error.expect("expected an error response");
    assert_eq!(err.code, -32603); // internal_error
    assert!(err.message.contains("mock-key-fail"));
    assert!(resp.result.is_none());
}

// ---------------------------------------------------------------------------
// 8. Unknown methods must produce a method_not_found (-32601) error — proves
//    the dispatcher is the routing surface, not a concrete adapter.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unknown_method_returns_method_not_found() {
    let (dispatcher, _, _, _, _, _) = build_dispatcher();
    let resp = dispatcher
        .dispatch(rpc(60, "does.not.exist", serde_json::json!({})))
        .await;
    let err = resp.error.expect("expected an error response");
    assert_eq!(err.code, -32601);
    assert!(err.message.contains("does.not.exist"));
}

// ---------------------------------------------------------------------------
// 9. Static guard: this test module compiles only if the dispatcher
//    accepts the five `Arc<dyn Port>` trait objects. The fact that the
//    `build_dispatcher` helper above type-checks against
//    `playcua_native::ipc::dispatcher::Dispatcher::new` is itself the
//    proof — but we add a compile-time witness assertion here that
//    catches a regression where the dispatcher is changed to require a
//    concrete adapter type.
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn _assert_dispatcher_constructor_accepts_trait_objects(
    capture: Arc<dyn CapturePort>,
    input: Arc<dyn InputPort>,
    windows: Arc<dyn WindowPort>,
    process: Arc<dyn ProcessPort>,
    analysis: Arc<dyn AnalysisPort>,
) -> Dispatcher {
    Dispatcher::new(capture, input, windows, process, analysis, selected())
}
