//! Hermetic sandbox JSON-RPC bridge — spawn fake-playcua-bridge over stdio.
//!
//! ADR-006 follow-up: capture/input/window under sandbox modality must talk
//! NDJSON JSON-RPC to playcua-bridge (never native host). This test points
//! `PLAYCUA_BRIDGE_BIN` at the fixture and exercises SandboxDriver spawn
//! plus the shared-slot I/O path.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use playcua_native::adapters::sandbox::WireSandboxAdapter;
use playcua_native::adapters::sandbox_bridge::SandboxBridgePorts;
use playcua_native::domain::input::{Key, KeyAction, MouseAction, MouseButton, MouseEvent};
use playcua_native::domain::window::WindowFilter;
use playcua_native::ipc::bridge_client::BridgeClient;
use playcua_native::ipc::BRIDGE_ENV_LOCK;
use playcua_native::modality::sandbox::{SandboxBackend, SandboxDriver};

fn fixture_bridge() -> PathBuf {
    let fixture = if cfg!(windows) {
        "fake-playcua-bridge.cmd"
    } else {
        "fake-playcua-bridge.sh"
    };
    let mut candidates = vec![];
    if let Ok(m) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(m).join("tests/fixtures").join(fixture));
    }
    candidates.push(PathBuf::from("native/tests/fixtures").join(fixture));
    candidates.push(PathBuf::from("tests/fixtures").join(fixture));
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .expect("platform fake-playcua-bridge fixture must exist")
}

fn chmod_bridge(_bin: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(_bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(_bin, perms).ok();
    }
}

#[tokio::test]
async fn fake_bridge_spawn_capture_input_windows() {
    let bin = fixture_bridge();
    chmod_bridge(&bin);

    let client = BridgeClient::spawn(&bin, &[])
        .await
        .expect("spawn fake bridge");
    let ports = SandboxBridgePorts::with_client(Arc::new(client));

    let frame = ports
        .capture()
        .capture_display(0)
        .await
        .expect("screenshot via bridge");
    assert_eq!(frame.width, 8);
    assert_eq!(frame.height, 4);
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
    assert_eq!(wins.len(), 1);
    assert_eq!(wins[0].title, "FakeSandboxWindow");

    let found = ports
        .windows()
        .find_window(WindowFilter {
            title: Some("Fake".into()),
            pid: None,
        })
        .await
        .expect("windows.find");
    assert!(found.is_some());

    ports
        .windows()
        .focus_window(1)
        .await
        .expect("windows.focus");
}

#[tokio::test]
async fn lazy_connect_uses_playcua_bridge_bin_env() {
    let _guard = BRIDGE_ENV_LOCK.lock().await;
    let bin = fixture_bridge();
    chmod_bridge(&bin);
    let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
    std::env::set_var("PLAYCUA_BRIDGE_BIN", &bin);

    let ports = SandboxBridgePorts::lazy_connect();
    let frame = ports
        .capture()
        .capture_window(Some("Fake"))
        .await
        .expect("lazy spawn via PLAYCUA_BRIDGE_BIN");
    assert_eq!(frame.width, 8);

    match prev {
        Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
        None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
    }
}

#[tokio::test]
async fn shared_slot_uses_driver_spawned_bridge() {
    let _guard = BRIDGE_ENV_LOCK.lock().await;
    let bin = fixture_bridge();
    chmod_bridge(&bin);
    let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
    let prev_backend = std::env::var("PLAYCUA_SANDBOX_BACKEND").ok();
    std::env::set_var("PLAYCUA_BRIDGE_BIN", &bin);
    std::env::set_var("PLAYCUA_SANDBOX_BACKEND", "direct");

    let adapter = WireSandboxAdapter::new();
    let ports = SandboxBridgePorts::from_shared_slot(adapter.bridge_slot());
    // First I/O call → SandboxDriver::spawn_bridge into shared slot.
    let frame = ports
        .capture()
        .capture_display(0)
        .await
        .expect("shared-slot driver spawn");
    assert_eq!(frame.width, 8);
    assert!(
        adapter.bridge_slot().lock().await.is_some(),
        "adapter slot must hold live bridge"
    );

    match prev {
        Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
        None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
    }
    match prev_backend {
        Some(v) => std::env::set_var("PLAYCUA_SANDBOX_BACKEND", v),
        None => std::env::remove_var("PLAYCUA_SANDBOX_BACKEND"),
    }
}

#[tokio::test]
async fn driver_spawn_bridge_then_ports() {
    let _guard = BRIDGE_ENV_LOCK.lock().await;
    let bin = fixture_bridge();
    chmod_bridge(&bin);
    let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
    std::env::set_var("PLAYCUA_BRIDGE_BIN", &bin);

    let mut driver = SandboxDriver::new(SandboxBackend::Direct);
    let client = driver.spawn_bridge().await.expect("driver spawn_bridge");
    let ports = SandboxBridgePorts::with_client(client);
    let frame = ports
        .capture()
        .capture_display(0)
        .await
        .expect("ports via driver bridge");
    assert_eq!(frame.width, 8);
    driver.shutdown().await.expect("shutdown");

    match prev {
        Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
        None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
    }
}
