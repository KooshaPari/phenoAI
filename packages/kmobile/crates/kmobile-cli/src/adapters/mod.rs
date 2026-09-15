//! Concrete adapter implementations for the KMobile core ports.
//!
//! Each adapter in this module is the CLI-side wiring of a hexagonal port
//! defined in `kmobile_core::ports`. The domain depends only on the trait;
//! this crate provides the actual implementation that talks to
//! `adb`, `xcrun simctl`, `xcodebuild`, `gradle`, `flutter`, the local
//! filesystem, and so on.

pub mod device;
pub mod material;
pub mod mcp;
pub mod plugin;
pub mod project;
pub mod serialization;
pub mod simulator;
pub mod testing;

pub use device::CliDeviceAdapter;
pub use material::CliMaterialAdapter;
pub use mcp::CliMcpAdapter;
pub use plugin::CliPluginAdapter;
pub use project::CliProjectAdapter;
pub use serialization::CliSerializationAdapter;
pub use simulator::CliSimulatorAdapter;
pub use testing::CliTestingAdapter;

/// Shared error conversion helper.
///
/// CLI adapters return `anyhow::Result` from their port methods. The core
/// error type (`KMobileError`) is converted from `std::io::Error`,
/// `serde_json::Error`, etc. via the `From` impls in `kmobile_core::error`.
/// This alias keeps the call sites compact.
#[expect(dead_code)]
pub type Result<T> = anyhow::Result<T>;
