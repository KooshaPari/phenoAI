//! Linux window enumeration via xcap.
//! Full x11rb implementation is a TODO — xcap covers the common case.

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
                visible: true, // xcap only returns visible windows
            })
            .collect())
    })
    .await
    .context("spawn_blocking panicked")?
}

pub async fn focus_window(_hwnd: i64) -> Result<()> {
    // TODO: use x11rb to raise/focus the window.
    // xcap does not expose a focus API.
    tracing::warn!("windows.focus: not yet implemented on Linux (stub)");
    Ok(())
}
