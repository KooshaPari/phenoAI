//! Hermetic SandboxDriver spawn — Direct backend guest + bridge alongside.
//!
//! FR-006 / ADR-006: `PLAYCUA_SANDBOX_BACKEND=direct` must spawn a real
//! guest, and `SandboxDriver::spawn_bridge` must start `PLAYCUA_BRIDGE_BIN`
//! (fake-playcua-bridge) as a sibling JSON-RPC child — no native host I/O.

use std::path::PathBuf;

use playcua_native::ipc::BRIDGE_ENV_LOCK;
use playcua_native::modality::sandbox::{SandboxBackend, SandboxDriver};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

#[tokio::test]
async fn direct_driver_spawn_tunnel_and_shutdown() {
    let mut driver = SandboxDriver::new(SandboxBackend::Direct);
    #[cfg(unix)]
    {
        driver
            .spawn_guest("cat", &[])
            .await
            .expect("direct cat spawn");
    }
    #[cfg(windows)]
    {
        driver
            .spawn_guest("cmd", &["/Q".into(), "/K".into(), "more".into()])
            .await
            .expect("direct more spawn");
    }

    let pid = driver.child_id().expect("pid");
    assert!(pid > 0);

    let mut stdin = driver.tunnel_stdin().expect("stdin");
    let mut stdout = driver.tunnel_stdout().expect("stdout");

    stdin.write_all(b"ping\n").await.expect("write");
    stdin.flush().await.expect("flush");
    // Close stdin so `cat`/`more` can finish echoing on some platforms.
    drop(stdin);

    let mut buf = vec![0u8; 4];
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stdout.read_exact(&mut buf),
    )
    .await
    .expect("read timed out")
    .expect("read");
    assert_eq!(n, 4);
    assert_eq!(&buf, b"ping");

    driver.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn direct_driver_spawn_bridge_alongside_guest() {
    let _guard = BRIDGE_ENV_LOCK.lock().await;
    let bin = fixture_bridge();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&bin).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&bin, perms).ok();
    }
    let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
    std::env::set_var("PLAYCUA_BRIDGE_BIN", &bin);

    let mut driver = SandboxDriver::new(SandboxBackend::Direct);
    #[cfg(unix)]
    let client = driver
        .spawn_guest_with_bridge("sleep", &["30".into()])
        .await
        .expect("guest+bridge");
    #[cfg(windows)]
    let client = driver
        .spawn_guest_with_bridge("cmd", &["/C".into(), "ping -n 30 127.0.0.1 >NUL".into()])
        .await
        .expect("guest+bridge");

    assert!(driver.child_id().is_some());
    assert!(driver.bridge_client().is_some());
    let pong = client
        .call("ping", serde_json::Value::Null)
        .await
        .expect("bridge ping");
    assert_eq!(pong["ok"], true);

    driver.shutdown().await.expect("shutdown");
    match prev {
        Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
        None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
    }
}

#[tokio::test]
async fn spawn_bridge_fails_loud_when_missing() {
    let _guard = BRIDGE_ENV_LOCK.lock().await;
    let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
    std::env::set_var("PLAYCUA_BRIDGE_BIN", "/nonexistent/playcua-bridge");
    let mut driver = SandboxDriver::new(SandboxBackend::Direct);
    let err = match driver.spawn_bridge().await {
        Ok(_) => panic!("must fail loud when bridge binary missing"),
        Err(e) => e,
    };
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
