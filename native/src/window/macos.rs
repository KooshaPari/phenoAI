//! macOS window enumeration via xcap.

use super::WindowInfo;
use anyhow::{Context, Result};

pub async fn list_windows() -> Result<Vec<WindowInfo>> {
    tokio::task::spawn_blocking(|| -> Result<Vec<WindowInfo>> {
        let windows = xcap::Window::all().context("xcap: failed to enumerate windows")?;
        Ok(windows
            .into_iter()
            .map(|w| WindowInfo {
                hwnd: w.id() as i64,
                title: w.title().to_string(),
                pid: w.pid(),
                x: w.x(),
                y: w.y(),
                width: w.width() as i32,
                height: w.height() as i32,
                visible: true,
            })
            .collect())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn focus_window(_hwnd: i64) -> Result<()> {
    // TODO: use NSApplication/AppleScript to focus window by ID.
    tracing::warn!("windows.focus: not yet implemented on macOS (stub)");
    Ok(())
}
