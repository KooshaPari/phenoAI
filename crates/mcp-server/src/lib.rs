//! MCP Server - Model Context Protocol server
//!
//! Implements MCP protocol for AI tool/resource/prompt access.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;

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

    pub async fn register_tool(&self, tool: Tool, handler: impl Fn(serde_json::Value) -> Result<serde_json::Value> + Send + Sync + 'static) {
        self.tools.write().await.insert(tool.name.clone(), ToolHandler {
            tool,
            handler: Arc::new(handler),
        });
    }

    pub async fn register_resource(&self, resource: Resource) {
        self.resources.write().await.insert(resource.uri.clone(), resource);
    }

    pub async fn list_tools(&self) -> Vec<Tool> {
        self.tools.read().await.values().map(|h| h.tool.clone()).collect()
    }

    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<ToolResult, McpError> {
        let tools = self.tools.read().await;
        let handler = tools.get(name).ok_or(McpError::ToolNotFound(name.to_string()))?;
        
        let result = (handler.handler)(arguments).map_err(|e| McpError::InvalidRequest(e.to_string()))?;
        
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
        let resource = resources.get(uri).ok_or(McpError::ResourceNotFound(uri.to_string()))?;
        Ok(format!("Resource: {}", resource.name))
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}
