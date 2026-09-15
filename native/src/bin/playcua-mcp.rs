//! `playcua-mcp` — Model Context Protocol server for playcua.
//!
//! Exposes the 14 native IPC methods (`screenshot`, `input.*`, `windows.*`,
//! `process.*`, `analysis.*`) as MCP tools. The protocol server is selected
//! by `--transport {stdio,http}` (default: stdio for Claude Desktop /
//! Cursor / mcp-agent subprocess usage).
//!
//! Examples:
//!   playcua-mcp                                    # stdio transport
//!   playcua-mcp --transport http --bind 127.0.0.1 --port 3000
//!
//! Build:
//!   cargo build --bin playcua-mcp --features mcp-server --release

#![cfg(feature = "mcp-server")]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use pheno_flags::FlagSet;
use rmcp::transport::streamable_http_server::tower::StreamableHttpService;
use rmcp::ServiceExt;
use tracing::info;
use tracing_subscriber::EnvFilter;

use playcua_native::app::App;
use playcua_native::mcp_server::PlayCuaMcp;
use playcua_native::modality::registry::{ModalityEnv, ModalityRegistry};
use playcua_native::modality::ModalityKind;

#[derive(Parser, Debug)]
#[command(name = "playcua-mcp", version, about = "playcua as an MCP server")]
struct Args {
    /// Transport: "stdio" (default, for Claude/Cursor subprocess) or "http"
    /// (streamable HTTP, for multi-client server deployments).
    #[arg(long, default_value = "stdio", value_parser = ["stdio", "http"])]
    transport: String,

    /// Bind address for `transport=http` (ignored on stdio).
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// Bind port for `transport=http` (ignored on stdio).
    #[arg(long, default_value_t = 3000)]
    port: u16,

    /// Path prefix for HTTP MCP endpoint. Default: `/mcp`.
    #[arg(long, default_value = "/mcp")]
    path: String,

    /// Modality: native | sandbox | nvms | wsl | container. Overrides
    /// PLAYCUA_MODALITY env var; falls back to auto.
    #[arg(long, value_parser = ["native", "sandbox", "nvms", "wsl", "container"])]
    modality: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let args = Args::parse();
    info!(
        version = env!("CARGO_PKG_VERSION"),
        transport = %args.transport,
        "playcua-mcp starting"
    );

    // Select modality from flag > env > auto.
    let mut registry = ModalityRegistry::with_defaults();
    let flag = args.modality.as_deref().and_then(ModalityKind::parse);
    let env = ModalityEnv::from_process_env(flag);
    let selected = registry.select(&env).clone();
    info!(
        kind = %selected.kind,
        describe = %selected.describe,
        detail = %selected.detail,
        available = selected.available,
        "modality selected"
    );

    // Wire the app once; the Arc is shared across all transports / clients.
    let flag_set = FlagSet::from_env("PLAYCUA")?;
    let app = App::build(selected, &flag_set);
    let dispatcher = Arc::new(app.dispatcher);

    match args.transport.as_str() {
        "stdio" => serve_stdio(dispatcher).await,
        "http" => serve_http(dispatcher, &args).await,
        other => {
            anyhow::bail!("Unknown transport: {other}");
        }
    }
}

async fn serve_stdio(dispatcher: Arc<playcua_native::ipc::dispatcher::Dispatcher>) -> Result<()> {
    let server = PlayCuaMcp::new(dispatcher)
        .serve(rmcp::transport::stdio())
        .await
        .context("failed to start stdio MCP server")?;
    info!("stdio transport ready; waiting for client");
    server.waiting().await?;
    info!("stdio transport closed; exiting");
    Ok(())
}

async fn serve_http(
    dispatcher: Arc<playcua_native::ipc::dispatcher::Dispatcher>,
    args: &Args,
) -> Result<()> {
    let path = args.path.clone();
    let svc = StreamableHttpService::new(
        move || Ok(PlayCuaMcp::new(Arc::clone(&dispatcher))),
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default()
            .into(),
        rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false),
    );
    let router = axum::Router::new().nest_service(&path, svc);
    let addr: SocketAddr = format!("{}:{}", args.bind, args.port).parse()?;
    info!(%addr, path, "streamable HTTP transport ready");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::from_env("PLAYCUA_LOG")
                .add_directive("playcua_native=info".parse().unwrap())
                .add_directive("rmcp=warn".parse().unwrap()),
        )
        .json()
        .init();
}
