//! `playcua-bridge` — guest-side stdio NDJSON JSON-RPC 2.0 server.
//!
//! Sandbox modality's [`BridgeClient`](playcua_native::ipc::BridgeClient)
//! spawns this binary (or `PLAYCUA_BRIDGE_BIN`) and tunnels
//! `screenshot` / `input.*` / `windows.*` over piped stdin/stdout.
//! Screenshot, input, and windows use real guest-OS adapters (same as host);
//! set `PLAYCUA_BRIDGE_STUB_SCREENSHOT=1` / `PLAYCUA_BRIDGE_STUB_INPUT=1`
//! for hermetic stubs in CI.
//! Logging goes to stderr only so stdout stays wire-clean.
//!
//! Build: `cargo build --locked -p playcua-native --bin playcua-bridge`
//! Install for local PATH: `cargo install --path native --bin playcua-bridge --locked`

use playcua_native::ipc::bridge_server::handle_request;
use playcua_native::ipc::{read_request, write_response};
use tokio::io::{self, BufReader};
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // stderr only — stdout is the JSON-RPC wire.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "playcua-bridge starting (stdio NDJSON JSON-RPC 2.0)"
    );

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = io::stdout();

    loop {
        match read_request(&mut reader).await {
            Ok(Some(req)) => {
                let resp = handle_request(req).await;
                if let Err(e) = write_response(&mut stdout, &resp).await {
                    error!(error = %e, "failed to write JSON-RPC response");
                    break;
                }
            }
            Ok(None) => {
                // EOF or blank line — peer closed stdin.
                info!("playcua-bridge stdin closed; exiting");
                break;
            }
            Err(e) => {
                error!(error = %e, "failed to read JSON-RPC request");
                // Parse errors: try to emit a JSON-RPC parse error with null id.
                let resp = playcua_native::ipc::Response::err(
                    serde_json::Value::Null,
                    -32700,
                    format!("parse error: {e}"),
                );
                let _ = write_response(&mut stdout, &resp).await;
            }
        }
    }
}
