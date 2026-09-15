//! Device management port.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Device information returned by the port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: String,
}

/// Device management port — implemented by the concrete adapter.
#[async_trait]
pub trait DevicePort: Send + Sync {
    /// List all connected devices.
    async fn list_devices(&self) -> anyhow::Result<Vec<DeviceInfo>>;
    /// Connect to a device by ID.
    async fn connect_device(&self, id: &str) -> anyhow::Result<()>;
    /// Install an app on a device.
    async fn install_app(&self, id: &str, app: &str) -> anyhow::Result<()>;
    /// Deploy a project to a device.
    async fn deploy_project(&self, id: &str, project: Option<&str>) -> anyhow::Result<()>;
    /// Run tests on a device.
    async fn run_device_tests(&self, id: &str, suite: Option<&str>) -> anyhow::Result<()>;
}
