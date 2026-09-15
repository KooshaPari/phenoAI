//! Adapters layer — concrete implementations of port traits.
//!
//! Cross-platform adapters are always compiled. Platform-specific adapters
//! are gated with cfg(target_os = ...).

pub mod analysis_adapter;
pub mod enigo;
pub mod process_adapter;
pub mod sandbox;
pub mod sandbox_bridge;
pub mod tokio;
pub mod xcap;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;
