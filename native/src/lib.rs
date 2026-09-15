//! playcua-native library entry point.
//!
//! Re-exports all public modules so integration tests and external crates
//! can import domain types, port traits, and adapter implementations.

pub mod adapters;
pub mod app;
pub mod domain;
pub mod ipc;
#[cfg(feature = "mcp-server")]
pub mod mcp_server;
pub mod modality;
pub mod plugins;
pub mod ports;
#[cfg(unix)]
pub mod socket;
