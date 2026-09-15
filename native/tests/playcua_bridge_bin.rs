//! Integration: spawn the real `playcua-bridge` binary via BridgeClient.
//!
//! FR-sandbox-bridge: host BridgeClient ↔ guest playcua-bridge NDJSON
//! JSON-RPC for screenshot / input.* / windows.*. Keeps the Python
//! fake fixture as the hermetic CI default; this test proves the
//! production binary speaks the same wire protocol.

use std::path::PathBuf;
use std::sync::Arc;

use playcua_native::adapters::sandbox_bridge::SandboxBridgePorts;
use playcua_native::domain::input::{Key, KeyAction, MouseAction, MouseButton, MouseEvent};
use playcua_native::domain::window::WindowFilter;
use playcua_native::ipc::bridge_client::{BridgeClient, BRIDGE_ENV_LOCK};

fn bridge_bin() -> PathBuf {
    // cargo sets CARGO_BIN_EXE_<name> for integration tests when the
    // package defines that [[bin]].
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_playcua-bridge") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return path;
        }
    }
    // Fallback: target/<profile>/playcua-bridge relative to CARGO_MANIFEST_DIR.
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let mut candidates = vec![];
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(
            PathBuf::from(&m)
                .join("../target")
                .join(&profile)
                .join("playcua-bridge"),
        );
        candidates.push(
            PathBuf::from(&m)
                .join("target")
                .join(&profile)
                .join("playcua-bridge"),
        );
    }
    candidates.into_iter().find(|p| p.is_file()).expect(
        "playcua-bridge binary missing — run \
             `cargo build --locked -p playcua-native --bin playcua-bridge` first, \
             or invoke via `cargo test --locked -p playcua-native`",
    )
}

#[tokio::test]
async fn real_bridge_bin_screenshot_input_windows() {
    let _guard = BRIDGE_ENV_LOCK.lock().await;
    // Hermetic screenshot/input for CI (avoid real OS injection); windows
    // use real guest adapters.
    let prev_stub = std::env::var("PLAYCUA_BRIDGE_STUB_SCREENSHOT").ok();
    let prev_input = std::env::var("PLAYCUA_BRIDGE_STUB_INPUT").ok();
    std::env::set_var("PLAYCUA_BRIDGE_STUB_SCREENSHOT", "1");
    std::env::set_var("PLAYCUA_BRIDGE_STUB_INPUT", "1");

    let bin = bridge_bin();
    let client = BridgeClient::spawn(&bin, &[])
        .await
        .expect("spawn playcua-bridge");

    let ping = client
        .call("ping", serde_json::Value::Null)
        .await
        .expect("ping");
    assert_eq!(ping["screenshot"], "stub");
    assert_eq!(ping["input"], "stub");
    assert_eq!(ping["windows"], "real");

    let ports = SandboxBridgePorts::with_client(Arc::new(client));

    let frame = ports
        .capture()
        .capture_display(0)
        .await
        .expect("screenshot via real bridge");
    assert_eq!(frame.width, 1);
    assert_eq!(frame.height, 1);
    assert!(!frame.data.is_empty());

    ports
        .input()
        .key_event(Key::new("a"), KeyAction::Press)
        .await
        .expect("input.key");
    ports.input().type_text("hello").await.expect("input.type");
    ports
        .input()
        .mouse_event(MouseEvent::Click {
            x: 10,
            y: 20,
            button: MouseButton::Left,
            action: MouseAction::Click,
        })
        .await
        .expect("input.click");

    let wins = ports.windows().list_windows().await.expect("windows.list");
    // Real enumeration — length is host-dependent; must be a successful Vec.
    let _ = wins;

    let found = ports
        .windows()
        .find_window(WindowFilter {
            title: Some("___no_such_playcua_win___".into()),
            pid: None,
        })
        .await
        .expect("windows.find");
    assert!(found.is_none());

    ports
        .windows()
        .focus_window(1)
        .await
        .expect("windows.focus");

    match prev_stub {
        Some(v) => std::env::set_var("PLAYCUA_BRIDGE_STUB_SCREENSHOT", v),
        None => std::env::remove_var("PLAYCUA_BRIDGE_STUB_SCREENSHOT"),
    }
    match prev_input {
        Some(v) => std::env::set_var("PLAYCUA_BRIDGE_STUB_INPUT", v),
        None => std::env::remove_var("PLAYCUA_BRIDGE_STUB_INPUT"),
    }
}

#[tokio::test]
async fn real_bridge_unknown_method_fails_loud() {
    let bin = bridge_bin();
    let client = BridgeClient::spawn(&bin, &[])
        .await
        .expect("spawn playcua-bridge");
    let err = client
        .call("process.launch", serde_json::json!({ "path": "/bin/true" }))
        .await
        .expect_err("unknown method must fail");
    match err {
        playcua_native::ipc::BridgeError::Rpc { code, message } => {
            assert_eq!(code, -32601);
            assert!(message.contains("process.launch"), "{message}");
        }
        other => panic!("expected Rpc error, got {other:?}"),
    }
}
