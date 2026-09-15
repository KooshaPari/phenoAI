//! Guest-side `playcua-bridge` JSON-RPC handlers (screenshot / input.* / windows.*).
//!
//! Speaks the same NDJSON JSON-RPC 2.0 surface that [`super::bridge_client::BridgeClient`]
//! expects. Screenshot, input, and window methods use the same native adapters as host
//! `playcua-native` (xcap / enigo / platform ports). Failures surface as
//! JSON-RPC errors (fail loud) — never a silent empty stub, except when
//! `PLAYCUA_BRIDGE_STUB_SCREENSHOT=1` / `PLAYCUA_BRIDGE_STUB_INPUT=1` opt into
//! hermetic stubs for CI.
//!
//! `windows.focus` on macOS/Linux matches host dispatcher: honest stub (Ok + warn).

use serde_json::{json, Value};
use tracing::warn;

use crate::app::{native_capture, native_input, native_windows};
use crate::domain::capture::{CaptureError, Frame};
use crate::domain::input::{
    InputError, Key, KeyAction, MouseAction, MouseButton, MouseEvent, ScrollDirection,
};
use crate::domain::window::WindowFilter;

use super::mod_types::{Request, Response};

/// Canonical 1×1 transparent PNG (base64), shared with FR-001 contract tests.
pub const STUB_PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// Env flag: when `1`/`true`, `screenshot` returns the hermetic stub PNG envelope
/// instead of calling guest-OS capture. Documented for CI; default is real capture.
pub const STUB_SCREENSHOT_ENV: &str = "PLAYCUA_BRIDGE_STUB_SCREENSHOT";

/// Env flag: when `1`/`true`, `input.*` acks `{ok:true}` without injecting
/// guest-OS input (CI / headless). Default is real injection via native ports.
pub const STUB_INPUT_ENV: &str = "PLAYCUA_BRIDGE_STUB_INPUT";

/// Whether screenshot is stubbed via [`STUB_SCREENSHOT_ENV`].
pub fn screenshot_capability() -> &'static str {
    if stub_screenshot_enabled() {
        "stub"
    } else {
        "real"
    }
}

/// Whether input is stubbed via [`STUB_INPUT_ENV`].
pub fn input_capability() -> &'static str {
    if stub_input_enabled() {
        "stub"
    } else {
        "real"
    }
}

/// Window enumeration/focus capability. Always `"real"` on supported platforms
/// (list/find via native adapters); focus may still be an honest platform stub
/// matching host dispatcher semantics (e.g. macOS/Linux).
pub fn windows_capability() -> &'static str {
    "real"
}

fn stub_screenshot_enabled() -> bool {
    env_flag_truthy(STUB_SCREENSHOT_ENV)
}

fn stub_input_enabled() -> bool {
    env_flag_truthy(STUB_INPUT_ENV)
}

fn env_flag_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"),
        Err(_) => false,
    }
}

/// Dispatch one bridge request. Unknown methods → `-32601` (fail loud).
pub async fn handle_request(req: Request) -> Response {
    let id = req.id.clone();
    let params = req.params.unwrap_or(Value::Null);

    match req.method.as_str() {
        "ping" => Response::ok(
            id,
            json!({
                "ok": true,
                "version": env!("CARGO_PKG_VERSION"),
                "bridge": "playcua-bridge",
                "screenshot": screenshot_capability(),
                "input": input_capability(),
                "windows": windows_capability(),
            }),
        ),
        "screenshot" => handle_screenshot(id, params).await,
        "input.key" => handle_input_key(id, params).await,
        "input.type" => handle_input_type(id, params).await,
        "input.click" => handle_input_click(id, params).await,
        "input.scroll" => handle_input_scroll(id, params).await,
        "input.move" => handle_input_move(id, params).await,
        "windows.list" => handle_windows_list(id).await,
        "windows.find" => handle_windows_find(id, params).await,
        "windows.focus" => handle_windows_focus(id, params).await,
        unknown => {
            warn!(method = %unknown, "playcua-bridge unknown method");
            Response::method_not_found(id, unknown)
        }
    }
}

async fn handle_screenshot(id: Value, params: Value) -> Response {
    #[derive(serde::Deserialize, Default)]
    struct P {
        window_title: Option<String>,
        monitor: Option<u32>,
    }
    let p: P = match serde_json::from_value(if params.is_null() { json!({}) } else { params }) {
        Ok(v) => v,
        Err(e) => return Response::invalid_params(id, e.to_string()),
    };

    if stub_screenshot_enabled() {
        return Response::ok(
            id,
            json!({
                "data": STUB_PNG_B64,
                "width": 1,
                "height": 1,
                "format": "png",
            }),
        );
    }

    let capture = native_capture();
    let result = if let Some(ref title) = p.window_title {
        capture.capture_window(Some(title.as_str())).await
    } else {
        capture.capture_display(p.monitor.unwrap_or(0)).await
    };
    match result {
        Ok(frame) => screenshot_ok(id, frame),
        Err(e) => Response::internal_error(id, format_capture_error(e)),
    }
}

fn screenshot_ok(id: Value, frame: Frame) -> Response {
    Response::ok(
        id,
        json!({
            "data": frame.data,
            "width": frame.width,
            "height": frame.height,
            "format": "png",
        }),
    )
}

fn format_capture_error(e: CaptureError) -> String {
    format!(
        "screenshot failed (guest capture): {e}; \
         grant screen-recording / display access, or set {STUB_SCREENSHOT_ENV}=1 for hermetic stub"
    )
}

fn format_input_error(method: &str, e: InputError) -> String {
    format!(
        "{method} failed (guest input): {e}; \
         grant accessibility / input injection permission, or set {STUB_INPUT_ENV}=1 for hermetic stub"
    )
}

fn input_ack(id: Value) -> Response {
    Response::ok(id, json!({ "ok": true }))
}

async fn handle_input_key(id: Value, params: Value) -> Response {
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
    if stub_input_enabled() {
        return input_ack(id);
    }
    let action = match p.action {
        InputKeyAction::Press => KeyAction::Press,
        InputKeyAction::Down => KeyAction::Down,
        InputKeyAction::Up => KeyAction::Up,
    };
    match native_input().key_event(Key::new(p.key), action).await {
        Ok(()) => input_ack(id),
        Err(e) => Response::internal_error(id, format_input_error("input.key", e)),
    }
}

async fn handle_input_type(id: Value, params: Value) -> Response {
    #[derive(serde::Deserialize)]
    struct P {
        text: String,
    }
    let p: P = match serde_json::from_value(params) {
        Ok(v) => v,
        Err(e) => return Response::invalid_params(id, e.to_string()),
    };
    if stub_input_enabled() {
        return input_ack(id);
    }
    match native_input().type_text(&p.text).await {
        Ok(()) => input_ack(id),
        Err(e) => Response::internal_error(id, format_input_error("input.type", e)),
    }
}

async fn handle_input_click(id: Value, params: Value) -> Response {
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
    if stub_input_enabled() {
        return input_ack(id);
    }
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
    match native_input().mouse_event(event).await {
        Ok(()) => input_ack(id),
        Err(e) => Response::internal_error(id, format_input_error("input.click", e)),
    }
}

async fn handle_input_scroll(id: Value, params: Value) -> Response {
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
    if stub_input_enabled() {
        return input_ack(id);
    }
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
    match native_input().mouse_event(event).await {
        Ok(()) => input_ack(id),
        Err(e) => Response::internal_error(id, format_input_error("input.scroll", e)),
    }
}

async fn handle_input_move(id: Value, params: Value) -> Response {
    #[derive(serde::Deserialize)]
    struct P {
        x: i32,
        y: i32,
    }
    let p: P = match serde_json::from_value(params) {
        Ok(v) => v,
        Err(e) => return Response::invalid_params(id, e.to_string()),
    };
    if stub_input_enabled() {
        return input_ack(id);
    }
    let event = MouseEvent::Move { x: p.x, y: p.y };
    match native_input().mouse_event(event).await {
        Ok(()) => input_ack(id),
        Err(e) => Response::internal_error(id, format_input_error("input.move", e)),
    }
}

async fn handle_windows_list(id: Value) -> Response {
    let windows = native_windows();
    match windows.list_windows().await {
        Ok(wins) => match serde_json::to_value(&wins) {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::internal_error(id, e.to_string()),
        },
        Err(e) => {
            Response::internal_error(id, format!("windows.list failed (guest enumeration): {e}"))
        }
    }
}

async fn handle_windows_find(id: Value, params: Value) -> Response {
    #[derive(serde::Deserialize, Default)]
    struct P {
        title: Option<String>,
        pid: Option<u32>,
    }
    let p: P = match serde_json::from_value(if params.is_null() { json!({}) } else { params }) {
        Ok(v) => v,
        Err(e) => return Response::invalid_params(id, e.to_string()),
    };
    let filter = WindowFilter {
        title: p.title,
        pid: p.pid,
    };
    let windows = native_windows();
    match windows.find_window(filter).await {
        Ok(Some(w)) => match serde_json::to_value(&w) {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::internal_error(id, e.to_string()),
        },
        Ok(None) => Response::ok(id, Value::Null),
        Err(e) => {
            Response::internal_error(id, format!("windows.find failed (guest enumeration): {e}"))
        }
    }
}

async fn handle_windows_focus(id: Value, params: Value) -> Response {
    #[derive(serde::Deserialize)]
    struct P {
        hwnd: usize,
    }
    let p: P = match serde_json::from_value(params) {
        Ok(v) => v,
        Err(e) => return Response::invalid_params(id, e.to_string()),
    };
    let windows = native_windows();
    match windows.focus_window(p.hwnd).await {
        Ok(()) => Response::ok(id, json!({ "ok": true })),
        Err(e) => Response::internal_error(id, format!("windows.focus failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::bridge_client::BRIDGE_ENV_LOCK;

    fn req(method: &str, params: Value) -> Request {
        Request {
            jsonrpc: "2.0".into(),
            id: json!(1),
            method: method.into(),
            params: if params.is_null() { None } else { Some(params) },
        }
    }

    #[tokio::test]
    async fn screenshot_stub_env_returns_png_envelope() {
        let _guard = BRIDGE_ENV_LOCK.lock().await;
        let prev = std::env::var(STUB_SCREENSHOT_ENV).ok();
        let prev_input = std::env::var(STUB_INPUT_ENV).ok();
        std::env::set_var(STUB_SCREENSHOT_ENV, "1");
        std::env::remove_var(STUB_INPUT_ENV);

        let resp = handle_request(req("screenshot", json!({ "monitor": 0 }))).await;
        let result = resp.result.expect("result");
        assert_eq!(result["format"], "png");
        assert_eq!(result["width"], 1);
        assert_eq!(result["height"], 1);
        assert_eq!(result["data"], STUB_PNG_B64);

        let ping = handle_request(req("ping", Value::Null)).await;
        let meta = ping.result.expect("ping");
        assert_eq!(meta["screenshot"], "stub");
        assert_eq!(meta["input"], "real");
        assert_eq!(meta["windows"], "real");

        match prev {
            Some(v) => std::env::set_var(STUB_SCREENSHOT_ENV, v),
            None => std::env::remove_var(STUB_SCREENSHOT_ENV),
        }
        match prev_input {
            Some(v) => std::env::set_var(STUB_INPUT_ENV, v),
            None => std::env::remove_var(STUB_INPUT_ENV),
        }
    }

    #[tokio::test]
    async fn ping_reports_real_capabilities_when_stubs_unset() {
        let _guard = BRIDGE_ENV_LOCK.lock().await;
        let prev_shot = std::env::var(STUB_SCREENSHOT_ENV).ok();
        let prev_input = std::env::var(STUB_INPUT_ENV).ok();
        std::env::remove_var(STUB_SCREENSHOT_ENV);
        std::env::remove_var(STUB_INPUT_ENV);

        let ping = handle_request(req("ping", Value::Null)).await;
        let meta = ping.result.expect("ping");
        assert_eq!(meta["screenshot"], "real");
        assert_eq!(meta["input"], "real");
        assert_eq!(meta["windows"], "real");
        assert_eq!(meta["bridge"], "playcua-bridge");

        match prev_shot {
            Some(v) => std::env::set_var(STUB_SCREENSHOT_ENV, v),
            None => std::env::remove_var(STUB_SCREENSHOT_ENV),
        }
        match prev_input {
            Some(v) => std::env::set_var(STUB_INPUT_ENV, v),
            None => std::env::remove_var(STUB_INPUT_ENV),
        }
    }

    #[tokio::test]
    async fn input_stub_env_acks_without_injection() {
        let _guard = BRIDGE_ENV_LOCK.lock().await;
        let prev = std::env::var(STUB_INPUT_ENV).ok();
        std::env::set_var(STUB_INPUT_ENV, "1");

        let cases = [
            ("input.key", json!({ "key": "a", "action": "press" })),
            ("input.type", json!({ "text": "hello" })),
            (
                "input.click",
                json!({ "x": 1, "y": 2, "button": "left", "action": "click" }),
            ),
            (
                "input.scroll",
                json!({ "x": 1, "y": 2, "direction": "up", "amount": 3 }),
            ),
            ("input.move", json!({ "x": 10, "y": 20 })),
        ];
        for (method, params) in cases {
            let resp = handle_request(req(method, params)).await;
            assert_eq!(
                resp.result.as_ref().unwrap()["ok"],
                true,
                "{method}: {:?}",
                resp.error
            );
        }

        let ping = handle_request(req("ping", Value::Null)).await;
        assert_eq!(ping.result.unwrap()["input"], "stub");

        match prev {
            Some(v) => std::env::set_var(STUB_INPUT_ENV, v),
            None => std::env::remove_var(STUB_INPUT_ENV),
        }
    }

    #[tokio::test]
    async fn input_invalid_params_fails_loud() {
        let _guard = BRIDGE_ENV_LOCK.lock().await;
        let prev = std::env::var(STUB_INPUT_ENV).ok();
        // Even with stub, invalid params must fail before the stub path.
        std::env::set_var(STUB_INPUT_ENV, "1");

        let resp = handle_request(req("input.key", json!({}))).await;
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32602);

        let resp = handle_request(req("input.click", json!({ "x": 1 }))).await;
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32602);

        match prev {
            Some(v) => std::env::set_var(STUB_INPUT_ENV, v),
            None => std::env::remove_var(STUB_INPUT_ENV),
        }
    }

    #[tokio::test]
    async fn windows_list_find_focus_via_native_adapters() {
        // Real enumeration may return zero or more windows; must not silently
        // invent an empty stub when the platform adapter errors.
        let list = handle_request(req("windows.list", Value::Null)).await;
        assert!(
            list.error.is_none(),
            "windows.list must succeed or fail loud: {:?}",
            list.error
        );
        let wins = list.result.expect("result");
        assert!(wins.is_array(), "windows.list must return a JSON array");

        let found = handle_request(req(
            "windows.find",
            json!({ "title": "___no_such_playcua_win___" }),
        ))
        .await;
        assert!(found.error.is_none(), "{:?}", found.error);
        assert!(found.result.unwrap().is_null());

        // Matches host: focus is Ok on macOS/Linux honest stub; Windows may
        // succeed or fail loud for a bogus hwnd — either is acceptable.
        let focus = handle_request(req("windows.focus", json!({ "hwnd": 1 }))).await;
        if let Some(err) = focus.error {
            assert_eq!(err.code, -32603, "focus failures must be internal_error");
            assert!(err.message.contains("windows.focus"), "{}", err.message);
        } else {
            assert_eq!(focus.result.unwrap()["ok"], true);
        }
    }

    #[tokio::test]
    async fn unknown_method_fails_loud() {
        let resp = handle_request(req("process.launch", json!({}))).await;
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32601);
        assert!(err.message.contains("process.launch"));
    }

    #[tokio::test]
    async fn screenshot_invalid_params_fails_loud() {
        let resp = handle_request(req("screenshot", json!("not-an-object"))).await;
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32602);
    }
}
