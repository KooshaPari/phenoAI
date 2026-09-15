//! Dispatcher — routes JSON-RPC method calls to the correct port adapter.
//!
//! Holds Arc references to each port trait object so it is cheaply cloneable
//! and can be shared across async tasks.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::{json, Value};
use tracing::{instrument, warn};

use crate::domain::input::{Key, KeyAction, MouseAction, MouseButton, MouseEvent, ScrollDirection};
use crate::domain::process::ProcessHandle;
use crate::domain::window::WindowFilter;
use crate::ipc::mod_types::{Request, Response};
use crate::modality::registry::SelectedModality;
use crate::ports::{AnalysisPort, CapturePort, InputPort, ProcessPort, WindowPort};

pub struct Dispatcher {
    pub capture: Arc<dyn CapturePort>,
    pub input: Arc<dyn InputPort>,
    pub windows: Arc<dyn WindowPort>,
    pub process: Arc<dyn ProcessPort>,
    pub analysis: Arc<dyn AnalysisPort>,
    /// The modality that was selected at App::build() time. Surfaced via
    /// the `ping` method so MCP clients / playcua-cli can see which
    /// environment they are talking to.
    pub selected_modality: SelectedModality,
}

impl Dispatcher {
    pub fn new(
        capture: Arc<dyn CapturePort>,
        input: Arc<dyn InputPort>,
        windows: Arc<dyn WindowPort>,
        process: Arc<dyn ProcessPort>,
        analysis: Arc<dyn AnalysisPort>,
        selected_modality: SelectedModality,
    ) -> Self {
        Self {
            capture,
            input,
            windows,
            process,
            analysis,
            selected_modality,
        }
    }

    /// Dispatch a JSON-RPC request to the appropriate port method.
    #[instrument(name = "dispatcher.dispatch", skip(self), fields(method = %req.method))]
    pub async fn dispatch(&self, req: Request) -> Response {
        let id = req.id.clone();
        let params = req.params.unwrap_or(Value::Null);

        match req.method.as_str() {
            "ping" => {
                let m = &self.selected_modality;
                Response::ok(
                    id,
                    json!({
                        "ok": true,
                        "version": env!("CARGO_PKG_VERSION"),
                        "modality": {
                            "kind": m.kind.as_str(),
                            "describe": m.describe,
                            "detail": m.detail,
                            "available": m.available,
                        }
                    }),
                )
            }

            "screenshot" => self.handle_screenshot(id, params).await,

            "input.key" => self.handle_input_key(id, params).await,
            "input.type" => self.handle_input_type(id, params).await,
            "input.click" => self.handle_input_click(id, params).await,
            "input.scroll" => self.handle_input_scroll(id, params).await,
            "input.move" => self.handle_input_move(id, params).await,

            "windows.list" => self.handle_windows_list(id).await,
            "windows.focus" => self.handle_windows_focus(id, params).await,
            "windows.find" => self.handle_windows_find(id, params).await,

            "process.launch" => self.handle_process_launch(id, params).await,
            "process.kill" => self.handle_process_kill(id, params).await,
            "process.status" => self.handle_process_status(id, params).await,

            "analysis.diff" => self.handle_analysis_diff(id, params).await,
            "analysis.hash" => self.handle_analysis_hash(id, params).await,

            unknown => {
                warn!("Unknown method: {unknown}");
                Response::method_not_found(id, unknown)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Screenshot
    // -----------------------------------------------------------------------

    async fn handle_screenshot(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize, Default)]
        struct P {
            window_title: Option<String>,
            monitor: Option<u32>,
        }
        let p: P = match deserialize_or_default(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e),
        };
        let result = if let Some(ref title) = p.window_title {
            self.capture.capture_window(Some(title.as_str())).await
        } else {
            self.capture.capture_display(p.monitor.unwrap_or(0)).await
        };
        match result {
            Ok(frame) => Response::ok(
                id,
                json!({
                    "data": frame.data,
                    "width": frame.width,
                    "height": frame.height,
                    "format": "png",
                }),
            ),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------------

    async fn handle_input_key(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            key: String,
            action: InputKeyAction,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum InputKeyAction {
            Press,
            Down,
            Up,
        }

        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        let action = match p.action {
            InputKeyAction::Press => KeyAction::Press,
            InputKeyAction::Down => KeyAction::Down,
            InputKeyAction::Up => KeyAction::Up,
        };
        match self.input.key_event(Key::new(p.key), action).await {
            Ok(()) => Response::ok(id, json!({ "ok": true })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_input_type(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            text: String,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        match self.input.type_text(&p.text).await {
            Ok(()) => Response::ok(id, json!({ "ok": true })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_input_click(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            x: i32,
            y: i32,
            button: IpcMouseButton,
            action: IpcMouseAction,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum IpcMouseButton {
            Left,
            Right,
            Middle,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum IpcMouseAction {
            Click,
            Down,
            Up,
        }

        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        let button = match p.button {
            IpcMouseButton::Left => MouseButton::Left,
            IpcMouseButton::Right => MouseButton::Right,
            IpcMouseButton::Middle => MouseButton::Middle,
        };
        let action = match p.action {
            IpcMouseAction::Click => MouseAction::Click,
            IpcMouseAction::Down => MouseAction::Down,
            IpcMouseAction::Up => MouseAction::Up,
        };
        let event = MouseEvent::Click {
            x: p.x,
            y: p.y,
            button,
            action,
        };
        match self.input.mouse_event(event).await {
            Ok(()) => Response::ok(id, json!({ "ok": true })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_input_scroll(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            x: i32,
            y: i32,
            direction: IpcScrollDir,
            amount: Option<i32>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum IpcScrollDir {
            Up,
            Down,
            Left,
            Right,
        }

        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        let direction = match p.direction {
            IpcScrollDir::Up => ScrollDirection::Up,
            IpcScrollDir::Down => ScrollDirection::Down,
            IpcScrollDir::Left => ScrollDirection::Left,
            IpcScrollDir::Right => ScrollDirection::Right,
        };
        let event = MouseEvent::Scroll {
            x: p.x,
            y: p.y,
            direction,
            amount: p.amount.unwrap_or(3),
        };
        match self.input.mouse_event(event).await {
            Ok(()) => Response::ok(id, json!({ "ok": true })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_input_move(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            x: i32,
            y: i32,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        let event = MouseEvent::Move { x: p.x, y: p.y };
        match self.input.mouse_event(event).await {
            Ok(()) => Response::ok(id, json!({ "ok": true })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Windows
    // -----------------------------------------------------------------------

    async fn handle_windows_list(&self, id: Value) -> Response {
        match self.windows.list_windows().await {
            Ok(wins) => match serde_json::to_value(&wins) {
                Ok(v) => Response::ok(id, v),
                Err(e) => Response::internal_error(id, e.to_string()),
            },
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_windows_focus(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            hwnd: usize,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        match self.windows.focus_window(p.hwnd).await {
            Ok(()) => Response::ok(id, json!({ "ok": true })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_windows_find(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize, Default)]
        struct P {
            title: Option<String>,
            pid: Option<u32>,
        }
        let p: P = match deserialize_or_default(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e),
        };
        let filter = WindowFilter {
            title: p.title,
            pid: p.pid,
        };
        match self.windows.find_window(filter).await {
            Ok(Some(w)) => match serde_json::to_value(&w) {
                Ok(v) => Response::ok(id, v),
                Err(e) => Response::internal_error(id, e.to_string()),
            },
            Ok(None) => Response::ok(id, Value::Null),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Process
    // -----------------------------------------------------------------------

    async fn handle_process_launch(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            path: String,
            args: Option<Vec<String>>,
            cwd: Option<String>,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        let handle = ProcessHandle {
            path: p.path,
            args: p.args.unwrap_or_default(),
            cwd: p.cwd,
        };
        match self.process.launch(handle).await {
            Ok(pid) => Response::ok(id, json!({ "pid": pid })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_process_kill(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            pid: u32,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        match self.process.kill(p.pid).await {
            Ok(()) => Response::ok(id, json!({ "ok": true })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_process_status(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            pid: u32,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        match self.process.status(p.pid).await {
            Ok(st) => Response::ok(
                id,
                json!({ "running": st.running, "exit_code": st.exit_code }),
            ),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Analysis
    // -----------------------------------------------------------------------

    async fn handle_analysis_diff(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            image_a: String,
            image_b: String,
            threshold: Option<f32>,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        let bytes_a = match BASE64.decode(&p.image_a) {
            Ok(b) => b,
            Err(e) => return Response::invalid_params(id, format!("image_a base64: {e}")),
        };
        let bytes_b = match BASE64.decode(&p.image_b) {
            Ok(b) => b,
            Err(e) => return Response::invalid_params(id, format!("image_b base64: {e}")),
        };
        let threshold = p.threshold.unwrap_or(0.02);
        match self.analysis.diff(&bytes_a, &bytes_b, threshold).await {
            Ok(r) => Response::ok(
                id,
                json!({ "changed": r.changed, "change_ratio": r.change_ratio }),
            ),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }

    async fn handle_analysis_hash(&self, id: Value, params: Value) -> Response {
        #[derive(serde::Deserialize)]
        struct P {
            image: String,
        }
        let p: P = match serde_json::from_value(params) {
            Ok(v) => v,
            Err(e) => return Response::invalid_params(id, e.to_string()),
        };
        let bytes = match BASE64.decode(&p.image) {
            Ok(b) => b,
            Err(e) => return Response::invalid_params(id, format!("image base64: {e}")),
        };
        match self.analysis.hash(&bytes).await {
            Ok(r) => Response::ok(id, json!({ "hash": r.hash })),
            Err(e) => Response::internal_error(id, e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn deserialize_or_default<T: serde::de::DeserializeOwned + Default>(
    params: Value,
) -> Result<T, String> {
    if params.is_null() {
        Ok(T::default())
    } else {
        serde_json::from_value(params).map_err(|e| e.to_string())
    }
}
