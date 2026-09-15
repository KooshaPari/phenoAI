//! Integration tests for the `McpPort` hexagonal port.
//!
//! `McpPort` is the trait that drives the Model Context Protocol
//! surface: register / list / invoke / deregister tools.  These
//! tests exercise the in-memory adapter and the recording mock
//! from a separate test binary so any change to the trait
//! signature or the DTOs is caught at the integration boundary.

use kmobile_core::{
    InMemoryMcpPort, KMobileError, McpInvocationResult, McpPort, McpToolInfo, McpToolKind,
    MockMcpCall, MockMcpPort,
};

fn sample_tool(id: &str, kind: McpToolKind) -> McpToolInfo {
    McpToolInfo {
        id: id.into(),
        name: id.into(),
        kind,
        description: format!("{id} test tool"),
        input_schema: None,
    }
}

/// FR-KMOBILE-PORT-MCP-IT-000 — full MCP tool lifecycle
/// (register → list → invoke → deregister) preserves the call
/// order and returns the expected payload.
#[tokio::test]
async fn mcp_port_in_memory_full_lifecycle() {
    let mut port = InMemoryMcpPort::new();

    port.register_tool_mut(sample_tool("list_devices", McpToolKind::Device))
        .expect("register list_devices");
    port.register_tool_mut(sample_tool("list_simulators", McpToolKind::Simulator))
        .expect("register list_simulators");

    // Listing returns both tools.
    let tools = port.list_tools().await.expect("list");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].id, "list_devices");
    assert_eq!(tools[1].kind, McpToolKind::Simulator);

    // Lookup by id works.
    let got = port
        .get_tool("list_simulators")
        .await
        .expect("get")
        .expect("present");
    assert_eq!(got.kind, McpToolKind::Simulator);

    // Invoking a known tool echoes the args.
    let result = port
        .invoke_tool("list_devices", serde_json::json!({ "platform": "ios" }))
        .await
        .expect("invoke");
    assert!(result.success);
    assert_eq!(result.output["id"], "list_devices");

    // Deregister removes the tool.
    assert!(port.deregister_tool_mut("list_devices"));
    let after = port.list_tools().await.expect("list");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, "list_simulators");
}

/// FR-KMOBILE-PORT-MCP-IT-001 — `register_tool` validates input:
/// empty / whitespace ids and duplicate ids are rejected.
#[tokio::test]
async fn mcp_port_register_validates_input() {
    let mut port = InMemoryMcpPort::new();

    // Empty id → InvalidInput.
    let err = port
        .register_tool_mut(sample_tool("   ", McpToolKind::Custom))
        .expect_err("empty id must error");
    assert!(matches!(err, KMobileError::InvalidInput(_)));

    // Duplicate id → InvalidInput.
    port.register_tool_mut(sample_tool("a", McpToolKind::Custom))
        .expect("first register");
    let err = port
        .register_tool_mut(sample_tool("a", McpToolKind::Custom))
        .expect_err("duplicate id must error");
    assert!(matches!(err, KMobileError::InvalidInput(_)));
}

/// FR-KMOBILE-PORT-MCP-IT-002 — invoking an unknown tool returns
/// an error **result** (not a port error), so the MCP server can
/// forward it as a JSON-RPC error response.
#[tokio::test]
async fn mcp_port_invoke_unknown_tool_returns_error_result() {
    let port = InMemoryMcpPort::new();

    let result = port
        .invoke_tool("missing", serde_json::json!({}))
        .await
        .expect("invoke returns Ok with error result");

    assert!(!result.success);
    assert!(result.error.is_some());
    let msg = result.error.unwrap();
    assert!(msg.contains("missing"), "error message: {msg}");

    // The output is JSON null on failure.
    assert_eq!(result.output, serde_json::Value::Null);
}

/// FR-KMOBILE-PORT-MCP-IT-003 — `MockMcpPort` records the call
/// sequence so domain tests can assert on it.
#[tokio::test]
async fn mcp_port_mock_records_call_sequence() {
    let mut mock = MockMcpPort::default();

    mock.record_register(sample_tool("list_devices", McpToolKind::Device));
    mock.record_invoke("list_devices", serde_json::json!({ "filter": "ios" }));
    mock.record_deregister("list_devices");

    let calls = mock.calls();
    assert_eq!(calls.len(), 3);
    assert!(matches!(&calls[0], MockMcpCall::Register(id) if id == "list_devices"));
    assert!(matches!(
        &calls[1],
        MockMcpCall::Invoke { id, args } if id == "list_devices" && args["filter"] == "ios"
    ));
    assert!(matches!(&calls[2], MockMcpCall::Deregister(id) if id == "list_devices"));

    // reset_calls clears the log but leaves the storage alone.
    // In this case the storage is empty because the previous
    // record_deregister removed the only registered tool.
    mock.reset_calls();
    assert!(mock.calls().is_empty());

    let stored = mock.list_tools().await.expect("list");
    assert!(stored.is_empty());
}

/// FR-KMOBILE-PORT-MCP-IT-004 — `McpToolInfo` and
/// `McpInvocationResult` round-trip through JSON, so the port
/// can be transported over MCP / HTTP / saved to disk.
#[tokio::test]
async fn mcp_port_dto_serde_roundtrip() {
    let tool = McpToolInfo {
        id: "run_tests".into(),
        name: "Run tests".into(),
        kind: McpToolKind::Testing,
        description: "Run the test suite".into(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "suite": { "type": "string" } },
            "required": ["suite"]
        })),
    };

    let json = serde_json::to_string(&tool).expect("serialize");
    let recovered: McpToolInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(recovered.id, tool.id);
    assert_eq!(recovered.kind, tool.kind);
    assert!(recovered.input_schema.is_some());

    // `McpInvocationResult` round-trips.
    let result = McpInvocationResult::ok(serde_json::json!({ "passed": 7 }));
    let json = serde_json::to_string(&result).expect("serialize");
    let recovered: McpInvocationResult = serde_json::from_str(&json).expect("deserialize");
    assert!(recovered.success);
    assert_eq!(recovered.output["passed"], 7);

    // `McpInvocationResult::err` produces a failure with a message.
    let err_result = McpInvocationResult::err("boom");
    assert!(!err_result.success);
    assert_eq!(err_result.error.as_deref(), Some("boom"));
    assert_eq!(err_result.output, serde_json::Value::Null);
}
