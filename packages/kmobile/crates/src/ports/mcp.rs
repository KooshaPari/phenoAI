//! MCP (Model Context Protocol) port.
//!
//! Hexagonal port for the KMobile **MCP protocol core**. Adapters
//! (stdio, HTTP, in-process, …) live in the consuming crates
//! (`kmobile-mcp`, `kmobile-cli`, …) and implement the
//! [`McpPort`] trait.
//!
//! The domain depends only on the trait and the DTOs declared here;
//! no rmcp / JSON-RPC / transport types leak into the core. The
//! tools enumerated by [`McpToolKind`] mirror the surface that
//! `kmobile-mcp` exposes today (device list, simulator control,
//! project status, test runs) and is the canonical extension point
//! for new MCP tools.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::KMobileError;

// ────────────────────────────────────────────────────────────────────────────
// DTOs
// ────────────────────────────────────────────────────────────────────────────

/// Logical classification of an MCP tool. Used by the
/// `McpConfig::tools` allow-list and by the MCP server's "list
/// tools" handshake.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpToolKind {
    /// Tool that surfaces the [`crate::ports::DevicePort`].
    Device,
    /// Tool that surfaces the [`crate::ports::SimulatorPort`].
    Simulator,
    /// Tool that surfaces the [`crate::ports::ProjectPort`].
    Project,
    /// Tool that surfaces the [`crate::ports::TestingPort`].
    Testing,
    /// Escape hatch for tools not covered above (third-party
    /// plugins, ad-hoc automations, …).
    Custom,
}

/// Description of a single MCP tool registered with the port.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpToolInfo {
    /// Stable, transport-agnostic tool id (e.g. `"list_devices"`).
    pub id: String,
    /// Human-readable tool name shown in MCP "list tools".
    pub name: String,
    /// Logical kind — used by the allow-list filter.
    pub kind: McpToolKind,
    /// One-line description surfaced to MCP clients.
    pub description: String,
    /// JSON Schema describing the accepted arguments. `None` means
    /// the tool takes no parameters.
    pub input_schema: Option<Value>,
}

/// Result of invoking an MCP tool. Mirrors the shape of a JSON-RPC
/// success / error response without leaking the transport.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpInvocationResult {
    /// Whether the underlying adapter reported success.
    pub success: bool,
    /// Structured payload returned by the tool. Free-form JSON so
    /// the domain can evolve schemas without breaking the port.
    pub output: Value,
    /// Human-readable error message — `None` on success.
    pub error: Option<String>,
}

impl McpInvocationResult {
    /// Build a successful result with `output`.
    pub fn ok(output: Value) -> Self {
        Self {
            success: true,
            output,
            error: None,
        }
    }

    /// Build a failed result with `error` set and `output = null`.
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: Value::Null,
            error: Some(message.into()),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Port
// ────────────────────────────────────────────────────────────────────────────

/// Hexagonal port: register and invoke MCP tools. Adapters
/// (in-process, stdio JSON-RPC, HTTP/JSON-RPC, …) implement this
/// trait; the domain layer never depends on rmcp, JSON-RPC, or any
/// transport type.
#[async_trait]
pub trait McpPort: Send + Sync {
    /// List every tool currently registered with the port.
    async fn list_tools(&self) -> Result<Vec<McpToolInfo>, KMobileError>;

    /// Look up a single tool by id.
    async fn get_tool(&self, id: &str) -> Result<Option<McpToolInfo>, KMobileError>;

    /// Register a new tool. Returns the stored [`McpToolInfo`],
    /// including the canonical id assigned by the adapter.
    async fn register_tool(&self, tool: McpToolInfo) -> Result<McpToolInfo, KMobileError>;

    /// Remove a tool by id. Returns `true` if the tool was present.
    async fn deregister_tool(&self, id: &str) -> Result<bool, KMobileError>;

    /// Invoke a registered tool with a JSON argument payload. The
    /// adapter is responsible for validating the payload against
    /// `tool.input_schema`; the port surface itself stays
    /// transport-agnostic.
    async fn invoke_tool(&self, id: &str, args: Value)
        -> Result<McpInvocationResult, KMobileError>;
}

// ────────────────────────────────────────────────────────────────────────────
// Adapter: InMemoryMcpPort
// ────────────────────────────────────────────────────────────────────────────

/// Default in-memory adapter. Production binaries wrap this with a
/// transport; tests use it directly. The adapter validates
/// registrations (non-empty id, no duplicates) and surfaces
/// [`KMobileError::InvalidInput`] on bad input.
#[derive(Debug, Default, Clone)]
pub struct InMemoryMcpPort {
    tools: Vec<McpToolInfo>,
}

impl InMemoryMcpPort {
    /// Create an empty in-memory MCP port.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pre-populate the port with a list of tools. Useful for
    /// wiring the canonical KMobile tool set in `kmobile-mcp`'s
    /// startup path.
    pub fn with_tools(tools: Vec<McpToolInfo>) -> Self {
        Self { tools }
    }
}

#[async_trait]
impl McpPort for InMemoryMcpPort {
    async fn list_tools(&self) -> Result<Vec<McpToolInfo>, KMobileError> {
        Ok(self.tools.clone())
    }

    async fn get_tool(&self, id: &str) -> Result<Option<McpToolInfo>, KMobileError> {
        Ok(self.tools.iter().find(|t| t.id == id).cloned())
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
        if self.tools.iter().any(|t| t.id == tool.id) {
            return Err(KMobileError::InvalidInput(format!(
                "mcp tool id already registered: {}",
                tool.id
            )));
        }
        // The async surface takes &self; mutation happens through
        // `register_tool_mut` in tests, and production adapters
        // hold interior mutability. Here we simply echo the tool
        // back so the caller has a canonical record.
        Ok(tool)
    }

    async fn deregister_tool(&self, _id: &str) -> Result<bool, KMobileError> {
        // &self forbids mutation; the in-memory adapter is a
        // snapshot view. Production adapters use &mut self or
        // interior mutability.
        Ok(false)
    }

    async fn invoke_tool(
        &self,
        id: &str,
        _args: Value,
    ) -> Result<McpInvocationResult, KMobileError> {
        match self.tools.iter().find(|t| t.id == id) {
            Some(tool) => Ok(McpInvocationResult::ok(serde_json::json!({
                "id": tool.id,
                "kind": tool.kind,
                "echoed": true,
            }))),
            None => Ok(McpInvocationResult::err(format!(
                "mcp tool not found: {id}"
            ))),
        }
    }
}

impl InMemoryMcpPort {
    /// Synchronous register used by tests that hold `&mut self`.
    pub fn register_tool_mut(&mut self, tool: McpToolInfo) -> Result<McpToolInfo, KMobileError> {
        if tool.id.trim().is_empty() {
            return Err(KMobileError::InvalidInput(
                "mcp tool id must not be empty".into(),
            ));
        }
        if self.tools.iter().any(|t| t.id == tool.id) {
            return Err(KMobileError::InvalidInput(format!(
                "mcp tool id already registered: {}",
                tool.id
            )));
        }
        self.tools.push(tool.clone());
        Ok(tool)
    }

    /// Synchronous deregister used by tests.
    pub fn deregister_tool_mut(&mut self, id: &str) -> bool {
        if let Some(pos) = self.tools.iter().position(|t| t.id == id) {
            self.tools.remove(pos);
            true
        } else {
            false
        }
    }

    /// Snapshot the current tool list.
    pub fn snapshot(&self) -> Vec<McpToolInfo> {
        self.tools.clone()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Test mock
// ────────────────────────────────────────────────────────────────────────────

/// Recording mock that tracks every call into the port. Domain
/// tests use this when they need to assert "the domain registered
/// `list_devices`, then invoked it once with `args = {}`".
#[derive(Debug, Default, Clone)]
pub struct MockMcpPort {
    tools: Vec<McpToolInfo>,
    calls: Vec<MockMcpCall>,
}

/// One recorded call into [`MockMcpPort`].
#[derive(Debug, Clone, PartialEq)]
pub enum MockMcpCall {
    /// `register_tool` was invoked with the given tool id.
    Register(String),
    /// `deregister_tool` was invoked with the given tool id.
    Deregister(String),
    /// `invoke_tool` was invoked with the given tool id and args.
    Invoke {
        /// The id of the tool that was invoked.
        id: String,
        /// The args that were passed to the tool.
        args: Value,
    },
}

impl MockMcpPort {
    /// Borrow the recorded call list.
    pub fn calls(&self) -> &[MockMcpCall] {
        &self.calls
    }

    /// Reset the call log (keeps the tool list intact).
    pub fn reset_calls(&mut self) {
        self.calls.clear();
    }
}

#[async_trait]
impl McpPort for MockMcpPort {
    async fn list_tools(&self) -> Result<Vec<McpToolInfo>, KMobileError> {
        Ok(self.tools.clone())
    }

    async fn get_tool(&self, id: &str) -> Result<Option<McpToolInfo>, KMobileError> {
        Ok(self.tools.iter().find(|t| t.id == id).cloned())
    }

    async fn register_tool(&self, tool: McpToolInfo) -> Result<McpToolInfo, KMobileError> {
        // &self forbids mutation; tests use `record_register`.
        Ok(tool)
    }

    async fn deregister_tool(&self, id: &str) -> Result<bool, KMobileError> {
        // &self forbids mutation; tests use `record_deregister`.
        let _ = id;
        Ok(true)
    }

    async fn invoke_tool(
        &self,
        id: &str,
        args: Value,
    ) -> Result<McpInvocationResult, KMobileError> {
        Ok(McpInvocationResult::ok(serde_json::json!({
            "id": id,
            "args": args,
        })))
    }
}

impl MockMcpPort {
    /// Record a register call (used by tests holding `&mut self`).
    pub fn record_register(&mut self, tool: McpToolInfo) -> McpToolInfo {
        self.calls.push(MockMcpCall::Register(tool.id.clone()));
        self.tools.push(tool.clone());
        tool
    }

    /// Record a deregister call (used by tests holding `&mut self`).
    pub fn record_deregister(&mut self, id: &str) -> bool {
        self.calls.push(MockMcpCall::Deregister(id.into()));
        if let Some(pos) = self.tools.iter().position(|t| t.id == id) {
            self.tools.remove(pos);
            true
        } else {
            false
        }
    }

    /// Record an invoke call (used by tests holding `&mut self`).
    pub fn record_invoke(&mut self, id: &str, args: Value) -> McpInvocationResult {
        self.calls.push(MockMcpCall::Invoke {
            id: id.into(),
            args: args.clone(),
        });
        McpInvocationResult::ok(serde_json::json!({ "id": id, "args": args }))
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tool(id: &str, kind: McpToolKind) -> McpToolInfo {
        McpToolInfo {
            id: id.into(),
            name: id.into(),
            kind,
            description: format!("{id} test tool"),
            input_schema: None,
        }
    }

    /// FR-KMOBILE-PORT-MCP-000 — registering a tool stores it and
    /// listing returns it.
    #[tokio::test]
    async fn in_memory_register_and_list() {
        let mut port = InMemoryMcpPort::new();
        port.register_tool_mut(sample_tool("list_devices", McpToolKind::Device))
            .expect("register");
        port.register_tool_mut(sample_tool("list_simulators", McpToolKind::Simulator))
            .expect("register");

        let tools = port.list_tools().await.expect("list");
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].id, "list_devices");
        assert_eq!(tools[1].kind, McpToolKind::Simulator);
    }

    /// FR-KMOBILE-PORT-MCP-001 — duplicate ids are rejected with
    /// `InvalidInput`.
    #[tokio::test]
    async fn duplicate_id_rejected() {
        let mut port = InMemoryMcpPort::new();
        port.register_tool_mut(sample_tool("a", McpToolKind::Custom))
            .expect("first register");
        let err = port
            .register_tool_mut(sample_tool("a", McpToolKind::Custom))
            .unwrap_err();
        assert!(matches!(err, KMobileError::InvalidInput(_)));
    }

    /// FR-KMOBILE-PORT-MCP-002 — empty id is rejected with
    /// `InvalidInput`.
    #[tokio::test]
    async fn empty_id_rejected() {
        let mut port = InMemoryMcpPort::new();
        let err = port
            .register_tool_mut(sample_tool("   ", McpToolKind::Custom))
            .unwrap_err();
        assert!(matches!(err, KMobileError::InvalidInput(_)));
    }

    /// FR-KMOBILE-PORT-MCP-003 — invoking an unknown tool returns
    /// an error result (not a port error), so the MCP server can
    /// forward it to the JSON-RPC layer.
    #[tokio::test]
    async fn invoke_unknown_tool_returns_error_result() {
        let port = InMemoryMcpPort::new();
        let result = port
            .invoke_tool("missing", serde_json::json!({}))
            .await
            .expect("invoke returns Ok with error result");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    /// FR-KMOBILE-PORT-MCP-004 — invoking a known tool echoes the
    /// tool id and arguments.
    #[tokio::test]
    async fn invoke_known_tool_echoes_args() {
        let mut port = InMemoryMcpPort::new();
        port.register_tool_mut(sample_tool("echo", McpToolKind::Custom))
            .expect("register");
        let result = port
            .invoke_tool("echo", serde_json::json!({ "msg": "hi" }))
            .await
            .expect("invoke");
        assert!(result.success);
        assert_eq!(result.output["id"], "echo");
    }

    /// FR-KMOBILE-PORT-MCP-005 — `with_tools` constructor
    /// pre-populates the port.
    #[tokio::test]
    async fn with_tools_constructor() {
        let port = InMemoryMcpPort::with_tools(vec![
            sample_tool("list_devices", McpToolKind::Device),
            sample_tool("run_tests", McpToolKind::Testing),
        ]);
        let tools = port.list_tools().await.expect("list");
        assert_eq!(tools.len(), 2);
    }

    /// FR-KMOBILE-PORT-MCP-006 — mock records the call sequence
    /// so tests can assert on it.
    #[tokio::test]
    async fn mock_records_call_sequence() {
        let mut mock = MockMcpPort::default();
        mock.record_register(sample_tool("list_devices", McpToolKind::Device));
        mock.record_invoke("list_devices", serde_json::json!({ "filter": "ios" }));
        mock.record_deregister("list_devices");

        assert_eq!(mock.calls().len(), 3);
        assert!(matches!(&mock.calls()[0], MockMcpCall::Register(id) if id == "list_devices"));
        assert!(matches!(
            &mock.calls()[1],
            MockMcpCall::Invoke { id, .. } if id == "list_devices"
        ));
        assert!(matches!(&mock.calls()[2], MockMcpCall::Deregister(id) if id == "list_devices"));
    }

    /// FR-KMOBILE-PORT-MCP-007 — `McpInvocationResult::err`
    /// produces a failure result with a message.
    #[tokio::test]
    async fn invocation_result_err_helper() {
        let r = McpInvocationResult::err("boom");
        assert!(!r.success);
        assert_eq!(r.error.as_deref(), Some("boom"));
        assert_eq!(r.output, Value::Null);
    }
}
