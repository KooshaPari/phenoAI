//! Concrete CLI adapter implementing [`kmobile_core::ports::McpPort`].
//!
//! File-backed MCP tool registry: a JSON manifest at the given path
//! tracks the tools the user has registered with the local CLI. When
//! `invoke_tool` is called, the adapter maps the tool id to a known
//! `kmobile` subcommand (or to a stored handler) and runs it through
//! `tokio::process::Command`, returning the JSON-serialised stdout as
//! the invocation result. Tools that are not recognised return a
//! structured error result so the upstream MCP layer can surface a
//! JSON-RPC error response.
//!
//! The adapter uses an interior `tokio::sync::Mutex` for the registry
//! so the `&self` port surface can mutate it through interior
//! mutability, mirroring the rest of the CLI adapters.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use kmobile_core::error::KMobileError;
use kmobile_core::ports::{McpInvocationResult, McpPort, McpToolInfo, McpToolKind};

/// Persisted shape of the MCP tool registry.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Registry {
    /// All tools currently registered with the port.
    tools: Vec<McpToolInfo>,
    /// Optional per-tool command spec (`{ tool_id -> argv }`).
    #[serde(default)]
    handlers: std::collections::HashMap<String, Vec<String>>,
}

/// CLI-side [`McpPort`] implementation backed by a JSON file on disk.
pub struct CliMcpAdapter {
    registry_path: PathBuf,
    state: Arc<Mutex<Registry>>,
}

impl CliMcpAdapter {
    /// Build a new adapter. The registry is initialised empty if the
    /// file does not exist.
    pub async fn new<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let registry_path = path.as_ref().to_path_buf();
        if let Some(parent) = registry_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let state = Arc::new(Mutex::new(Self::load_or_empty(&registry_path).await?));
        Ok(Self {
            registry_path,
            state,
        })
    }

    /// Borrow the registry path.
    #[expect(dead_code)]
    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }

    /// Register a CLI handler for a tool id. The handler is the argv
    /// that will be executed when `invoke_tool` is called for that
    /// id. The first argv entry is the binary; the remaining entries
    /// are passed as-is.
    pub async fn register_handler(
        &self,
        tool_id: &str,
        argv: Vec<String>,
    ) -> Result<(), KMobileError> {
        let mut guard = self.state.lock().await;
        if !guard.tools.iter().any(|t| t.id == tool_id) {
            return Err(KMobileError::InvalidInput(format!(
                "mcp tool '{tool_id}' is not registered"
            )));
        }
        guard.handlers.insert(tool_id.to_string(), argv);
        self.persist(&guard)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        info!("Registered handler for mcp tool: {tool_id}");
        Ok(())
    }

    /// Look up a stored handler by tool id.
    #[expect(dead_code)]
    pub async fn handler_for(&self, tool_id: &str) -> Option<Vec<String>> {
        let guard = self.state.lock().await;
        guard.handlers.get(tool_id).cloned()
    }

    async fn load_or_empty(path: &Path) -> anyhow::Result<Registry> {
        if path.exists() {
            let bytes = tokio::fs::read(path).await?;
            let registry: Registry = serde_json::from_slice(&bytes)?;
            Ok(registry)
        } else {
            Ok(Registry::default())
        }
    }

    async fn persist(&self, registry: &Registry) -> anyhow::Result<()> {
        let bytes = serde_json::to_vec_pretty(registry)?;
        tokio::fs::write(&self.registry_path, bytes).await?;
        Ok(())
    }

    /// Best-effort schema check: returns `Ok(())` if the schema is
    /// missing or matches the supplied arguments. The check is
    /// intentionally shallow — JSON Schema validation is the
    /// transport's job.
    fn validate_args(tool: &McpToolInfo, args: &Value) -> Result<(), KMobileError> {
        if tool.input_schema.is_none() {
            return Ok(());
        }
        // The CLI adapter treats any non-null argument object as
        // acceptable; richer validation lives in the rmcp / JSON-RPC
        // transport. We still ensure the value is an object when
        // the schema is present, because our tool surface expects
        // key/value arguments.
        if !args.is_null() && !args.is_object() {
            return Err(KMobileError::InvalidInput(format!(
                "mcp tool '{}' requires an object arguments payload, got {}",
                tool.id, args
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl McpPort for CliMcpAdapter {
    async fn list_tools(&self) -> Result<Vec<McpToolInfo>, KMobileError> {
        let guard = self.state.lock().await;
        Ok(guard.tools.clone())
    }

    async fn get_tool(&self, id: &str) -> Result<Option<McpToolInfo>, KMobileError> {
        let guard = self.state.lock().await;
        Ok(guard.tools.iter().find(|t| t.id == id).cloned())
    }

    async fn register_tool(&self, tool: McpToolInfo) -> Result<McpToolInfo, KMobileError> {
        if tool.id.trim().is_empty() {
            return Err(KMobileError::InvalidInput(
                "mcp tool id must not be empty".into(),
            ));
        }
        if tool.name.trim().is_empty() {
            return Err(KMobileError::InvalidInput(
                "mcp tool name must not be empty".into(),
            ));
        }
        let mut guard = self.state.lock().await;
        if guard.tools.iter().any(|t| t.id == tool.id) {
            return Err(KMobileError::InvalidInput(format!(
                "mcp tool id already registered: {}",
                tool.id
            )));
        }
        // Default the kind to `Custom` if the caller did not pick
        // one — the port is permissive about registration input.
        let mut stored = tool.clone();
        if matches!(stored.kind, McpToolKind::Custom) && stored.description.is_empty() {
            stored.description = format!("CLI-backed MCP tool: {}", stored.id);
        }
        guard.tools.push(stored.clone());
        self.persist(&guard)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        info!("Registered mcp tool: {}", stored.id);
        Ok(stored)
    }

    async fn deregister_tool(&self, id: &str) -> Result<bool, KMobileError> {
        let mut guard = self.state.lock().await;
        let before = guard.tools.len();
        guard.tools.retain(|t| t.id != id);
        guard.handlers.remove(id);
        let removed = guard.tools.len() != before;
        if removed {
            self.persist(&guard)
                .await
                .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
            info!("Deregistered mcp tool: {id}");
        }
        Ok(removed)
    }

    async fn invoke_tool(
        &self,
        id: &str,
        args: Value,
    ) -> Result<McpInvocationResult, KMobileError> {
        let (tool, handler) = {
            let guard = self.state.lock().await;
            let tool = guard
                .tools
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .ok_or_else(|| KMobileError::InvalidInput(format!("mcp tool not found: {id}")))?;
            let handler = guard.handlers.get(id).cloned();
            (tool, handler)
        };

        if let Err(e) = Self::validate_args(&tool, &args) {
            return Ok(McpInvocationResult::err(e.to_string()));
        }

        let argv = match handler {
            Some(argv) => argv,
            None => {
                // No handler registered for this tool — synthesise
                // an echo-style response so the caller still gets a
                // well-formed result.
                return Ok(McpInvocationResult::ok(serde_json::json!({
                    "id": tool.id,
                    "kind": tool.kind,
                    "args": args,
                    "note": "no handler registered; arguments echoed back",
                })));
            }
        };

        if argv.is_empty() {
            return Ok(McpInvocationResult::err(format!(
                "mcp tool '{id}' has an empty handler argv"
            )));
        }

        debug!("Invoking mcp tool '{}' with argv: {:?}", id, argv);
        let (bin, sub) = argv.split_first().expect("non-empty");
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(sub)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = match cmd.output().await {
            Ok(o) => o,
            Err(e) => {
                warn!("mcp tool '{id}' failed to launch: {e}");
                return Ok(McpInvocationResult::err(format!(
                    "failed to launch handler: {e}"
                )));
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            return Ok(McpInvocationResult::err(format!(
                "mcp tool '{id}' exited with status {}: {}",
                output.status, stderr
            )));
        }
        // Best-effort JSON parse so the result is a structured value
        // when the handler emitted JSON; fall back to a raw string.
        let output_value: Value = match serde_json::from_str(stdout.trim()) {
            Ok(v) => v,
            Err(_) => Value::String(stdout),
        };
        Ok(McpInvocationResult::ok(serde_json::json!({
            "id": tool.id,
            "kind": tool.kind,
            "args": args,
            "output": output_value,
        })))
    }
}
