pub mod cli;
pub mod config;
pub mod error;
pub mod mcp;
pub mod project;
pub mod testing;
pub mod utils;

// Legacy modules (kept for compatibility)
pub mod device_basic;
pub mod simulator_basic;

// Advanced modules from desktop integration
pub mod device_bridge;
pub mod hardware_emulator;

// Desktop-specific modules (feature-gated)
#[cfg(feature = "desktop")]
pub mod desktop {
    pub mod android_studio_integration;
    pub mod app;
    pub mod audio;
    pub mod computer_vision;
    pub mod ui;
    pub mod xcode_integration;

    pub use super::device_bridge::DeviceBridge;
    pub use super::hardware_emulator::HardwareEmulator;
    pub use app::{Args, KMobileDesktopApp};
}

pub use cli::KMobileCli;
pub use config::Config;
pub use error::{KMobileError, Result};
pub use mcp::{McpRequest, McpResponse, McpServer};

// Re-export manager / runner types so the `kmobile-mcp` adapter (and any
// other consumer outside the workspace) can name them as `kmobile::Foo`
// rather than reaching into the legacy `*_basic` modules directly.
pub use device_basic::DeviceManager;
pub use project::ProjectManager;
pub use simulator_basic::SimulatorManager;
pub use testing::TestRunner;

// Re-export advanced modules as primary device/hardware interfaces
pub use device_bridge as device;
pub use hardware_emulator as simulator;
