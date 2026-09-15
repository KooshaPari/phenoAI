//! `playcua-cli` — scriptable shell client for the playcua-native daemon.
//!
//! Subcommands wrap the 14 JSON-RPC methods into ergonomic shell verbs:
//!   playcua-cli ping
//!   playcua-cli screenshot > shot.png
//!   playcua-cli click 100 200
//!   playcua-cli type "hello world"
//!   playcua-cli key Return
//!   playcua-cli run --path /bin/echo --args hello --args world
//!   playcua-cli ps kill 1234
//!
//! Transport: spawns the daemon as a subprocess and talks newline-delimited
//! JSON-RPC 2.0 over its stdio. No socket, no daemonization — just `pipe`.
//! This keeps the CLI stateless, scriptable, and `nix`-friendly (works
//! inside `xargs`, `parallel`, `for f in ...; do` loops, etc.).
//!
//! Exit codes: 0 on success, 1 on JSON-RPC error, 2 on transport error.

use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

#[derive(Parser, Debug)]
#[command(
    name = "playcua-cli",
    version,
    about = "Shell client for playcua-native"
)]
struct Cli {
    /// Path to the playcua-native binary. Defaults to $PLAYCUA_NATIVE
    /// or `playcua-native` on $PATH.
    #[arg(long, env = "PLAYCUA_NATIVE", default_value = "playcua-native")]
    daemon: PathBuf,

    /// Path to a Unix-socket daemon started with `playcua-native --socket <path>`.
    /// If set, the CLI connects to the running daemon instead of spawning
    /// a fresh subprocess per call. This is dramatically faster for tight
    /// loops (for, xargs, parallel, etc.). Set via flag or $PLAYCUA_SOCKET.
    #[arg(long, env = "PLAYCUA_SOCKET")]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Ping the daemon (round-trip health check).
    Ping,
    /// Capture the screen (or a window) and write base64-encoded PNG to stdout.
    Screenshot {
        /// Display index; omit for primary.
        #[arg(long)]
        display: Option<u32>,
        /// Substring of the window title to capture.
        #[arg(long)]
        window: Option<String>,
    },
    /// Click (or press/release) a mouse button at (x, y).
    Click {
        x: i32,
        y: i32,
        /// left | right | middle (default: left)
        #[arg(long, default_value = "left")]
        button: String,
        /// click | down | up (default: click)
        #[arg(long, default_value = "click")]
        action: String,
    },
    /// Move the mouse to (x, y).
    Move { x: i32, y: i32 },
    /// Scroll the wheel at (x, y).
    Scroll {
        x: i32,
        y: i32,
        /// up | down | left | right (default: down)
        #[arg(long, default_value = "down")]
        direction: String,
        /// Wheel notch count (default: 3).
        #[arg(long, default_value_t = 3)]
        amount: i32,
    },
    /// Press, hold, or release a key or chord.
    Key {
        /// Key name, e.g. "Return", "ctrl+c", "shift+End".
        key: String,
        /// press | down | up (default: press)
        #[arg(long, default_value = "press")]
        action: String,
    },
    /// Type a literal string of text.
    Type {
        /// Text to type. Reads from stdin if "--" sentinel.
        text: String,
    },
    /// List all visible top-level windows (newline-delimited JSON).
    Windows,
    /// Find a window by title substring and/or owner PID.
    WindowsFind {
        /// Substring of the window title.
        #[arg(long)]
        title: Option<String>,
        /// Owner process PID.
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Bring a window to the foreground.
    WindowsFocus {
        /// Platform-specific window handle.
        hwnd: usize,
    },
    /// Launch a process and print its PID.
    Run {
        /// Path to the executable.
        #[arg(long)]
        path: String,
        /// argv entries (excluding argv[0]). Repeatable.
        #[arg(long, value_delimiter = ' ')]
        args: Vec<String>,
        /// Optional working directory.
        #[arg(long)]
        cwd: Option<String>,
    },
    /// Terminate a process by PID.
    Kill {
        /// Process ID.
        pid: u32,
    },
    /// Query process status.
    Status {
        /// Process ID.
        pid: u32,
    },
    /// Compute the fraction of differing pixels between two PNG files.
    Diff {
        /// Path to first PNG.
        a: PathBuf,
        /// Path to second PNG.
        b: PathBuf,
        /// Pixel-difference threshold [0.0, 1.0]. Default 0.02.
        #[arg(long, default_value_t = 0.02)]
        threshold: f32,
    },
    /// Compute a BLAKE3 hash of a PNG (for change detection).
    Hash {
        /// Path to PNG.
        image: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let (method, params) = match &cli.cmd {
        Cmd::Ping => ("ping".to_string(), Value::Null),
        Cmd::Screenshot { display, window } => (
            "screenshot".to_string(),
            json!({ "display": display, "window_title": window }),
        ),
        Cmd::Click {
            x,
            y,
            button,
            action,
        } => (
            "input.click".to_string(),
            json!({ "x": x, "y": y, "button": button, "action": action }),
        ),
        Cmd::Move { x, y } => ("input.move".to_string(), json!({ "x": x, "y": y })),
        Cmd::Scroll {
            x,
            y,
            direction,
            amount,
        } => (
            "input.scroll".to_string(),
            json!({ "x": x, "y": y, "direction": direction, "amount": amount }),
        ),
        Cmd::Key { key, action } => (
            "input.key".to_string(),
            json!({ "key": key, "action": action }),
        ),
        Cmd::Type { text } => ("input.type".to_string(), json!({ "text": text })),
        Cmd::Windows => ("windows.list".to_string(), Value::Null),
        Cmd::WindowsFind { title, pid } => (
            "windows.find".to_string(),
            json!({ "title": title, "pid": pid }),
        ),
        Cmd::WindowsFocus { hwnd } => ("windows.focus".to_string(), json!({ "hwnd": hwnd })),
        Cmd::Run { path, args, cwd } => (
            "process.launch".to_string(),
            json!({ "path": path, "args": args, "cwd": cwd }),
        ),
        Cmd::Kill { pid } => ("process.kill".to_string(), json!({ "pid": pid })),
        Cmd::Status { pid } => ("process.status".to_string(), json!({ "pid": pid })),
        Cmd::Diff { a, b, threshold } => {
            let a_b64 = base64_encode_file(a).await?;
            let b_b64 = base64_encode_file(b).await?;
            (
                "analysis.diff".to_string(),
                json!({ "image_a": a_b64, "image_b": b_b64, "threshold": threshold }),
            )
        }
        Cmd::Hash { image } => {
            let image_b64 = base64_encode_file(image).await?;
            ("analysis.hash".to_string(), json!({ "image": image_b64 }))
        }
    };

    let (code, json) = match &cli.socket {
        #[cfg(unix)]
        Some(socket_path) => socket_call(socket_path, &method, params).await?,
        #[cfg(not(unix))]
        Some(_) => anyhow::bail!("--socket is only supported on Unix"),
        None => daemon_call(&cli.daemon, &method, params).await?,
    };
    if code != 0 {
        let msg = json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("unknown RPC error");
        CliError::rpc(code, msg).exit();
    }
    let result = json.get("result").cloned().unwrap_or(Value::Null);
    // Print compact JSON for non-screenshot cases; for screenshot, write the
    // raw base64 PNG bytes to stdout (decoded) so the shell can pipe it
    // directly to `> shot.png`.
    match &cli.cmd {
        Cmd::Screenshot { .. } => {
            if let Some(data) = result.get("data").and_then(|v| v.as_str()) {
                use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
                let bytes = BASE64
                    .decode(data)
                    .context("screenshot base64 decode failed")?;
                std::io::stdout().write_all(&bytes)?;
            } else {
                CliError::rpc(
                    -32603,
                    &format!("screenshot response missing `data` field: {result}"),
                )
                .exit();
            }
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

/// Connect to a Unix-socket daemon, send one request, read one response.
#[cfg(unix)]
async fn socket_call(socket_path: &PathBuf, method: &str, params: Value) -> Result<(i32, Value)> {
    use tokio::io::AsyncBufReadExt;
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("connecting to daemon at {}", socket_path.display()))?;
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    write_half.write_all(line.as_bytes()).await?;
    write_half.flush().await?;
    drop(write_half); // half-close so the daemon sees EOF and can answer

    let mut resp = String::new();
    reader.read_line(&mut resp).await?;
    let trimmed = resp.trim();
    if trimmed.is_empty() {
        anyhow::bail!("daemon closed connection without responding");
    }
    let parsed: Value = serde_json::from_str(trimmed)
        .with_context(|| format!("daemon sent non-JSON: {trimmed}"))?;
    let code = parsed
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
        .unwrap_or(0);
    Ok((code, parsed))
}

/// Spawn the daemon, send one request, read one response, kill the daemon.
async fn daemon_call(daemon: &PathBuf, method: &str, params: Value) -> Result<(i32, Value)> {
    let mut child = Command::new(daemon)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", daemon.display()))?;

    let mut stdin = child.stdin.take().context("missing daemon stdin")?;
    let stdout = child.stdout.take().context("missing daemon stdout")?;
    let mut reader = BufReader::new(stdout);

    let req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let mut line = serde_json::to_string(&req)?;
    line.push('\n');
    stdin.write_all(line.as_bytes()).await?;
    stdin.flush().await?;
    drop(stdin); // close stdin so the daemon exits after responding

    let mut resp = String::new();
    reader.read_line(&mut resp).await?;
    let trimmed = resp.trim();
    if trimmed.is_empty() {
        let status = child.wait().await?;
        anyhow::bail!("daemon closed stdout (exit code {:?})", status.code());
    }
    let parsed: Value = serde_json::from_str(trimmed)
        .with_context(|| format!("daemon sent non-JSON: {trimmed}"))?;
    let code = parsed
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
        .unwrap_or(0);
    let _ = child.kill().await; // daemon would exit on stdin EOF anyway
    Ok((code, parsed))
}

async fn base64_encode_file(path: &PathBuf) -> Result<String> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("read {} failed", path.display()))?;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    Ok(BASE64.encode(&bytes))
}

// ---------------------------------------------------------------------------
// Structured CLI error (L14 remediation: typed client-side error envelope)
// ---------------------------------------------------------------------------

/// Structured CLI error that prints a well-formed JSON-RPC error to stderr
/// and exits with the corresponding code.
///
/// This replaces the previous `eprintln!("RPC error: {err}")` pattern so that
/// tooling consuming stderr can parse structured error payloads instead of
/// free-form text.
#[derive(Debug, Serialize)]
struct CliError {
    code: i32,
    message: String,
}

impl CliError {
    /// Construct a JSON-RPC-style error.
    fn rpc(code: i32, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }

    /// Print the error as a one-line JSON object to stderr, then exit(1).
    fn exit(&self) -> ! {
        let payload = json!({
            "jsonrpc": "2.0",
            "error": {
                "code": self.code,
                "message": self.message,
            }
        });
        let line = serde_json::to_string(&payload).unwrap_or_else(|_| {
            format!(
                r#"{{"jsonrpc":"2.0","error":{{"code":{},"message":"{}"}}}}"#,
                self.code, self.message
            )
        });
        eprintln!("{line}");
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_error_constructs_rpc_error() {
        let err = CliError::rpc(-32603, "test error");
        assert_eq!(err.code, -32603);
        assert_eq!(err.message, "test error");
    }

    #[test]
    fn cli_error_serializes_to_json() {
        let err = CliError::rpc(-32700, "parse error");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["code"], -32700);
        assert_eq!(json["message"], "parse error");
    }

    #[test]
    fn cli_error_json_rpc_format() {
        // Verify the error produces valid JSON-RPC shaped output.
        let err = CliError::rpc(-32601, "method not found");
        let payload = json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32601,
                "message": "method not found",
            }
        });
        assert_eq!(serde_json::to_value(&err).unwrap(), payload["error"]);
    }
}
