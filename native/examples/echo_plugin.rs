//! `echo_plugin` — full worked example of a third-party `MethodPlugin`.
//!
//! This binary demonstrates the four steps from `docs/skill-sdk.md`:
//!
//! 1. Define a struct that holds plugin state (here: a struct with no
//!    state, but the pattern is `Arc<Mutex<T>>` for shared state).
//! 2. Implement `playcua_native::plugins::TypedMethodPlugin` for it.
//! 3. Register the plugin in a `PluginRegistry`.
//! 4. Drive a JSON-RPC 2.0 loop that consults the registry for any
//!    method not handled by a built-in.
//!
//! ## Running
//!
//! ```bash
//! cargo run --example echo_plugin
//! ```
//!
//! In another terminal, talk to it the same way you'd talk to
//! `playcua-native`:
//!
//! ```bash
//! $ echo '{"jsonrpc":"2.0","id":1,"method":"ping"}' | cargo run --example echo_plugin
//! # ... daemon logs to stderr, response goes to stdout:
//! {"jsonrpc":"2.0","id":1,"result":{...}}
//!
//! $ echo '{"jsonrpc":"2.0","id":2,"method":"acme.echo","params":{"hello":"world"}}' \
//!     | cargo run --example echo_plugin
//! {"jsonrpc":"2.0","id":2,"result":{"hello":"world"}}
//! ```
//!
//! ## What this proves
//!
//! - Plugins can live entirely outside the daemon crate
//! - The same raw `MethodPlugin` trait is consumed by the binary's own
//!   dispatcher, while plugin authors can stay on typed wrappers
//!   dispatcher, with no monkey-patching or trait-object casting
//! - The JSON-RPC 2.0 wire format is identical to `playcua-native`
//! - Testability: the plugin can be unit-tested without any I/O

use std::sync::Arc;

use async_trait::async_trait;
use playcua_native::ipc::{read_request, write_response, Response};
use playcua_native::plugins::{PluginRegistry, TypedMethodPlugin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncWriteExt, BufReader};

/// Step 1: the plugin struct.
///
/// In real skills this would carry `Arc<MySharedState>` so the plugin
/// can hold resources (DB pools, browser sessions, etc.) across calls.
pub struct EchoPlugin {
    /// Echoes back whatever you give it, prefixed with this tag.
    tag: &'static str,
}

impl EchoPlugin {
    pub const fn new(tag: &'static str) -> Self {
        Self { tag }
    }
}

/// Step 2: implement the trait.
#[derive(Debug, Deserialize, Serialize)]
pub struct EchoParams {
    #[serde(default)]
    hello: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EchoResult {
    echoed: EchoParams,
    tag: &'static str,
    len: usize,
}

#[async_trait]
impl TypedMethodPlugin for EchoPlugin {
    fn method_name(&self) -> &'static str {
        "acme.echo"
    }

    type Params = EchoParams;
    type Output = EchoResult;

    async fn handle_typed(&self, params: Self::Params) -> anyhow::Result<Self::Output> {
        // Tiny bit of work: wrap the params in a {"echoed": ..., "tag": ...}
        // envelope so callers can tell which plugin answered.
        Ok(EchoResult {
            len: serde_json::to_string(&params)?.len(),
            echoed: params,
            tag: self.tag,
        })
    }
}

/// A second plugin, just to prove the registry holds N.
pub struct ReversePlugin;

#[async_trait]
impl TypedMethodPlugin for ReversePlugin {
    fn method_name(&self) -> &'static str {
        "acme.reverse"
    }

    type Params = String;
    type Output = Value;

    async fn handle_typed(&self, params: Self::Params) -> anyhow::Result<Self::Output> {
        let s = params;
        let reversed: String = s.chars().rev().collect();
        Ok(json!({ "reversed": reversed }))
    }
}

/// Build the registry with the example plugins pre-registered.
fn build_registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Box::new(EchoPlugin::new("example-1")));
    r.register(Box::new(ReversePlugin));
    r
}

/// Step 3+4: drive a minimal JSON-RPC 2.0 stdio loop.
///
/// This is a stripped-down version of `playcua_native::run_stdio` —
/// in a real binary you'd wire the registry into the full dispatcher
/// (which also routes to the platform adapters). For demonstrating the
/// plugin path, a no-port loop is enough.
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    // Tracing to stderr so the JSON-RPC wire on stdout is clean.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("echo_plugin=info".parse().unwrap()),
        )
        .init();

    let registry = Arc::new(build_registry());
    tracing::info!(plugins = registry.len(), "echo_plugin example starting");

    let mut reader = BufReader::new(tokio::io::stdin());
    let mut writer = tokio::io::BufWriter::new(tokio::io::stdout());

    loop {
        let req = match read_request(&mut reader).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                tracing::info!("stdin EOF — shutting down");
                break;
            }
            Err(e) => {
                tracing::warn!(error = %e, "parse error");
                let resp = Response::err(Value::Null, -32700, format!("Parse error: {e}"));
                let _ = write_response(&mut writer, &resp).await;
                continue;
            }
        };

        let id = req.id.clone();
        let method = req.method.clone();

        // Routing: built-in "ping" first, then consult the plugin registry.
        let resp = if method == "ping" {
            Response::ok(id, json!({ "ok": true, "plugins": registry.len() }))
        } else if let Some(plugin) = registry.find(&method) {
            let params = req.params.clone().unwrap_or(Value::Null);
            match plugin.handle(params).await {
                Ok(v) => Response::ok(id, v),
                Err(e) => Response::err(id, -32603, format!("Plugin error: {e}")),
            }
        } else {
            Response::err(
                id,
                -32601,
                format!(
                    "Method not found: {method} (registered: {})",
                    registry.len()
                ),
            )
        };

        if let Err(e) = write_response(&mut writer, &resp).await {
            tracing::warn!(error = %e, "write error");
            break;
        }
        writer.flush().await.ok();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_plugin_round_trips_params() {
        let p = EchoPlugin::new("test");
        let out = p.handle(json!({ "hello": "world" })).await.unwrap();
        assert_eq!(out["echoed"]["hello"], "world");
        assert_eq!(out["tag"], "test");
    }

    #[tokio::test]
    async fn reverse_plugin_reverses_string() {
        let p = ReversePlugin;
        let out = p.handle(json!("hello")).await.unwrap();
        assert_eq!(out["reversed"], "olleh");
    }

    #[test]
    fn registry_finds_both_plugins() {
        let r = build_registry();
        assert_eq!(r.len(), 2);
        assert!(r.find("acme.echo").is_some());
        assert!(r.find("acme.reverse").is_some());
        assert!(r.find("nope").is_none());
    }
}
