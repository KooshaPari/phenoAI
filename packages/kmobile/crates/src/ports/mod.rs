//! Hexagonal port traits for KMobile.
//!
//! Every adapter — CLI, API, MCP, TUI — works through these traits.

pub mod device;
pub mod material;
pub mod mcp;
pub mod plugin;
pub mod project;
pub mod serialization;
pub mod simulator;
pub mod testing;

pub use device::{DeviceInfo, DevicePort};
pub use material::{
    AssetInfo, AssetKind, InMemoryMaterialPort, MaterialPort, MockMaterialCall, MockMaterialPort,
    MutableInMemoryMaterialPort,
};
pub use mcp::{
    InMemoryMcpPort, McpInvocationResult, McpPort, McpToolInfo, McpToolKind, MockMcpCall,
    MockMcpPort,
};
pub use plugin::{
    InMemoryPluginPort, MockPluginCall, MockPluginPort, PluginCapability, PluginInfo, PluginPort,
    PluginSource, PluginState,
};
pub use project::{ProjectPort, ProjectStatus};
pub use serialization::{JsonFileSerializer, MockSerializationPort, SerializationPort};
pub use simulator::{SimulatorInfo, SimulatorPort};
pub use testing::{TestResult, TestingPort};
