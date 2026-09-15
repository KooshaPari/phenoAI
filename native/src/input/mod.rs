//! Input injection — keyboard, mouse, scroll.
//! Platform dispatch to OS-specific implementations.

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};

/// Key action variant.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyAction {
    Press,
    Down,
    Up,
}

/// Mouse button variant.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Mouse action variant.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseAction {
    Click,
    Down,
    Up,
}

/// Scroll direction.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Params for `input.key`.
#[derive(Debug, Deserialize)]
pub struct KeyParams {
    pub key: String,
    pub action: KeyAction,
}

/// Params for `input.type`.
#[derive(Debug, Deserialize)]
pub struct TypeParams {
    pub text: String,
}

/// Params for `input.click`.
#[derive(Debug, Deserialize)]
pub struct ClickParams {
    pub x: i32,
    pub y: i32,
    pub button: MouseButton,
    pub action: MouseAction,
}

/// Params for `input.scroll`.
#[derive(Debug, Deserialize)]
pub struct ScrollParams {
    pub x: i32,
    pub y: i32,
    pub direction: ScrollDirection,
    pub amount: Option<i32>,
}

/// Params for `input.move`.
#[derive(Debug, Deserialize)]
pub struct MoveParams {
    pub x: i32,
    pub y: i32,
}

/// RPC handler for `input.key`.
pub async fn key_rpc(params: Value) -> Result<Value> {
    let p: KeyParams = serde_json::from_value(params)?;
    platform_key(p).await?;
    Ok(json!({ "ok": true }))
}

/// RPC handler for `input.type`.
pub async fn type_rpc(params: Value) -> Result<Value> {
    let p: TypeParams = serde_json::from_value(params)?;
    platform_type(p).await?;
    Ok(json!({ "ok": true }))
}

/// RPC handler for `input.click`.
pub async fn click_rpc(params: Value) -> Result<Value> {
    let p: ClickParams = serde_json::from_value(params)?;
    platform_click(p).await?;
    Ok(json!({ "ok": true }))
}

/// RPC handler for `input.scroll`.
pub async fn scroll_rpc(params: Value) -> Result<Value> {
    let p: ScrollParams = serde_json::from_value(params)?;
    platform_scroll(p).await?;
    Ok(json!({ "ok": true }))
}

/// RPC handler for `input.move`.
pub async fn move_rpc(params: Value) -> Result<Value> {
    let p: MoveParams = serde_json::from_value(params)?;
    platform_move(p).await?;
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Platform dispatch
// ---------------------------------------------------------------------------

async fn platform_key(p: KeyParams) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows::key(p).await;
    #[cfg(target_os = "linux")]
    return linux::key(p).await;
    #[cfg(target_os = "macos")]
    return macos::key(p).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    anyhow::bail!("input.key not supported on this platform")
}

async fn platform_type(p: TypeParams) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows::type_text(p).await;
    #[cfg(target_os = "linux")]
    return linux::type_text(p).await;
    #[cfg(target_os = "macos")]
    return macos::type_text(p).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    anyhow::bail!("input.type not supported on this platform")
}

async fn platform_click(p: ClickParams) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows::click(p).await;
    #[cfg(target_os = "linux")]
    return linux::click(p).await;
    #[cfg(target_os = "macos")]
    return macos::click(p).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    anyhow::bail!("input.click not supported on this platform")
}

async fn platform_scroll(p: ScrollParams) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows::scroll(p).await;
    #[cfg(target_os = "linux")]
    return linux::scroll(p).await;
    #[cfg(target_os = "macos")]
    return macos::scroll(p).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    anyhow::bail!("input.scroll not supported on this platform")
}

async fn platform_move(p: MoveParams) -> Result<()> {
    #[cfg(target_os = "windows")]
    return windows::move_mouse(p).await;
    #[cfg(target_os = "linux")]
    return linux::move_mouse(p).await;
    #[cfg(target_os = "macos")]
    return macos::move_mouse(p).await;
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    anyhow::bail!("input.move not supported on this platform")
}
