//! Unix-socket daemon mode for `playcua-native`.
//!
//! When the user passes `--socket <path>` to `playcua-native`, the daemon
//! binds a Unix-domain stream socket at that path and accepts concurrent
//! client connections, each of which gets the same JSON-RPC 2.0 / stdio
//! protocol as the stdio mode.
//!
//! Why: `playcua-cli` spawns `playcua-native` per call today. For tight
//! loops (`for i in $(seq 1 1000); do playcua-cli click $i $i; done`) the
//! process-fork cost dominates. The Unix-socket daemon eliminates that:
//! the CLI connects, sends one newline-delimited JSON-RPC request, reads
//! the response, and disconnects — no fork, no cold start.
//!
//! The dispatcher holds `Arc<dyn Port>` references, so it's cheaply
//! cloneable and `Send + Sync`, which means we can share one `App`
//! across N connection handlers via a single `Arc<App>`.
//!
//! Concurrency model: one accept loop + one task per connection. Each
//! connection's tasks terminate cleanly when the client disconnects or
//! EOFs. The accept loop terminates on signal (Ctrl-C) or fatal
//! accept-error.
//!
//! Platform: this module is `cfg(unix)` because Tokio's `UnixListener`
//! is Unix-only. Windows support would use `tokio::net::windows::named_pipe`
//! and is out of scope for this slice.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

use crate::app::App;
use crate::ipc::{read_request, write_response};

/// Run the Unix-socket daemon, accepting connections until the listener
/// is shut down (Ctrl-C) or a fatal error occurs.
///
/// `socket_path` is created (and any existing socket file at that path
/// is removed first) before binding. The socket file is removed on
/// drop via `OwnedFd`-style semantics — Tokio's `UnixListener` keeps
/// the file alive for the lifetime of the listener.
pub async fn run(app: Arc<App>, socket_path: PathBuf) -> Result<()> {
    // Remove any stale socket file from a previous run. Common when the
    // previous daemon was killed with SIGKILL.
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)
            .with_context(|| format!("removing stale socket at {}", socket_path.display()))?;
    }

    // Ensure parent dir exists (XDG_RUNTIME_DIR is usually pre-created,
    // but a user-supplied path may need mkdir -p).
    if let Some(parent) = socket_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir {}", parent.display()))?;
        }
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("binding Unix socket at {}", socket_path.display()))?;

    info!(socket = %socket_path.display(), "playcua daemon listening on Unix socket");

    // Clean up the socket file on graceful shutdown. The DeferCleanup
    // destructor also removes it; this handles the Ctrl-C case.
    let _cleanup = DeferCleanup {
        path: socket_path.clone(),
    };

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _addr)) => {
                        let app = app.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(app, stream).await {
                                debug!(error = %e, "connection ended with error");
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "accept error; shutting down");
                        break;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received, shutting down daemon");
                break;
            }
        }
    }

    // Explicit cleanup (DeferCleanup would also handle it, but the loop
    // break above drops `_cleanup` which races with us — explicit is safer).
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

/// Handle a single client connection: read newline-delimited JSON-RPC
/// requests, dispatch them, write responses. Closes on EOF or error.
async fn handle_connection(app: Arc<App>, stream: UnixStream) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    loop {
        let req = match read_request(&mut reader).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                debug!("client EOF");
                return Ok(());
            }
            Err(e) => {
                warn!(error = %e, "client parse error");
                let resp = crate::ipc::Response::err(
                    serde_json::Value::Null,
                    -32700,
                    format!("Parse error: {e}"),
                );
                let _ = write_response(&mut write_half, &resp).await;
                continue;
            }
        };

        let resp = app.dispatcher.dispatch(req).await;

        if let Err(e) = write_response(&mut write_half, &resp).await {
            warn!(error = %e, "client write error");
            return Ok(());
        }

        // Flush after every response so the client can `read(2)` promptly.
        write_half.flush().await.ok();
    }
}

/// RAII guard that removes the socket file on drop. Belt-and-suspenders
/// alongside the explicit `std::fs::remove_file` in `run`.
struct DeferCleanup {
    path: PathBuf,
}

impl Drop for DeferCleanup {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: DeferCleanup removes the file it points at on drop.
    #[test]
    fn defer_cleanup_removes_file() {
        let dir = tempdir_in_target();
        let path = dir.join("test.sock");
        std::fs::write(&path, b"").unwrap();
        assert!(path.exists());
        drop(DeferCleanup { path: path.clone() });
        assert!(
            !path.exists(),
            "DeferCleanup should have removed {}",
            path.display()
        );
    }

    fn tempdir_in_target() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "playcua-socket-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
