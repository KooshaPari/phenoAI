// Integration tests for mcp-server crate
// Traces to: FR-001

use mcp_server::{ContentItem, McpError, McpServer, Resource, Tool, ToolResult};

#[tokio::test]
async fn test_mcp_server_creation() {
    let server = McpServer::new();
    let tools = server.list_tools().await;
    let resources = server.list_resources().await;

    assert!(tools.is_empty());
    assert!(resources.is_empty());
}

#[tokio::test]
async fn test_register_tool() {
    let server = McpServer::new();

    let tool = Tool {
        name: "test_tool".to_string(),
        description: "A test tool".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "input": {"type": "string"}
            }
        }),
    };

    server
        .register_tool(tool, |args| {
            Ok(serde_json::json!({"result": "success"}))
        })
        .await;

    let tools = server.list_tools().await;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "test_tool");
}

#[tokio::test]
async fn test_register_multiple_tools() {
    let server = McpServer::new();

    for i in 0..5 {
        let tool = Tool {
            name: format!("tool_{}", i),
            description: format!("Tool number {}", i),
            input_schema: serde_json::json!({"type": "object"}),
        };

        server
            .register_tool(tool, move |_| Ok(serde_json::json!({"id": i})))
            .await;
    }

    let tools = server.list_tools().await;
    assert_eq!(tools.len(), 5);
}

#[tokio::test]
async fn test_call_tool() {
    let server = McpServer::new();

    let tool = Tool {
        name: "echo".to_string(),
        description: "Echoes the input".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "message": {"type": "string"}
            }
        }),
    };

    server
        .register_tool(tool, |args| {
            let msg = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
            Ok(serde_json::json!({"echoed": msg}))
        })
        .await;

    let result = server
        .call_tool("echo", serde_json::json!({"message": "hello"}))
        .await
        .expect("Should succeed");

    assert!(!result.is_error);
    assert_eq!(result.content.len(), 1);
    assert_eq!(result.content[0].content_type, "text");
}

#[tokio::test]
async fn test_call_nonexistent_tool() {
    let server = McpServer::new();

    let result = server
        .call_tool("nonexistent", serde_json::json!({}))
        .await;

    assert!(result.is_err());
    match result {
        Err(McpError::ToolNotFound(name)) => assert_eq!(name, "nonexistent"),
        _ => panic!("Expected ToolNotFound error"),
    }
}

#[tokio::test]
async fn test_register_resource() {
    let server = McpServer::new();

    let resource = Resource {
        uri: "file:///test/data.txt".to_string(),
        name: "test_data".to_string(),
        mime_type: Some("text/plain".to_string()),
    };

    server.register_resource(resource).await;

    let resources = server.list_resources().await;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, "file:///test/data.txt");
}

#[tokio::test]
async fn test_read_resource() {
    let server = McpServer::new();

    let resource = Resource {
        uri: "file:///test/readable.txt".to_string(),
        name: "readable_file".to_string(),
        mime_type: Some("text/plain".to_string()),
    };

    server.register_resource(resource).await;

    let content = server
        .read_resource("file:///test/readable.txt")
        .await
        .expect("Should read resource");

    assert!(content.contains("readable_file"));
}

#[tokio::test]
async fn test_read_nonexistent_resource() {
    let server = McpServer::new();

    let result = server.read_resource("file:///nonexistent").await;

    assert!(result.is_err());
    match result {
        Err(McpError::ResourceNotFound(uri)) => assert_eq!(uri, "file:///nonexistent"),
        _ => panic!("Expected ResourceNotFound error"),
    }
}

#[test]
fn test_tool_serialization() {
    let tool = Tool {
        name: "test".to_string(),
        description: "A test".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
    };

    let json = serde_json::to_string(&tool).expect("Should serialize");
    let deserialized: Tool = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(deserialized.name, "test");
    assert_eq!(deserialized.description, "A test");
}

#[test]
fn test_tool_result_serialization() {
    let result = ToolResult {
        content: vec![
            ContentItem {
                content_type: "text".to_string(),
                text: Some("Hello".to_string()),
            },
        ],
        is_error: false,
    };

    let json = serde_json::to_string(&result).expect("Should serialize");
    let deserialized: ToolResult = serde_json::from_str(&json).expect("Should deserialize");

    assert!(!deserialized.is_error);
    assert_eq!(deserialized.content.len(), 1);
    assert_eq!(deserialized.content[0].text, Some("Hello".to_string()));
}

#[test]
fn test_resource_serialization() {
    let resource = Resource {
        uri: "test://resource".to_string(),
        name: "my_resource".to_string(),
        mime_type: Some("application/json".to_string()),
    };

    let json = serde_json::to_string(&resource).expect("Should serialize");
    let deserialized: Resource = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(deserialized.uri, "test://resource");
    assert_eq!(deserialized.mime_type, Some("application/json".to_string()));
}

#[test]
fn test_mcp_error_display() {
    let err = McpError::ToolNotFound("tool".to_string());
    assert_eq!(format!("{}", err), "tool not found: tool");

    let err = McpError::ResourceNotFound("resource".to_string());
    assert_eq!(format!("{}", err), "resource not found: resource");

    let err = McpError::InvalidRequest("bad input".to_string());
    assert_eq!(format!("{}", err), "invalid request: bad input");
}

#[test]
fn test_content_item_serialization() {
    let item = ContentItem {
        content_type: "text".to_string(),
        text: Some("Sample text".to_string()),
    };

    let json = serde_json::to_string(&item).expect("Should serialize");
    let deserialized: ContentItem = serde_json::from_str(&json).expect("Should deserialize");

    assert_eq!(deserialized.content_type, "text");
    assert_eq!(deserialized.text, Some("Sample text".to_string()));
}
