//! Project management port.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Project status returned by the port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStatus {
    pub name: String,
    pub state: String,
}

/// Project management port — implemented by the concrete adapter.
#[async_trait]
pub trait ProjectPort: Send + Sync {
    /// Initialize a new project.
    async fn init_project(&self, name: &str, template: Option<&str>) -> anyhow::Result<()>;
    /// Build the current project.
    async fn build_project(&self, target: Option<&str>) -> anyhow::Result<()>;
    /// Clean the current project.
    async fn clean_project(&self) -> anyhow::Result<()>;
    /// Get the current project status.
    async fn get_project_status(&self) -> anyhow::Result<ProjectStatus>;
}
