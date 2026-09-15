//! JSON-RPC 2.0 wire types and read/write helpers.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// JSON-RPC 2.0 request from caller (also used by the sandbox bridge client).
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response to caller (also parsed by the sandbox bridge client).
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Response {
    /// Successful response.
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Error response with explicit code.
    pub fn err(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Internal error (code -32603).
    pub fn internal_error(id: serde_json::Value, msg: impl Into<String>) -> Self {
        Self::err(id, -32603, msg)
    }

    /// Method not found (code -32601).
    pub fn method_not_found(id: serde_json::Value, method: &str) -> Self {
        Self::err(id, -32601, format!("Method not found: {method}"))
    }

    /// Invalid params (code -32602).
    pub fn invalid_params(id: serde_json::Value, msg: impl Into<String>) -> Self {
        Self::err(id, -32602, msg)
    }
}

/// Read one newline-delimited JSON-RPC request from stdin.
pub async fn read_request<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Request>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let req: Request = serde_json::from_str(trimmed)?;
    Ok(Some(req))
}

/// Write one JSON-RPC response to stdout, followed by newline.
pub async fn write_response<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    response: &Response,
) -> Result<()> {
    let mut json = serde_json::to_string(response)?;
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
