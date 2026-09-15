//! Window enumeration, focus, and search — platform dispatch.

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Metadata about a top-level window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub hwnd: i64,
    pub title: String,
    pub pid: u32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// RPC handlers
// ---------------------------------------------------------------------------

pub async fn list_rpc(_params: Value) -> Result<Value> {
    let windows = platform_list().await?;
    Ok(serde_json::to_value(windows)?)
}

pub async fn focus_rpc(params: Value) -> Result<Value> {
    #[derive(Deserialize)]
    struct FocusParams { hwnd: i64 }
    let p: FocusParams = serde_json::from_value(params)?;
    platform_focus(p.hwnd).await?;
    Ok(json!({ "ok": true }))
}

pub async fn find_rpc(params: Value) -> Result<Value> {
    #[derive(Deserialize)]
    struct FindParams {
        title: Option<String>,
        pid: Option<u32>,
    }
    let p: FindParams = serde_json::from_value(params)?;
    let result = platform_find(p.title, p.pid).await?;
    match result {
        Some(w) => Ok(serde_json::to_value(w)?),
        None => Ok(Value::Null),
    }
}

// ---------------------------------------------------------------------------
// Platform dispatch
// ---------------------------------------------------------------------------

async fn platform_list() -> Result<Vec<WindowInfo>> {
    #[cfg(target_os = "windows")]
    return windows::list_windows().await;
    #[cfg(target_os = "linux")]
    return linux::list_windows().await;
    #[cfg(target_os = "macos")]
    return macos::list_windows().await;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    anyhow::bail!("windows.list not supported on this platform")
}

async fn platform_focus(hwnd: i64) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows::focus_window(hwnd).await;
    #[cfg(target_os = "linux")]
    return linux::focus_window(hwnd).await;
    #[cfg(target_os = "macos")]
    return macos::focus_window(hwnd).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    anyhow::bail!("windows.focus not supported on this platform")
}

async fn platform_find(title: Option<String>, pid: Option<u32>) -> Result<Option<WindowInfo>> {
    let all = platform_list().await?;
    let found = all.into_iter().find(|w| {
        let title_match = title.as_ref().map_or(true, |t| {
            w.title.to_lowercase().contains(&t.to_lowercase())
        });
        let pid_match = pid.map_or(true, |p| w.pid == p);
        title_match && pid_match
    });
    Ok(found)
}
