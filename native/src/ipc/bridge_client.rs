//! Stdio NDJSON JSON-RPC 2.0 client for the sandbox `playcua-bridge` tunnel.
//!
//! Host playcua-native speaks to a guest-side bridge over piped stdin/stdout:
//! one request line → one response line. Same method names as the public
//! dispatcher (`screenshot`, `input.*`, `windows.*`).
//!
//! Production: spawn `PLAYCUA_BRIDGE_BIN` or `playcua-bridge` on `$PATH`
//! (optionally wrapped by [`SandboxDriver`]). Tests: inject any
//! `AsyncRead + AsyncWrite` pair (in-process duplex or fake script).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex as AsyncMutex;

use super::mod_types::{Request, Response};

/// Serializes tests that mutate `PLAYCUA_BRIDGE_BIN` (process-global).
/// Available outside `cfg(test)` so integration tests can take the same lock.
pub static BRIDGE_ENV_LOCK: AsyncMutex<()> = AsyncMutex::const_new(());

/// Errors from the sandbox JSON-RPC bridge client.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error(
        "playcua-bridge not found: set PLAYCUA_BRIDGE_BIN or install playcua-bridge on $PATH ({0})"
    )]
    BinaryMissing(String),
    #[error("bridge spawn failed: {0}")]
    SpawnFailed(String),
    #[error("bridge transport closed")]
    Closed,
    #[error("bridge I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("bridge protocol error: {0}")]
    Protocol(String),
    #[error("bridge RPC error ({code}): {message}")]
    Rpc { code: i32, message: String },
}

/// Live stdio session to a playcua-bridge (or hermetic fake).
pub struct BridgeClient {
    io: AsyncMutex<BridgeIo>,
    next_id: AtomicU64,
}

enum BridgeIo {
    Child {
        child: Box<Child>,
        stdin: ChildStdin,
        stdout: BufReader<ChildStdout>,
    },
    Duplex {
        stream: BufReader<DuplexStream>,
    },
}

impl BridgeClient {
    /// Resolve bridge binary: `PLAYCUA_BRIDGE_BIN`, else `which playcua-bridge`.
    pub fn resolve_binary() -> Result<PathBuf, BridgeError> {
        if let Ok(override_bin) = std::env::var("PLAYCUA_BRIDGE_BIN") {
            let p = PathBuf::from(&override_bin);
            if p.is_file() {
                return Ok(p);
            }
            return Err(BridgeError::BinaryMissing(format!(
                "PLAYCUA_BRIDGE_BIN={override_bin} is not a file"
            )));
        }
        which("playcua-bridge").ok_or_else(|| {
            BridgeError::BinaryMissing(
                "no PLAYCUA_BRIDGE_BIN and playcua-bridge not on $PATH".into(),
            )
        })
    }

    /// Spawn `program` with piped stdio and wrap as a bridge client.
    pub async fn spawn(program: &Path, args: &[String]) -> Result<Self, BridgeError> {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            unsafe {
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| BridgeError::SpawnFailed(format!("{}: {e}", program.display())))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BridgeError::SpawnFailed("stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BridgeError::SpawnFailed("stdout missing".into()))?;
        Ok(Self {
            io: AsyncMutex::new(BridgeIo::Child {
                child: Box::new(child),
                stdin,
                stdout: BufReader::new(stdout),
            }),
            next_id: AtomicU64::new(1),
        })
    }

    /// Connect using the resolved bridge binary (fail-loud if missing).
    pub async fn connect_default() -> Result<Self, BridgeError> {
        let bin = Self::resolve_binary()?;
        Self::spawn(&bin, &[]).await
    }

    /// In-process duplex for hermetic unit tests (no child process).
    ///
    /// Returns `(client, peer)`. The peer half is a raw `DuplexStream` that a
    /// test fake-server loop can drive with [`read_request`] /
    /// [`write_response`].
    pub fn duplex_pair(max_buf: usize) -> (Self, DuplexStream) {
        let (client_side, peer) = tokio::io::duplex(max_buf);
        let client = Self {
            io: AsyncMutex::new(BridgeIo::Duplex {
                stream: BufReader::new(client_side),
            }),
            next_id: AtomicU64::new(1),
        };
        (client, peer)
    }

    /// Send one NDJSON request and wait for the matching response.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, BridgeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: json!(id),
            method: method.into(),
            params: if params.is_null() { None } else { Some(params) },
        };
        let mut guard = self.io.lock().await;
        write_request_line(&mut guard, &req).await?;
        let resp = read_response_line(&mut guard).await?;
        if resp.id != req.id {
            return Err(BridgeError::Protocol(format!(
                "id mismatch: sent {}, got {}",
                req.id, resp.id
            )));
        }
        if let Some(err) = resp.error {
            return Err(BridgeError::Rpc {
                code: err.code,
                message: err.message,
            });
        }
        // JSON-RPC allows `"result": null` (e.g. windows.find miss). serde maps
        // that to `Option::None` for `Option<Value>`, so treat absent result
        // with no error as JSON null rather than a protocol failure.
        Ok(resp.result.unwrap_or(Value::Null))
    }

    /// Best-effort shutdown of a child-backed client.
    pub async fn shutdown(&self) -> Result<(), BridgeError> {
        let mut guard = self.io.lock().await;
        match &mut *guard {
            BridgeIo::Child { child, .. } => {
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
            BridgeIo::Duplex { .. } => {}
        }
        Ok(())
    }
}

impl Drop for BridgeClient {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.io.try_lock() {
            if let BridgeIo::Child { child, .. } = &mut *guard {
                let _ = child.start_kill();
            }
        }
    }
}

async fn write_request_line(io: &mut BridgeIo, req: &Request) -> Result<(), BridgeError> {
    let mut line = serde_json::to_string(req).map_err(|e| BridgeError::Protocol(e.to_string()))?;
    line.push('\n');
    match io {
        BridgeIo::Child { stdin, .. } => {
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }
        BridgeIo::Duplex { stream } => {
            stream.get_mut().write_all(line.as_bytes()).await?;
            stream.get_mut().flush().await?;
        }
    }
    Ok(())
}

async fn read_response_line(io: &mut BridgeIo) -> Result<Response, BridgeError> {
    let mut line = String::new();
    let n = match io {
        BridgeIo::Child { stdout, .. } => stdout.read_line(&mut line).await?,
        BridgeIo::Duplex { stream } => stream.read_line(&mut line).await?,
    };
    if n == 0 {
        return Err(BridgeError::Closed);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(BridgeError::Protocol("empty response line".into()));
    }
    serde_json::from_str(trimmed).map_err(|e| BridgeError::Protocol(e.to_string()))
}

fn which(bin: &str) -> Option<PathBuf> {
    let var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&var) {
        let candidate = dir.join(bin);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::mod_types::{write_response, Response as WireResponse};

    #[tokio::test]
    async fn duplex_round_trip_ok() {
        let (client, mut peer) = BridgeClient::duplex_pair(64 * 1024);
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(&mut peer);
            let req = crate::ipc::mod_types::read_request(&mut reader)
                .await
                .expect("read")
                .expect("eof");
            assert_eq!(req.method, "ping");
            let resp = WireResponse::ok(req.id, json!({ "ok": true }));
            write_response(&mut peer, &resp).await.expect("write");
        });
        let result = client.call("ping", Value::Null).await.expect("call");
        assert_eq!(result["ok"], true);
        server.await.expect("server");
    }

    #[tokio::test]
    async fn duplex_rpc_error_surfaces() {
        let (client, mut peer) = BridgeClient::duplex_pair(64 * 1024);
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(&mut peer);
            let req = crate::ipc::mod_types::read_request(&mut reader)
                .await
                .expect("read")
                .expect("eof");
            let resp = WireResponse::err(req.id, -32000, "boom");
            write_response(&mut peer, &resp).await.expect("write");
        });
        let err = client
            .call("screenshot", json!({}))
            .await
            .expect_err("must surface RPC error");
        assert!(matches!(err, BridgeError::Rpc { code: -32000, .. }));
        server.await.expect("server");
    }

    #[tokio::test]
    async fn duplex_null_result_is_ok() {
        let (client, mut peer) = BridgeClient::duplex_pair(64 * 1024);
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(&mut peer);
            let req = crate::ipc::mod_types::read_request(&mut reader)
                .await
                .expect("read")
                .expect("eof");
            assert_eq!(req.method, "windows.find");
            let resp = WireResponse::ok(req.id, Value::Null);
            write_response(&mut peer, &resp).await.expect("write");
        });
        let result = client
            .call("windows.find", json!({ "title": "x" }))
            .await
            .expect("null result must succeed");
        assert!(result.is_null());
        server.await.expect("server");
    }

    #[test]
    fn resolve_binary_fails_loud_when_missing() {
        let _guard = BRIDGE_ENV_LOCK.blocking_lock();
        let prev = std::env::var("PLAYCUA_BRIDGE_BIN").ok();
        std::env::set_var("PLAYCUA_BRIDGE_BIN", "/nonexistent/playcua-bridge-xyz");
        let err = BridgeClient::resolve_binary().expect_err("must fail");
        assert!(matches!(err, BridgeError::BinaryMissing(_)));
        match prev {
            Some(v) => std::env::set_var("PLAYCUA_BRIDGE_BIN", v),
            None => std::env::remove_var("PLAYCUA_BRIDGE_BIN"),
        }
    }
}
