//! MCP server adapter — wraps the existing JSON-RPC `Dispatcher` as MCP tools.
//!
//! All 14 methods on `Dispatcher::dispatch` (screenshot, input.key, input.type,
//! input.click, input.scroll, input.move, windows.list, windows.focus,
//! windows.find, process.launch, process.kill, process.status, analysis.diff,
//! analysis.hash) are exposed as MCP tools. This lets Claude Desktop, Cursor,
//! `mcp-agent`, and any MCP-compatible client drive playcua without writing
//! a JSON-RPC pipe of their own.
//!
//! Transport: stdio (default for Claude/Cursor) or streamable HTTP (for
//! multi-client server deployments). See `bin/playcua-mcp.rs`.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ipc::dispatcher::Dispatcher;
use crate::ipc::mod_types::Request;

// ---------------------------------------------------------------------------
// Param structs (one per tool). Field-level doc comments become JSON Schema
// property descriptions, which is what MCP clients surface in their tool
// picker UIs (Claude Desktop, Cursor, mcp-agent).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScreenshotParams {
    /// Display index; omit for the primary monitor.
    #[schemars(description = "Display index (omit for primary)")]
    pub display: Option<u32>,
    /// Capture the named window instead of a full display.
    #[schemars(description = "Substring of the window title to capture (omit for full display)")]
    pub window_title: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InputKeyParams {
    /// Key name, e.g. "Return", "Tab", "ctrl+c", "shift+End".
    pub key: String,
    /// `press` (down+up), `down` (hold), or `up` (release). Default: press.
    pub action: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InputTypeParams {
    /// Literal text to type.
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ClickParams {
    /// X coordinate in screen pixels.
    pub x: i32,
    /// Y coordinate in screen pixels.
    pub y: i32,
    /// `left`, `right`, or `middle`. Default: left.
    pub button: Option<String>,
    /// `click` (down+up), `down`, or `up`. Default: click.
    pub action: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ScrollParams {
    /// X coordinate of the scroll target.
    pub x: i32,
    /// Y coordinate of the scroll target.
    pub y: i32,
    /// `up`, `down`, `left`, or `right`. Default: down.
    pub direction: Option<String>,
    /// Wheel notch count. Default: 3.
    pub amount: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MoveParams {
    /// X coordinate in screen pixels.
    pub x: i32,
    /// Y coordinate in screen pixels.
    pub y: i32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListWindowsParams {}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FocusWindowParams {
    /// Platform-specific window handle (hwnd on Windows, xcb_window_t on Linux, NSWindow on macOS).
    pub hwnd: usize,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindWindowParams {
    /// Substring of the window title.
    pub title: Option<String>,
    /// Owner process PID.
    pub pid: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LaunchParams {
    /// Path to the executable.
    pub path: String,
    /// Optional argv (excluding argv[0]).
    pub args: Option<Vec<String>>,
    /// Optional working directory.
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct KillParams {
    /// Process ID to terminate.
    pub pid: u32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StatusParams {
    /// Process ID to query.
    pub pid: u32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DiffParams {
    /// Base64-encoded PNG #1.
    pub image_a: String,
    /// Base64-encoded PNG #2.
    pub image_b: String,
    /// Per-pixel difference threshold in [0.0, 1.0]. Default: 0.02.
    pub threshold: Option<f32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HashParams {
    /// Base64-encoded PNG.
    pub image: String,
}

// ---------------------------------------------------------------------------
// The MCP server itself
// ---------------------------------------------------------------------------

/// MCP server exposing playcua's 14 IPC methods as tools.
///
/// Cheap to clone (the inner `Dispatcher` is wrapped in `Arc`, the
/// `ToolRouter` is a `#[tool_router]`-generated field).
#[derive(Clone)]
pub struct PlayCuaMcp {
    dispatcher: Arc<Dispatcher>,
    // Read by the `#[tool_handler]` macro, which the dead-code pass can't see.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl PlayCuaMcp {
    /// Build from a fresh `App::build().dispatcher`. See `crate::app::App`.
    pub fn new(dispatcher: Arc<Dispatcher>) -> Self {
        Self {
            dispatcher,
            tool_router: Self::tool_router(),
        }
    }

    /// Forward a method+params to the underlying JSON-RPC dispatcher and render
    /// the response as an MCP `CallToolResult` (one structured JSON content).
    async fn call(&self, method: &'static str, params: Value) -> Result<CallToolResult, ErrorData> {
        // The existing `Request` struct is a pure JSON-RPC 2.0 wire type with
        // no constructor — we build it inline. `id` is unused by the dispatcher
        // (it echoes whatever we pass), so a Null sentinel is fine.
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: Value::Null,
            method: method.to_string(),
            params: Some(params),
        };
        let resp = self.dispatcher.dispatch(req).await;
        if let Some(err) = resp.error {
            // Map standard JSON-RPC error codes onto rmcp's typed `ErrorCode`.
            // Anything else becomes INTERNAL_ERROR with the code preserved in
            // the message for debugging.
            let (code, msg) = match err.code {
                -32601 => (ErrorCode::METHOD_NOT_FOUND, err.message),
                -32602 => (ErrorCode::INVALID_PARAMS, err.message),
                -32603 => (ErrorCode::INTERNAL_ERROR, err.message),
                other => (
                    ErrorCode::INTERNAL_ERROR,
                    format!("[{}] {}", other, err.message),
                ),
            };
            return Err(ErrorData::new(code, msg, err.data));
        }
        let result = resp.result.unwrap_or(Value::Null);
        Ok(CallToolResult::success(vec![ContentBlock::json(result)?]))
    }
}

// ---------------------------------------------------------------------------
// Tool registrations (14 total). The `#[tool_router]` macro generates
// `PlayCuaMcp::tool_router()`; the `#[tool_handler]` macro generates the
// `ServerHandler` impl that delegates to it.
// ---------------------------------------------------------------------------

#[tool_router]
impl PlayCuaMcp {
    /// Capture the current screen (or named window) as base64-encoded PNG.
    #[tool(description = "Capture the screen (or a named window) as base64-encoded PNG bytes.")]
    async fn screenshot(
        &self,
        Parameters(p): Parameters<ScreenshotParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("screenshot", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Press, hold, or release a single key or chord.
    #[tool(description = "Press, hold, or release a key or chord (e.g. \"Return\", \"ctrl+c\").")]
    async fn input_key(
        &self,
        Parameters(p): Parameters<InputKeyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("input.key", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Type a literal string of text.
    #[tool(description = "Type a literal string of text into the focused element.")]
    async fn input_type(
        &self,
        Parameters(p): Parameters<InputTypeParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("input.type", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Click (or press/release) a mouse button at the given coordinates.
    #[tool(
        description = "Click, press, or release a mouse button at the given screen coordinates."
    )]
    async fn input_click(
        &self,
        Parameters(p): Parameters<ClickParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("input.click", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Scroll the mouse wheel at the given coordinates.
    #[tool(
        description = "Scroll the mouse wheel up, down, left, or right at the given coordinates."
    )]
    async fn input_scroll(
        &self,
        Parameters(p): Parameters<ScrollParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("input.scroll", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Move the mouse to the given coordinates.
    #[tool(description = "Move the mouse pointer to the given screen coordinates.")]
    async fn input_move(
        &self,
        Parameters(p): Parameters<MoveParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("input.move", serde_json::to_value(&p).unwrap())
            .await
    }

    /// List all visible top-level windows.
    #[tool(description = "List all top-level windows visible to the OS.")]
    async fn windows_list(
        &self,
        Parameters(_): Parameters<ListWindowsParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("windows.list", Value::Null).await
    }

    /// Bring a window to the foreground.
    #[tool(
        description = "Bring a window to the foreground by its platform handle (hwnd / xcb_window_t / NSWindow pointer)."
    )]
    async fn windows_focus(
        &self,
        Parameters(p): Parameters<FocusWindowParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("windows.focus", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Find a window by title substring and/or owner PID.
    #[tool(description = "Find a single window matching a title substring and/or owner PID.")]
    async fn windows_find(
        &self,
        Parameters(p): Parameters<FindWindowParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("windows.find", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Launch a new process. Returns its PID.
    #[tool(description = "Launch a new process. Returns the child PID.")]
    async fn process_launch(
        &self,
        Parameters(p): Parameters<LaunchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("process.launch", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Terminate a running process.
    #[tool(description = "Terminate a running process by its PID.")]
    async fn process_kill(
        &self,
        Parameters(p): Parameters<KillParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("process.kill", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Query whether a process is still running.
    #[tool(
        description = "Query whether a process is still running and its exit code (if terminated)."
    )]
    async fn process_status(
        &self,
        Parameters(p): Parameters<StatusParams>,
    ) -> Result<CallToolResult, ErrorData> {
        self.call("process.status", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Compute the fraction of pixels that differ between two PNG images.
    #[tool(
        description = "Compute the fraction of pixels that differ between two base64-encoded PNGs."
    )]
    async fn analysis_diff(
        &self,
        Parameters(p): Parameters<DiffParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Validate the base64 up-front to give a clean error before the
        // dispatcher does its own decode.
        BASE64.decode(&p.image_a).map_err(|e| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("image_a base64: {e}"),
                None,
            )
        })?;
        BASE64.decode(&p.image_b).map_err(|e| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("image_b base64: {e}"),
                None,
            )
        })?;
        self.call("analysis.diff", serde_json::to_value(&p).unwrap())
            .await
    }

    /// Compute a BLAKE3 hash of the normalized pixel data of a PNG.
    #[tool(description = "Compute a BLAKE3 perceptual hash of a base64-encoded PNG.")]
    async fn analysis_hash(
        &self,
        Parameters(p): Parameters<HashParams>,
    ) -> Result<CallToolResult, ErrorData> {
        BASE64.decode(&p.image).map_err(|e| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("image base64: {e}"),
                None,
            )
        })?;
        self.call("analysis.hash", serde_json::to_value(&p).unwrap())
            .await
    }
}

#[tool_handler]
impl ServerHandler for PlayCuaMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("playcua", env!("CARGO_PKG_VERSION"))
                    .with_title("playcua computer-use agent".to_string())
                    .with_website_url("https://github.com/KooshaPari/PlayCua".to_string()),
            )
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_instructions(
                "playcua is a computer-use agent for macOS, Linux, and Windows. \
                 14 tools are available for screen capture, keyboard/mouse input, \
                 window management, process lifecycle, and image analysis. \
                 For multi-step tasks, prefer the screenshot -> action -> screenshot loop."
                    .to_string(),
            )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_param_structs_serialize_to_expected_json_rpc_payloads() {
        // The MCP param structs must serialize to the same shape the existing
        // JSON-RPC dispatcher expects. Round-trip via serde_json to catch drift.
        let s = serde_json::to_value(ScreenshotParams {
            display: Some(1),
            window_title: None,
        })
        .unwrap();
        assert_eq!(s, json!({ "display": 1, "window_title": null }));

        let s = serde_json::to_value(ClickParams {
            x: 100,
            y: 200,
            button: Some("right".to_string()),
            action: Some("click".to_string()),
        })
        .unwrap();
        assert_eq!(
            s,
            json!({ "x": 100, "y": 200, "button": "right", "action": "click" })
        );
    }

    #[test]
    fn test_diff_params_base64_validation_catches_garbage() {
        let p = DiffParams {
            image_a: "not!valid!base64".to_string(),
            image_b: BASE64.encode([0u8, 1, 2, 3]),
            threshold: None,
        };
        assert!(BASE64.decode(&p.image_a).is_err());
        assert!(BASE64.decode(&p.image_b).is_ok());
    }
}
