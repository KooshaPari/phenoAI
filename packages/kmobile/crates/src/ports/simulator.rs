//! Simulator management port.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Simulator information returned by the port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorInfo {
    pub id: String,
    pub name: String,
    pub platform: String,
}

/// Simulator management port — implemented by the concrete adapter.
#[async_trait]
pub trait SimulatorPort: Send + Sync {
    /// List all available simulators.
    async fn list_simulators(&self) -> anyhow::Result<Vec<SimulatorInfo>>;
    /// Start a simulator by ID.
    async fn start_simulator(&self, id: &str) -> anyhow::Result<()>;
    /// Stop a simulator by ID.
    async fn stop_simulator(&self, id: &str) -> anyhow::Result<()>;
    /// Reset a simulator by ID.
    async fn reset_simulator(&self, id: &str) -> anyhow::Result<()>;
    /// Install an app on a simulator.
    async fn install_app(&self, id: &str, app: &str) -> anyhow::Result<()>;
}
