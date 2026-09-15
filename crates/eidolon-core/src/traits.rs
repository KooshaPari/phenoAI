use crate::{AutomationEvent, Result, Viewport};

/// Desktop automation trait.
/// Implemented by: macOS (native), Windows (native), Linux (X11/Wayland).
#[async_trait::async_trait]
pub trait DesktopAutomator: Send + Sync {
    /// Get current viewport dimensions.
    async fn get_viewport(&self) -> Result<Viewport>;

    /// Take a screenshot.
    async fn screenshot(&self, path: &str) -> Result<()>;

    /// Execute pointer input.
    async fn pointer(&self, event: &crate::input::PointerInput) -> Result<()>;

    /// Execute text input.
    async fn text(&self, event: &crate::input::TextInput) -> Result<()>;

    /// Record automation event for audit log.
    async fn record_event(&self, event: AutomationEvent) -> Result<()>;
}

/// Mobile automation trait.
/// Implemented by: iOS (via XCTest), Android (via UiAutomator).
#[async_trait::async_trait]
pub trait MobileAutomator: Send + Sync {
    /// Get current viewport (screen dimensions).
    async fn get_viewport(&self) -> Result<Viewport>;

    /// Take a screenshot.
    async fn screenshot(&self, path: &str) -> Result<()>;

    /// Tap screen at coordinates.
    async fn tap(&self, x: i32, y: i32) -> Result<()>;

    /// Swipe from (x1, y1) to (x2, y2).
    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()>;

    /// Input text.
    async fn input_text(&self, text: &str) -> Result<()>;

    /// Record automation event for audit log.
    async fn record_event(&self, event: AutomationEvent) -> Result<()>;
}

/// Sandbox / container automation trait.
/// Implemented by: nanoVMs, Docker, Firecracker, KVM VMs.
#[async_trait::async_trait]
pub trait SandboxAutomator: Send + Sync {
    /// Get sandbox metadata (image, resource limits).
    async fn get_metadata(&self) -> Result<SandboxMetadata>;

    /// Start the sandbox.
    async fn start(&self) -> Result<()>;

    /// Stop the sandbox.
    async fn stop(&self) -> Result<()>;

    /// Execute command inside sandbox.
    async fn exec(&self, cmd: &str) -> Result<String>;

    /// Get current resource usage (CPU, memory, disk).
    async fn resource_usage(&self) -> Result<ResourceUsage>;

    /// Record automation event for audit log.
    async fn record_event(&self, event: AutomationEvent) -> Result<()>;
}

/// Sandbox metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SandboxMetadata {
    pub id: String,
    pub image: String,
    pub cpu_limit: u32,
    pub memory_limit_mb: u32,
    pub disk_limit_mb: Option<u32>,
}

/// Resource usage snapshot.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_mb: u32,
    pub disk_mb: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_usage_zero_values() {
        let usage = ResourceUsage {
            cpu_percent: 0.0,
            memory_mb: 0,
            disk_mb: Some(0),
        };
        assert_eq!(usage.cpu_percent, 0.0);
        assert_eq!(usage.memory_mb, 0);
        assert_eq!(usage.disk_mb, Some(0));
    }

    #[test]
    fn resource_usage_none_disk() {
        let usage = ResourceUsage {
            cpu_percent: 42.5,
            memory_mb: 1024,
            disk_mb: None,
        };
        assert_eq!(usage.cpu_percent, 42.5);
        assert_eq!(usage.memory_mb, 1024);
        assert!(usage.disk_mb.is_none());
    }

    #[test]
    fn sandbox_metadata_serde_round_trip() {
        let meta = SandboxMetadata {
            id: "test-sandbox-1".into(),
            image: "alpine:3.19".into(),
            cpu_limit: 2,
            memory_limit_mb: 512,
            disk_limit_mb: Some(2048),
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        let decoded: SandboxMetadata = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.id, "test-sandbox-1");
        assert_eq!(decoded.image, "alpine:3.19");
        assert_eq!(decoded.cpu_limit, 2);
        assert_eq!(decoded.memory_limit_mb, 512);
        assert_eq!(decoded.disk_limit_mb, Some(2048));
    }

    #[test]
    fn sandbox_metadata_none_disk_limit() {
        let meta = SandboxMetadata {
            id: "s1".into(),
            image: "ubuntu:22.04".into(),
            cpu_limit: 4,
            memory_limit_mb: 2048,
            disk_limit_mb: None,
        };
        let json = serde_json::to_string(&meta).expect("serialize");
        let decoded: SandboxMetadata = serde_json::from_str(&json).expect("deserialize");
        assert!(decoded.disk_limit_mb.is_none());
    }

    #[test]
    fn resource_usage_serde_round_trip() {
        let usage = ResourceUsage {
            cpu_percent: 73.2,
            memory_mb: 256,
            disk_mb: Some(512),
        };
        let json = serde_json::to_string(&usage).expect("serialize");
        let decoded: ResourceUsage = serde_json::from_str(&json).expect("deserialize");
        assert!((decoded.cpu_percent - 73.2).abs() < f64::EPSILON);
        assert_eq!(decoded.memory_mb, 256);
        assert_eq!(decoded.disk_mb, Some(512));
    }

    #[test]
    fn sandbox_metadata_clone() {
        let meta = SandboxMetadata {
            id: "clone-test".into(),
            image: "node:20".into(),
            cpu_limit: 1,
            memory_limit_mb: 256,
            disk_limit_mb: None,
        };
        let cloned = meta.clone();
        assert_eq!(cloned.id, meta.id);
        assert_eq!(cloned.image, meta.image);
        assert_eq!(cloned.cpu_limit, meta.cpu_limit);
        assert_eq!(cloned.memory_limit_mb, meta.memory_limit_mb);
        assert_eq!(cloned.disk_limit_mb, meta.disk_limit_mb);
    }
}
