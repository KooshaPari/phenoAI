//! Testing automation port.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Test result returned by the port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub suite: String,
    pub passed: usize,
    pub failed: usize,
}

/// Testing automation port — implemented by the concrete adapter.
#[async_trait]
pub trait TestingPort: Send + Sync {
    /// Run tests with optional suite and device.
    async fn run_tests(
        &self,
        suite: Option<&str>,
        device: Option<&str>,
    ) -> anyhow::Result<TestResult>;
    /// Record a test to a file.
    async fn record_test(&self, output: &str) -> anyhow::Result<()>;
    /// Replay a test from a file.
    async fn replay_test(&self, file: &str) -> anyhow::Result<()>;
    /// Run tests on a specific device.
    async fn run_device_tests(&self, id: &str, suite: Option<&str>) -> anyhow::Result<TestResult>;
}
