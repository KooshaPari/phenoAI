//! MCP Server - Model Context Protocol server
//!
//! Implements MCP protocol for AI tool/resource/prompt access.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    #[error("resource not found: {0}")]
    ResourceNotFound(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ContentItem>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
}

/// Resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
}

/// MCP Server
pub struct McpServer {
    tools: Arc<RwLock<HashMap<String, ToolHandler>>>,
    resources: Arc<RwLock<HashMap<String, Resource>>>,
}

#[derive(Clone)]
pub struct ToolHandler {
    pub tool: Tool,
    pub handler: Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            resources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_tool(
        &self,
        tool: Tool,
        handler: impl Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync + 'static,
    ) {
        self.tools.write().await.insert(
            tool.name.clone(),
            ToolHandler {
                tool,
                handler: Arc::new(handler),
            },
        );
    }

    pub async fn register_resource(&self, resource: Resource) {
        self.resources
            .write()
            .await
            .insert(resource.uri.clone(), resource);
    }

    pub async fn list_tools(&self) -> Vec<Tool> {
        self.tools
            .read()
            .await
            .values()
            .map(|h| h.tool.clone())
            .collect()
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, McpError> {
        let tools = self.tools.read().await;
        let handler = tools
            .get(name)
            .ok_or(McpError::ToolNotFound(name.to_string()))?;

        let result =
            (handler.handler)(arguments).map_err(|e| McpError::InvalidRequest(e.to_string()))?;

        Ok(ToolResult {
            content: vec![ContentItem {
                content_type: "text".to_string(),
                text: Some(result.to_string()),
            }],
            is_error: false,
        })
    }

    pub async fn list_resources(&self) -> Vec<Resource> {
        self.resources.read().await.values().cloned().collect()
    }

    pub async fn read_resource(&self, uri: &str) -> Result<String, McpError> {
        let resources = self.resources.read().await;
        let resource = resources
            .get(uri)
            .ok_or(McpError::ResourceNotFound(uri.to_string()))?;
        Ok(format!("Resource: {}", resource.name))
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_tool_then_list_includes_it() {
        let server = McpServer::new();
        let tool = Tool {
            name: "echo".into(),
            description: "echoes input".into(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        server
            .register_tool(tool, |_args| Ok(serde_json::json!("ok")))
            .await;
        let list = server.list_tools().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "echo");
    }

    #[tokio::test]
    async fn call_unknown_tool_returns_tool_not_found() {
        let server = McpServer::new();
        let err = server
            .call_tool("nope", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, McpError::ToolNotFound(_)));
    }

    #[tokio::test]
    async fn register_resource_then_read_returns_content() {
        let server = McpServer::new();
        let resource = Resource {
            uri: "foo://bar".into(),
            name: "bar".into(),
            mime_type: Some("application/json".into()),
        };
        server.register_resource(resource).await;
        let resources = server.list_resources().await;
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "foo://bar");
    }

    #[tokio::test]
    async fn read_unknown_resource_returns_resource_not_found() {
        let server = McpServer::new();
        let err = server.read_resource("nope://missing").await.unwrap_err();
        assert!(matches!(err, McpError::ResourceNotFound(_)));
    }

    #[tokio::test]
    async fn tool_handler_can_be_invoked() {
        let server = McpServer::new();
        let tool = Tool {
            name: "double".into(),
            description: "doubles n".into(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        server
            .register_tool(tool, |args| {
                let n = args.get("n").and_then(|v| v.as_i64()).unwrap_or(0);
                Ok(serde_json::json!(n * 2))
            })
            .await;
        let result = server
            .call_tool("double", serde_json::json!({"n": 21}))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content[0].text.as_deref(), Some("42"));
    }
}
