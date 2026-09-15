//! Docker orchestration module (A+ T2 / KDesktopVirt Phase C).
//!
//! # Status
//!
//! - Always-on: [`DockerOrchestrator`] trait, [`ContainerConfig`], fail-loud
//!   [`StubDockerOrchestrator`], and [`probe`] helpers that detect a Docker
//!   Engine (CLI / socket) without requiring `bollard`.
//! - Feature `sandbox-docker`: [`BollardDockerOrchestrator`] +
//!   [`DockerSandboxClient`] — live start / stop / exec when a daemon is
//!   reachable. When the daemon is absent, constructors return
//!   [`PhenoError::UnsupportedPlatform`] with [`codes::SANDBOX_DOCKER_STUB`].
//!
//! Do **not** unarchive KDesktopVirt for routine work; copy patterns only in a
//! scoped extract PR.
//!
//! # Extraction targets (archived KDesktopVirt)
//!
//! | Source (archived) | Destination | Notes |
//! |---|---|---|
//! | `src/containerization.rs` | this module / [`BollardDockerOrchestrator`] | behind `sandbox-docker` |
//! | `src/virtualization.rs` | evaluate vs this module | prefer this as canonical |
//!
//! See `docs/EXTRACTION_PLAN.md` Phase C and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

mod stats;

#[cfg(feature = "sandbox-docker")]
mod bollard_backend;
#[cfg(feature = "sandbox-docker")]
pub use bollard_backend::{BollardDockerOrchestrator, DockerSandboxClient};

pub use stats::cpu_percent_from_deltas;

/// Docker container orchestrator trait.
///
/// Prefer [`BollardDockerOrchestrator`] when the `sandbox-docker` feature is
/// enabled and [`probe::docker_ready`] is true. Otherwise use
/// [`StubDockerOrchestrator`] (fail-loud).
#[async_trait::async_trait]
pub trait DockerOrchestrator: Send + Sync {
    /// Create and start a container; returns the container id.
    async fn start_container(&self, image: &str, config: ContainerConfig) -> Result<String>;

    /// Stop and remove a container.
    async fn stop_container(&self, container_id: &str) -> Result<()>;

    /// Run a command inside a running container (argv-style via whitespace split).
    async fn exec(&self, container_id: &str, cmd: &str) -> Result<String>;

    /// Get container resource usage.
    async fn get_resource_usage(&self, container_id: &str) -> Result<ResourceSnapshot>;

    /// Whether a live Docker Engine client is wired and reachable.
    fn docker_ready(&self) -> bool {
        false
    }
}

/// Container configuration for orchestration.
#[derive(Debug, Clone)]
pub struct ContainerConfig {
    pub cpu_limit: f64,
    pub memory_limit_mb: u64,
    pub disk_limit_mb: Option<u64>,
    pub ports: Vec<PortMapping>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            cpu_limit: 1.0,
            memory_limit_mb: 512,
            disk_limit_mb: None,
            ports: vec![],
        }
    }
}

/// Port mapping for container networking.
#[derive(Debug, Clone)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
}

/// Resource snapshot from container introspection.
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    pub cpu_percent: f64,
    pub memory_mb: u64,
    pub disk_mb: Option<u64>,
}

/// Probe helpers for a Docker Engine (no `bollard` required).
pub mod probe {
    use std::path::Path;
    use std::process::Command;

    /// Env override for the Docker CLI binary path (`EIDOLON_DOCKER`).
    pub const DOCKER_PATH_ENV: &str = "EIDOLON_DOCKER";

    /// `true` when a Docker Engine appears reachable via CLI or socket.
    ///
    /// This is a **host** probe (CI-friendly). Live API work still requires
    /// feature `sandbox-docker` and a successful bollard ping.
    pub fn docker_ready() -> bool {
        docker_cli_ready() || docker_socket_present()
    }

    /// Resolve `docker` via `EIDOLON_DOCKER` or `PATH`.
    pub fn resolve_docker_cli() -> Option<std::path::PathBuf> {
        if let Ok(override_path) = std::env::var(DOCKER_PATH_ENV) {
            let p = std::path::PathBuf::from(override_path);
            if p.is_file() {
                return Some(p);
            }
        }
        which_docker()
    }

    /// `docker info` succeeds (daemon reachable through the CLI).
    pub fn docker_cli_ready() -> bool {
        let bin = match resolve_docker_cli() {
            Some(b) => b,
            None => return false,
        };
        let output = Command::new(&bin)
            .args(["info", "--format", "{{.ServerVersion}}"])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                let version = String::from_utf8_lossy(&o.stdout);
                !version.trim().is_empty()
            }
            _ => false,
        }
    }

    /// Common Docker socket paths exist (daemon may or may not answer).
    pub fn docker_socket_present() -> bool {
        if let Ok(host) = std::env::var("DOCKER_HOST") {
            if let Some(path) = host.strip_prefix("unix://") {
                return Path::new(path).exists();
            }
        }
        candidate_unix_sockets().iter().any(|p| p.exists())
    }

    /// Candidate unix socket paths (Docker Desktop, Colima, OrbStack, system).
    pub fn candidate_unix_sockets() -> Vec<std::path::PathBuf> {
        let mut paths = vec![
            std::path::PathBuf::from("/var/run/docker.sock"),
            std::path::PathBuf::from("/run/docker.sock"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            let home = std::path::PathBuf::from(home);
            paths.push(home.join(".docker/run/docker.sock"));
            paths.push(home.join(".colima/default/docker.sock"));
            paths.push(home.join(".colima/docker.sock"));
            paths.push(home.join(".orbstack/run/docker.sock"));
        }
        paths
    }

    fn which_docker() -> Option<std::path::PathBuf> {
        let output = Command::new("which").arg("docker").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return None;
        }
        let p = std::path::PathBuf::from(path);
        p.is_file().then_some(p)
    }
}

/// Fail-loud Docker orchestrator stub — default when `sandbox-docker` is off
/// or no Docker daemon is available.
pub struct StubDockerOrchestrator;

impl StubDockerOrchestrator {
    pub fn new() -> Self {
        Self
    }

    fn unsupported(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::SANDBOX_DOCKER_STUB,
            format!(
                "DockerOrchestrator::{method} not implemented — enable feature \
                 `sandbox-docker` and ensure a Docker Engine is reachable, or \
                 extract remaining KDesktopVirt `src/containerization.rs` \
                 (docs/EXTRACTION_PLAN.md Phase C; do not unarchive routinely)"
            ),
        )
    }
}

impl Default for StubDockerOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl DockerOrchestrator for StubDockerOrchestrator {
    async fn start_container(&self, _image: &str, _config: ContainerConfig) -> Result<String> {
        Err(Self::unsupported("start_container"))
    }

    async fn stop_container(&self, _container_id: &str) -> Result<()> {
        Err(Self::unsupported("stop_container"))
    }

    async fn exec(&self, _container_id: &str, _cmd: &str) -> Result<String> {
        Err(Self::unsupported("exec"))
    }

    async fn get_resource_usage(&self, _container_id: &str) -> Result<ResourceSnapshot> {
        Err(Self::unsupported("get_resource_usage"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_docker_code(err: PhenoError) {
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_DOCKER_STUB));
        assert_eq!(err.status_code(), 501);
    }

    #[tokio::test]
    async fn stub_start_container_fail_loud() {
        let orch = StubDockerOrchestrator;
        let err = orch
            .start_container(
                "ubuntu:latest",
                ContainerConfig {
                    cpu_limit: 1.0,
                    memory_limit_mb: 512,
                    disk_limit_mb: None,
                    ports: vec![],
                },
            )
            .await
            .unwrap_err();
        assert_docker_code(err);
        assert!(!orch.docker_ready());
    }

    #[tokio::test]
    async fn stub_stop_container_fail_loud() {
        let orch = StubDockerOrchestrator;
        assert_docker_code(orch.stop_container("abc123").await.unwrap_err());
    }

    #[tokio::test]
    async fn stub_exec_fail_loud() {
        let orch = StubDockerOrchestrator;
        assert_docker_code(orch.exec("abc123", "echo hi").await.unwrap_err());
    }

    #[tokio::test]
    async fn stub_get_resource_usage_fail_loud() {
        let orch = StubDockerOrchestrator;
        assert_docker_code(orch.get_resource_usage("abc123").await.unwrap_err());
    }

    #[test]
    fn probe_docker_consistency() {
        let ready = probe::docker_ready();
        // ready ⇒ at least one of CLI or socket; not the reverse (socket can
        // exist while daemon is down).
        if probe::docker_cli_ready() {
            assert!(ready);
        }
    }
}
