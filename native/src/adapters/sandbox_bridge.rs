//! Sandbox capture/input/window ports via stdio JSON-RPC to `playcua-bridge`.
//!
//! ADR-006 follow-up after M2 (#137): sandbox modality must not call native
//! host adapters for I/O. Instead these ports tunnel through
//! [`BridgeClient`] to a guest-side bridge (or a hermetic fake).
//!
//! The live child is preferably spawned by [`SandboxDriver`] /
//! [`crate::adapters::sandbox::WireSandboxAdapter`] into a shared slot so
//! ports do not rely only on ambient `$PATH`. Missing bridge binary → fail
//! loud with an actionable error (no silent native leak).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::adapters::sandbox::SharedBridgeSlot;
use crate::domain::capture::{CaptureError, Frame};
use crate::domain::input::{
    InputError, Key, KeyAction, MouseAction, MouseButton, MouseEvent, ScrollDirection,
};
use crate::domain::window::{WindowError, WindowFilter, WindowInfo};
use crate::ipc::bridge_client::{BridgeClient, BridgeError};
use crate::modality::sandbox::{SandboxDriver, SandboxModality};
use crate::ports::{CapturePort, InputPort, WindowPort};

/// Shared bridge handle used by the three sandbox I/O ports.
#[derive(Clone)]
pub struct SandboxBridgePorts {
    inner: Arc<SandboxBridgeInner>,
}

struct SandboxBridgeInner {
    /// Lazily-connected client. `None` until first I/O call.
    client: Mutex<Option<Arc<BridgeClient>>>,
    /// Pre-injected client for hermetic tests (skips binary resolve/spawn).
    injected: Option<Arc<BridgeClient>>,
    /// Optional slot published by [`WireSandboxAdapter`] / [`SandboxDriver`].
    shared: Option<SharedBridgeSlot>,
}

impl SandboxBridgePorts {
    /// Production: resolve + spawn `playcua-bridge` on first I/O call via
    /// [`SandboxDriver::spawn_bridge`] (fail-loud if missing).
    pub fn lazy_connect() -> Self {
        Self {
            inner: Arc::new(SandboxBridgeInner {
                client: Mutex::new(None),
                injected: None,
                shared: None,
            }),
        }
    }

    /// Prefer a bridge already published by [`WireSandboxAdapter`]; otherwise
    /// spawn via [`SandboxDriver`] into that shared slot.
    pub fn from_shared_slot(slot: SharedBridgeSlot) -> Self {
        Self {
            inner: Arc::new(SandboxBridgeInner {
                client: Mutex::new(None),
                injected: None,
                shared: Some(slot),
            }),
        }
    }

    /// Hermetic: use an already-connected [`BridgeClient`] (duplex or fake).
    pub fn with_client(client: Arc<BridgeClient>) -> Self {
        Self {
            inner: Arc::new(SandboxBridgeInner {
                client: Mutex::new(Some(Arc::clone(&client))),
                injected: Some(client),
                shared: None,
            }),
        }
    }

    /// Capture port trait object sharing this bridge.
    pub fn capture(&self) -> Arc<dyn CapturePort> {
        Arc::new(BridgeCapture {
            ports: self.clone(),
        })
    }

    /// Input port trait object sharing this bridge.
    pub fn input(&self) -> Arc<dyn InputPort> {
        Arc::new(BridgeInput {
            ports: self.clone(),
        })
    }

    /// Window port trait object sharing this bridge.
    pub fn windows(&self) -> Arc<dyn WindowPort> {
        Arc::new(BridgeWindow {
            ports: self.clone(),
        })
    }

    async fn client(&self) -> Result<Arc<BridgeClient>, BridgeError> {
        if let Some(ref c) = self.inner.injected {
            return Ok(Arc::clone(c));
        }
        // Prefer driver/adapter-published shared slot.
        if let Some(ref slot) = self.inner.shared {
            {
                let guard = slot.lock().await;
                if let Some(ref c) = *guard {
                    return Ok(Arc::clone(c));
                }
            }
            // Spawn via SandboxDriver into the shared slot (not ambient-only).
            let client = spawn_bridge_via_driver().await?;
            let mut guard = slot.lock().await;
            if let Some(ref existing) = *guard {
                return Ok(Arc::clone(existing));
            }
            *guard = Some(Arc::clone(&client));
            return Ok(client);
        }
        let mut guard = self.inner.client.lock().await;
        if let Some(ref c) = *guard {
            return Ok(Arc::clone(c));
        }
        let client = spawn_bridge_via_driver().await?;
        *guard = Some(Arc::clone(&client));
        Ok(client)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        let client = self.client().await?;
        client.call(method, params).await
    }
}

/// Canonical spawn path: [`SandboxDriver::spawn_bridge`].
///
/// Uses the probed sandbox backend when available; otherwise
/// [`SandboxBackend::Direct`] so hermetic I/O can spawn
/// `PLAYCUA_BRIDGE_BIN` / fake-playcua-bridge without a host wrapper.
async fn spawn_bridge_via_driver() -> Result<Arc<BridgeClient>, BridgeError> {
    use crate::modality::sandbox::SandboxBackend;
    let mut driver = SandboxDriver::driver_for_probe(&SandboxModality::new())
        .unwrap_or_else(|| SandboxDriver::new(SandboxBackend::Direct));
    driver.spawn_bridge().await
}

fn map_bridge(err: BridgeError) -> String {
    err.to_string()
}

struct BridgeCapture {
    ports: SandboxBridgePorts,
}

#[async_trait]
impl CapturePort for BridgeCapture {
    async fn capture_display(&self, monitor: u32) -> Result<Frame, CaptureError> {
        let result = self
            .ports
            .call("screenshot", json!({ "monitor": monitor }))
            .await
            .map_err(|e| CaptureError::CaptureFailed(map_bridge(e)))?;
        frame_from_result(result)
    }

    async fn capture_window(&self, title: Option<&str>) -> Result<Frame, CaptureError> {
        let params = match title {
            Some(t) => json!({ "window_title": t }),
            None => json!({}),
        };
        let result = self
            .ports
            .call("screenshot", params)
            .await
            .map_err(|e| CaptureError::CaptureFailed(map_bridge(e)))?;
        frame_from_result(result)
    }
}

fn frame_from_result(result: Value) -> Result<Frame, CaptureError> {
    let data = result
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CaptureError::CaptureFailed("bridge result missing data".into()))?
        .to_string();
    let width = result
        .get("width")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CaptureError::CaptureFailed("bridge result missing width".into()))?
        as u32;
    let height = result
        .get("height")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CaptureError::CaptureFailed("bridge result missing height".into()))?
        as u32;
    Ok(Frame {
        data,
        width,
        height,
    })
}

struct BridgeInput {
    ports: SandboxBridgePorts,
}

#[async_trait]
impl InputPort for BridgeInput {
    async fn key_event(&self, key: Key, action: KeyAction) -> Result<(), InputError> {
        let action = match action {
            KeyAction::Press => "press",
            KeyAction::Down => "down",
            KeyAction::Up => "up",
        };
        self.ports
            .call("input.key", json!({ "key": key.0, "action": action }))
            .await
            .map_err(|e| InputError::InjectionFailed(map_bridge(e)))?;
        Ok(())
    }

    async fn type_text(&self, text: &str) -> Result<(), InputError> {
        self.ports
            .call("input.type", json!({ "text": text }))
            .await
            .map_err(|e| InputError::InjectionFailed(map_bridge(e)))?;
        Ok(())
    }

    async fn mouse_event(&self, event: MouseEvent) -> Result<(), InputError> {
        let (method, params) = match event {
            MouseEvent::Move { x, y } => ("input.move", json!({ "x": x, "y": y })),
            MouseEvent::Click {
                x,
                y,
                button,
                action,
            } => {
                let button = match button {
                    MouseButton::Left => "left",
                    MouseButton::Right => "right",
                    MouseButton::Middle => "middle",
                };
                let action = match action {
                    MouseAction::Click => "click",
                    MouseAction::Down => "down",
                    MouseAction::Up => "up",
                };
                (
                    "input.click",
                    json!({ "x": x, "y": y, "button": button, "action": action }),
                )
            }
            MouseEvent::Scroll {
                x,
                y,
                direction,
                amount,
            } => {
                let direction = match direction {
                    ScrollDirection::Up => "up",
                    ScrollDirection::Down => "down",
                    ScrollDirection::Left => "left",
                    ScrollDirection::Right => "right",
                };
                (
                    "input.scroll",
                    json!({ "x": x, "y": y, "direction": direction, "amount": amount }),
                )
            }
        };
        self.ports
            .call(method, params)
            .await
            .map_err(|e| InputError::InjectionFailed(map_bridge(e)))?;
        Ok(())
    }
}

struct BridgeWindow {
    ports: SandboxBridgePorts,
}

#[async_trait]
impl WindowPort for BridgeWindow {
    async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError> {
        let result = self
            .ports
            .call("windows.list", Value::Null)
            .await
            .map_err(|e| WindowError::Failed(map_bridge(e)))?;
        serde_json::from_value(result)
            .map_err(|e| WindowError::Failed(format!("bridge windows.list decode: {e}")))
    }

    async fn find_window(&self, filter: WindowFilter) -> Result<Option<WindowInfo>, WindowError> {
        let mut params = serde_json::Map::new();
        if let Some(title) = filter.title {
            params.insert("title".into(), json!(title));
        }
        if let Some(pid) = filter.pid {
            params.insert("pid".into(), json!(pid));
        }
        let result = self
            .ports
            .call("windows.find", Value::Object(params))
            .await
            .map_err(|e| WindowError::Failed(map_bridge(e)))?;
        if result.is_null() {
            return Ok(None);
        }
        let info: WindowInfo = serde_json::from_value(result)
            .map_err(|e| WindowError::Failed(format!("bridge windows.find decode: {e}")))?;
        Ok(Some(info))
    }

    async fn focus_window(&self, hwnd: usize) -> Result<(), WindowError> {
        self.ports
            .call("windows.focus", json!({ "hwnd": hwnd }))
            .await
            .map_err(|e| WindowError::Failed(map_bridge(e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::mod_types::{read_request, write_response, Response};
    use tokio::io::BufReader;

    async fn serve_one(
        peer: &mut tokio::io::DuplexStream,
        handler: impl FnOnce(crate::ipc::mod_types::Request) -> Response,
    ) {
        let mut reader = BufReader::new(&mut *peer);
        let req = read_request(&mut reader).await.expect("read").expect("eof");
        let resp = handler(req);
        write_response(peer, &resp).await.expect("write");
    }

    #[tokio::test]
    async fn bridge_capture_display_via_duplex() {
        let (client, mut peer) = BridgeClient::duplex_pair(64 * 1024);
        let ports = SandboxBridgePorts::with_client(Arc::new(client));
        let capture = ports.capture();
        let server = tokio::spawn(async move {
            serve_one(&mut peer, |req| {
                assert_eq!(req.method, "screenshot");
                Response::ok(
                    req.id,
                    json!({
                        "data": "ZmFrZQ==",
                        "width": 2,
                        "height": 1,
                        "format": "png",
                    }),
                )
            })
            .await;
        });
        let frame = capture.capture_display(0).await.expect("capture");
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 1);
        assert_eq!(frame.data, "ZmFrZQ==");
        server.await.expect("server");
    }

    #[tokio::test]
    async fn bridge_input_and_windows_via_duplex() {
        let (client, mut peer) = BridgeClient::duplex_pair(64 * 1024);
        let ports = SandboxBridgePorts::with_client(Arc::new(client));
        let input = ports.input();
        let windows = ports.windows();

        let server = tokio::spawn(async move {
            serve_one(&mut peer, |req| {
                assert_eq!(req.method, "input.type");
                Response::ok(req.id, json!({ "ok": true }))
            })
            .await;
            serve_one(&mut peer, |req| {
                assert_eq!(req.method, "windows.list");
                Response::ok(
                    req.id,
                    json!([{
                        "hwnd": 42,
                        "title": "Sandbox",
                        "pid": 7,
                        "x": 0, "y": 0, "width": 100, "height": 50,
                        "visible": true
                    }]),
                )
            })
            .await;
        });

        input.type_text("hi").await.expect("type");
        let list = windows.list_windows().await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].hwnd, 42);
        server.await.expect("server");
    }

    #[tokio::test]
    async fn missing_bridge_fails_loud_no_native() {
        let _bguard = crate::ipc::bridge_client::BRIDGE_ENV_LOCK.lock().await;
        let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
        std::env::set_var("PLAYCUA_BRIDGE_BIN", "/nonexistent/no-bridge");
        let ports = SandboxBridgePorts::lazy_connect();
        let err = ports
            .capture()
            .capture_display(0)
            .await
            .expect_err("must fail loud");
        let msg = err.to_string();
        assert!(
            msg.contains("playcua-bridge") || msg.contains("PLAYCUA_BRIDGE_BIN"),
            "unexpected: {msg}"
        );
        match prev {
            Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
            None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
        }
    }
}
