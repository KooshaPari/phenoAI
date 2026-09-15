//! NSWorkspaceAdapter — macOS window enumeration via xcap.
//! Implements WindowPort for macOS.
//!
//! Full NSWorkspace/AppleScript window focus is a TODO;
//! enumeration is covered by xcap.

use crate::domain::window::{WindowError, WindowFilter, WindowInfo};
use crate::ports::WindowPort;
use async_trait::async_trait;
use tracing::{instrument, warn};

pub struct NSWorkspaceAdapter;

impl NSWorkspaceAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NSWorkspaceAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WindowPort for NSWorkspaceAdapter {
    #[instrument(name = "nsworkspace.list_windows", skip(self))]
    async fn list_windows(&self) -> Result<Vec<WindowInfo>, WindowError> {
        tokio::task::spawn_blocking(|| -> Result<Vec<WindowInfo>, WindowError> {
            let windows =
                xcap::Window::all().map_err(|e| WindowError::EnumerationFailed(e.to_string()))?;
            Ok(windows
                .into_iter()
                .map(|w| {
                    let title = w.title().ok().unwrap_or_default();
                    let x = w.x().ok().unwrap_or(0);
                    let y = w.y().ok().unwrap_or(0);
                    let width = w.width().ok().unwrap_or(0) as i32;
                    let height = w.height().ok().unwrap_or(0) as i32;
                    WindowInfo {
                        hwnd: w.id().ok().unwrap_or(0) as usize,
                        title,
                        pid: w.pid().ok().unwrap_or(0),
                        x,
                        y,
                        width,
                        height,
                        visible: true,
                    }
                })
                .collect())
        })
        .await
        .map_err(|e| WindowError::Failed(format!("spawn_blocking panic: {e}")))?
    }

    #[instrument(name = "nsworkspace.find_window", skip(self))]
    async fn find_window(&self, filter: WindowFilter) -> Result<Option<WindowInfo>, WindowError> {
        let all = self.list_windows().await?;
        let found = all.into_iter().find(|w| {
            let title_lower = filter.title.as_ref().map(|t| t.to_lowercase());
            let pid_match = filter.pid.map_or(true, |pid| w.pid == pid);
            let title_ok = title_lower
                .as_ref()
                .map_or(true, |t| w.title.to_lowercase().contains(t));
            title_ok && pid_match
        });
        Ok(found)
    }

    #[instrument(name = "nsworkspace.focus_window", skip(self))]
    async fn focus_window(&self, _hwnd: usize) -> Result<(), WindowError> {
        // TODO: implement using NSWorkspace / AppleScript activate.
        warn!("windows.focus: not yet implemented on macOS (stub)");
        Ok(())
    }
}
