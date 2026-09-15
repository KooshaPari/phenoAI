//! Live Docker Engine backend via `bollard` (`sandbox-docker` feature).
//!
//! Do not unarchive KDesktopVirt — this is a fresh trait-shaped wiring of the
//! Docker Engine API for start / stop / exec / stats.

use super::stats;
use super::{probe, ContainerConfig, DockerOrchestrator, PortMapping, ResourceSnapshot};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::security::{validate_exec_cmd, validate_sandbox_id, SandboxPolicy};
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};
use futures_util::stream::StreamExt;
use futures_util::TryStreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

use bollard::exec::StartExecResults;
use bollard::models::{
    ContainerCreateBody, ExecConfig, HostConfig, PortBinding, PortMap,
};
use bollard::query_parameters::{
    CreateImageOptionsBuilder, RemoveContainerOptionsBuilder, StatsOptionsBuilder,
    StopContainerOptionsBuilder,
};
use bollard::Docker;

fn docker_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_DOCKER_STUB,
        format!(
            "DockerOrchestrator::{method} unavailable — Docker Engine not \
             reachable ({detail}); enable feature `sandbox-docker` and start \
             the daemon, or use StubDockerOrchestrator (docs/EXTRACTION_PLAN.md \
             Phase C; do not unarchive KDesktopVirt routinely)"
        ),
    )
}

fn map_docker_err(method: &str, err: bollard::errors::Error) -> PhenoError {
    PhenoError::Platform(format!("DockerOrchestrator::{method}: {err}"))
}

fn split_cmd(cmd: &str) -> Result<Vec<String>> {
    validate_exec_cmd(cmd)?;
    let parts: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
    if parts.is_empty() {
        return Err(PhenoError::BadRequest(
            "exec command must contain at least one argv token".into(),
        ));
    }
    Ok(parts)
}

fn host_config_from(config: &ContainerConfig) -> HostConfig {
    let nano_cpus = if config.cpu_limit > 0.0 {
        Some((config.cpu_limit * 1_000_000_000.0) as i64)
    } else {
        None
    };
    let memory = if config.memory_limit_mb > 0 {
        Some((config.memory_limit_mb as i64).saturating_mul(1024 * 1024))
    } else {
        None
    };

    let port_bindings: Option<PortMap> = if config.ports.is_empty() {
        None
    } else {
        let mut map: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        for PortMapping {
            host_port,
            container_port,
        } in &config.ports
        {
            map.insert(
                format!("{container_port}/tcp"),
                Some(vec![PortBinding {
                    host_ip: Some("127.0.0.1".into()),
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }
        Some(map)
    };

    HostConfig {
        nano_cpus,
        memory,
        port_bindings,
        ..Default::default()
    }
}

/// Live bollard-backed Docker orchestrator.
pub struct BollardDockerOrchestrator {
    docker: Docker,
    ready: AtomicBool,
}

impl BollardDockerOrchestrator {
    /// Connect and ping the local Docker Engine.
    ///
    /// Tries `DOCKER_HOST` / defaults, then well-known unix sockets (system,
    /// Docker Desktop, Colima, OrbStack). Returns
    /// [`PhenoError::UnsupportedPlatform`] with [`codes::SANDBOX_DOCKER_STUB`]
    /// when no daemon answers (CI-friendly fail-loud path).
    pub async fn try_new() -> Result<Self> {
        let mut last_detail = String::from("no Docker client candidates");
        for docker in Self::candidate_clients() {
            match docker.ping().await {
                Ok(_) => {
                    return Ok(Self {
                        docker,
                        ready: AtomicBool::new(true),
                    });
                }
                Err(e) => {
                    last_detail = format!("ping failed: {e}");
                }
            }
        }
        Err(docker_unavailable("try_new", last_detail))
    }

    /// Host probe only — does not open a bollard client.
    pub fn host_docker_ready() -> bool {
        probe::docker_ready()
    }

    fn candidate_clients() -> Vec<Docker> {
        let mut clients = Vec::new();
        // Honour DOCKER_HOST when set; otherwise default host string.
        if let Ok(d) = Docker::connect_with_defaults() {
            clients.push(d);
        }
        if let Ok(d) = Docker::connect_with_local_defaults() {
            clients.push(d);
        }
        if let Ok(d) = Docker::connect_with_socket_defaults() {
            clients.push(d);
        }
        for path in probe::candidate_unix_sockets() {
            if !path.exists() {
                continue;
            }
            let path_str = path.to_string_lossy();
            if let Ok(d) = Docker::connect_with_unix(
                path_str.as_ref(),
                120,
                bollard::API_DEFAULT_VERSION,
            ) {
                clients.push(d);
            }
        }
        clients
    }

    async fn ensure_image(&self, image: &str) -> Result<()> {
        let options = CreateImageOptionsBuilder::default()
            .from_image(image)
            .build();
        self.docker
            .create_image(Some(options), None, None)
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| map_docker_err("ensure_image", e))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl DockerOrchestrator for BollardDockerOrchestrator {
    async fn start_container(&self, image: &str, config: ContainerConfig) -> Result<String> {
        if image.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "container image must be non-empty".into(),
            ));
        }
        self.ensure_image(image).await?;

        let body = ContainerCreateBody {
            image: Some(image.to_string()),
            // Keep the container alive so subsequent exec calls succeed.
            // BusyBox `sleep` may not accept `infinity`; `tail -f` is portable.
            cmd: Some(vec![
                "tail".into(),
                "-f".into(),
                "/dev/null".into(),
            ]),
            host_config: Some(host_config_from(&config)),
            tty: Some(false),
            ..Default::default()
        };

        let created = self
            .docker
            .create_container(
                None::<bollard::query_parameters::CreateContainerOptions>,
                body,
            )
            .await
            .map_err(|e| map_docker_err("start_container", e))?;

        self.docker
            .start_container(
                &created.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await
            .map_err(|e| map_docker_err("start_container", e))?;

        Ok(created.id)
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        if container_id.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "container_id must be non-empty".into(),
            ));
        }
        let _ = self
            .docker
            .stop_container(
                container_id,
                Some(StopContainerOptionsBuilder::default().t(5).build()),
            )
            .await;
        self.docker
            .remove_container(
                container_id,
                Some(
                    RemoveContainerOptionsBuilder::default()
                        .force(true)
                        .build(),
                ),
            )
            .await
            .map_err(|e| map_docker_err("stop_container", e))?;
        Ok(())
    }

    async fn exec(&self, container_id: &str, cmd: &str) -> Result<String> {
        if container_id.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "container_id must be non-empty".into(),
            ));
        }
        let argv = split_cmd(cmd)?;
        let exec = self
            .docker
            .create_exec(
                container_id,
                ExecConfig {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(argv),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| map_docker_err("exec", e))?;

        match self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| map_docker_err("exec", e))?
        {
            StartExecResults::Attached { mut output, .. } => {
                let mut buf = String::new();
                while let Some(item) = output.next().await {
                    match item {
                        Ok(msg) => buf.push_str(&msg.to_string()),
                        Err(e) => return Err(map_docker_err("exec", e)),
                    }
                }
                Ok(buf)
            }
            StartExecResults::Detached => Ok(String::new()),
        }
    }

    async fn get_resource_usage(&self, container_id: &str) -> Result<ResourceSnapshot> {
        if container_id.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "container_id must be non-empty".into(),
            ));
        }
        let mut stream = self.docker.stats(
            container_id,
            Some(
                StatsOptionsBuilder::default()
                    .stream(false)
                    .one_shot(true)
                    .build(),
            ),
        );
        let stats = stream
            .next()
            .await
            .ok_or_else(|| {
                PhenoError::Platform("DockerOrchestrator::get_resource_usage: empty stats".into())
            })?
            .map_err(|e| map_docker_err("get_resource_usage", e))?;

        let memory_mb = stats
            .memory_stats
            .as_ref()
            .and_then(|m| m.usage)
            .unwrap_or(0)
            .saturating_div(1024 * 1024);
        let cpu_percent = stats::cpu_percent_from_container_stats(&stats);

        Ok(ResourceSnapshot {
            cpu_percent,
            memory_mb,
            disk_mb: None,
        })
    }

    fn docker_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }
}

/// `SandboxAutomator` backed by a live [`BollardDockerOrchestrator`].
///
/// Replaces the fail-loud [`crate::SandboxClient`] lifecycle path when Docker
/// is available and feature `sandbox-docker` is enabled.
pub struct DockerSandboxClient {
    sandbox_id: String,
    image: String,
    policy: SandboxPolicy,
    orch: BollardDockerOrchestrator,
    container_id: Mutex<Option<String>>,
}

impl DockerSandboxClient {
    /// Default image used when callers do not override (`alpine:3`).
    pub const DEFAULT_IMAGE: &'static str = "alpine:3";

    /// Connect to Docker and construct a sandbox client for `sandbox_id`.
    pub async fn try_new(sandbox_id: &str) -> Result<Self> {
        Self::try_with_image(sandbox_id, Self::DEFAULT_IMAGE, SandboxPolicy::default()).await
    }

    /// Connect with an explicit image and isolation policy.
    pub async fn try_with_image(
        sandbox_id: &str,
        image: &str,
        policy: SandboxPolicy,
    ) -> Result<Self> {
        validate_sandbox_id(sandbox_id)?;
        if image.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "container image must be non-empty".into(),
            ));
        }
        let orch = BollardDockerOrchestrator::try_new().await?;
        Ok(Self {
            sandbox_id: sandbox_id.to_string(),
            image: image.to_string(),
            policy,
            orch,
            container_id: Mutex::new(None),
        })
    }

    /// Declared sandbox id.
    pub fn sandbox_id(&self) -> &str {
        &self.sandbox_id
    }

    /// Container image label.
    pub fn image(&self) -> &str {
        &self.image
    }

    /// Isolation policy carried into container host config.
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// Whether the underlying bollard client is ready.
    pub fn docker_ready(&self) -> bool {
        self.orch.docker_ready()
    }

    fn container_config(&self) -> ContainerConfig {
        ContainerConfig {
            cpu_limit: f64::from(self.policy.cpu_cores),
            memory_limit_mb: u64::from(self.policy.memory_mib),
            disk_limit_mb: self.policy.disk_mib.map(u64::from),
            ports: vec![],
        }
    }
}

#[async_trait::async_trait]
impl SandboxAutomator for DockerSandboxClient {
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        Ok(SandboxMetadata {
            id: self.sandbox_id.clone(),
            image: self.image.clone(),
            cpu_limit: self.policy.cpu_cores,
            memory_limit_mb: self.policy.memory_mib,
            disk_limit_mb: self.policy.disk_mib,
        })
    }

    async fn start(&self) -> Result<()> {
        let mut guard = self.container_id.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let id = self
            .orch
            .start_container(&self.image, self.container_config())
            .await?;
        *guard = Some(id);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut guard = self.container_id.lock().await;
        if let Some(id) = guard.take() {
            self.orch.stop_container(&id).await?;
        }
        Ok(())
    }

    async fn exec(&self, cmd: &str) -> Result<String> {
        validate_exec_cmd(cmd)?;
        let guard = self.container_id.lock().await;
        let id = guard.as_ref().ok_or_else(|| {
            PhenoError::BadRequest(
                "DockerSandboxClient::exec requires start() first (no container id)"
                    .into(),
            )
        })?;
        self.orch.exec(id, cmd).await
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        let guard = self.container_id.lock().await;
        let id = guard.as_ref().ok_or_else(|| {
            PhenoError::BadRequest(
                "DockerSandboxClient::resource_usage requires start() first".into(),
            )
        })?;
        let snap = self.orch.get_resource_usage(id).await?;
        Ok(ResourceUsage {
            cpu_percent: snap.cpu_percent,
            memory_mb: snap.memory_mb.min(u64::from(u32::MAX)) as u32,
            disk_mb: snap
                .disk_mb
                .map(|d| d.min(u64::from(u32::MAX)) as u32),
        })
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded sandbox docker event: {:?}", event);
        Ok(())
    }
}
