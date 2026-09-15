#![forbid(unsafe_code)]

pub mod model;
pub mod nvms;
pub mod service_graph;

pub use service_graph::{
    ensure_service_lifecycle_capability, execute_lifecycle_plan, plan_lifecycle, IntentPhase, LifecyclePlan,
    LIFECYCLE_SCHEMA_VERSION, RollbackContract, SERVICE_LIFECYCLE_CAPABILITY,
};

use model::{
    canonical_gpu_uuid, CompositionManifest, EffectiveEngine, ExternalEngineToken, GpuVendor, Plan, ProviderStatus,
    RuntimeProvider, Service,
};
use nvms::{
    ArtifactRequirements, EvaluationClient, EvaluationRequest, EvaluationResult, GpuBinding, LifecycleClient,
    ProcessEvaluationClient, ProcessLifecycleClient, ResourceGpu, ResourceManifest, ServiceDefinition,
    ServiceLifecycleIntent, ServiceLifecycleRequest, ServiceLifecycleResult, ACTION_VERSION, DEFAULT_PODMAN_PIPE,
    RESOURCE_VERSION, SERVICE_LIFECYCLE_VERSION,
};
use phenocompose_port_composer::{ComposeError, Composer};
use phenocompose_port_publisher::{PublishError, Publisher};
use phenocompose_port_runtime::{Runtime, RuntimeError};
use phenocompose_port_types::{
    ComposedArtifact, ContainerId, ContainerStatus, ImageRef, Manifest, PublishReceipt, PublishTarget,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, CliError>;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CliError {
    pub kind: ErrorKind,
    pub code: String,
    pub message: String,
    pub capability: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    Unsupported,
    NotFound,
    Conflict,
    Backend,
    Io,
}

#[derive(Serialize)]
pub struct ErrorEnvelope<'a> {
    pub error: ErrorBody<'a>,
}

#[derive(Serialize)]
pub struct ErrorBody<'a> {
    pub kind: ErrorKind,
    pub code: &'a str,
    pub message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<&'a str>,
}

impl CliError {
    pub fn validation(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, code, message)
    }

    pub fn unsupported(
        code: impl Into<String>,
        message: impl Into<String>,
        capability: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            kind: ErrorKind::Unsupported,
            code: code.into(),
            message: message.into(),
            capability: Some(capability.into()),
            provider: Some(provider.into()),
        }
    }

    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, code, message)
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Conflict, code, message)
    }

    pub fn backend(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Backend, code, message)
    }

    pub fn io(code: impl Into<String>, error: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, code, error.to_string())
    }

    pub fn json(error: serde_json::Error) -> Self {
        Self::validation("json", error.to_string())
    }

    fn new(kind: ErrorKind, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: code.into(),
            message: message.into(),
            capability: None,
            provider: None,
        }
    }

    pub fn envelope(&self) -> ErrorEnvelope<'_> {
        ErrorEnvelope {
            error: ErrorBody {
                kind: self.kind,
                code: &self.code,
                message: &self.message,
                capability: self.capability.as_deref(),
                provider: self.provider.as_deref(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunState {
    pub state_version: String,
    pub run_id: String,
    pub manifest_sha256: String,
    pub provider: String,
    pub created_unix_seconds: u64,
    pub lifecycle: RunLifecycle,
    pub containers: BTreeMap<String, String>,
    pub manifest: CompositionManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunLifecycle {
    Running,
    Down,
}

#[derive(Debug, Serialize)]
pub struct ApplyOutput {
    pub run_id: String,
    pub manifest_sha256: String,
    pub provider: String,
    pub dry_run: bool,
    pub mutation: bool,
    pub lifecycle: LifecyclePlan,
    pub containers: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct StatusOutput {
    pub run_id: String,
    pub lifecycle: RunLifecycle,
    pub services: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct Provenance {
    pub provenance_version: String,
    pub run_id: String,
    pub manifest_sha256: String,
    pub effective_engine: String,
    pub created_unix_seconds: u64,
    pub containers: BTreeMap<String, String>,
    pub jobs: BTreeMap<String, JobProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JobProvenance {
    pub provenance_version: String,
    pub run_id: String,
    pub job_id: String,
    pub manifest_sha256: String,
    pub effective_engine: String,
    pub resolved_provider: String,
    pub execution_plane: String,
    pub action: String,
    pub service: String,
    pub command: Vec<String>,
    pub image: String,
    pub dependency_services: Vec<String>,
    pub gpu_bindings: Vec<GpuBinding>,
    pub timeout_millis: i64,
    pub max_output_bytes: usize,
    #[serde(default)]
    pub output_root: String,
    #[serde(default)]
    pub output_root_created: bool,
    #[serde(default)]
    pub output_root_available_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolkit_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolkit_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolkit_executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<nvms::Lifecycle>,
    pub success: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error_message: String,
}

pub fn load_manifest(path: &Path) -> Result<CompositionManifest> {
    let input = fs::read_to_string(path).map_err(|error| CliError::io("manifest_read", error))?;
    CompositionManifest::parse(&input)
}

pub fn render_plan(manifest: &CompositionManifest) -> Result<Plan> {
    manifest.plan()
}

pub fn apply(manifest: CompositionManifest, state_dir: &Path, dry_run: bool) -> Result<ApplyOutput> {
    apply_with_lifecycle_client(manifest, state_dir, dry_run, &ProcessLifecycleClient)
}

pub fn apply_with_lifecycle_client(
    manifest: CompositionManifest,
    state_dir: &Path,
    dry_run: bool,
    lifecycle_client: &dyn LifecycleClient,
) -> Result<ApplyOutput> {
    let plan = manifest.plan()?;
    let lifecycle = plan_lifecycle(&manifest, &plan.manifest_sha256)?;
    let run_id = format!(
        "{}-{}",
        sanitize_name(&manifest.metadata.name),
        &plan.manifest_sha256[..12]
    );
    let provider = runtime_provider_name(&manifest.runtime.provider).to_owned();
    if dry_run {
        return Ok(ApplyOutput {
            run_id,
            manifest_sha256: plan.manifest_sha256,
            provider,
            dry_run: true,
            mutation: false,
            lifecycle,
            containers: BTreeMap::new(),
        });
    }
    ensure_apply_capabilities(&manifest)?;
    ensure_service_lifecycle_capability(&manifest)?;
    let state_path = state_path(state_dir, &run_id);
    if state_path.exists() {
        return Err(CliError::conflict(
            "run_exists",
            format!("run {run_id} already has persisted state"),
        ));
    }

    let composer = PrebuiltImageComposer;
    let publisher = PodmanLocalPublisher::new(&manifest.runtime.wsl_distribution);
    publisher.probe()?;
    for intent in &lifecycle.intents {
        if intent.phase != IntentPhase::Create {
            continue;
        }
        let name = &intent.service;
        let service = &manifest.services[name];
        let port_manifest = Manifest::new(name).with_tag("image", &service.image);
        let artifact = composer.compose(&port_manifest).map_err(map_compose_error)?;
        publisher
            .publish(&artifact, &PublishTarget::new("podman-local", &service.image))
            .map_err(map_publish_error)?;
    }

    let request = build_lifecycle_request(&manifest, &lifecycle, &run_id, &plan.manifest_sha256)?;
    let result = match lifecycle_client.execute(&request) {
        Ok(result) => result,
        Err(failure) => {
            rollback_containers_from_map(&manifest.runtime.wsl_distribution, &failure.result.containers);
            return Err(failure.error);
        }
    };
    if let Err(error) = validate_lifecycle_result(&request, &result) {
        rollback_containers_from_map(&manifest.runtime.wsl_distribution, &result.containers);
        return Err(error);
    }
    let containers = result.containers;

    let state = RunState {
        state_version: "phenocompose.run/v0".to_owned(),
        run_id: run_id.clone(),
        manifest_sha256: plan.manifest_sha256.clone(),
        provider: "podman".to_owned(),
        created_unix_seconds: unix_seconds()?,
        lifecycle: RunLifecycle::Running,
        containers: containers.clone(),
        manifest,
    };
    if let Err(error) = save_state(state_dir, &state) {
        rollback_containers_from_map(&state.manifest.runtime.wsl_distribution, &containers);
        return Err(error);
    }
    Ok(ApplyOutput {
        run_id,
        manifest_sha256: plan.manifest_sha256,
        provider: "podman".to_owned(),
        dry_run: false,
        mutation: true,
        lifecycle,
        containers,
    })
}

pub fn status(state_dir: &Path, run_id: &str) -> Result<StatusOutput> {
    let state = load_state(state_dir, run_id)?;
    ensure_persisted_provider(&state)?;
    let runtime = PodmanRuntime::bare(&state.manifest.runtime.wsl_distribution);
    let mut services = BTreeMap::new();
    for (service, id) in &state.containers {
        let value = runtime
            .status(&ContainerId::new(id))
            .map_err(map_runtime_error)?
            .to_string();
        services.insert(service.clone(), value);
    }
    Ok(StatusOutput {
        run_id: state.run_id,
        lifecycle: state.lifecycle,
        services,
    })
}

pub fn down(state_dir: &Path, run_id: &str) -> Result<StatusOutput> {
    let mut state = load_state(state_dir, run_id)?;
    ensure_persisted_provider(&state)?;
    if state.lifecycle == RunLifecycle::Down {
        return Err(CliError::conflict(
            "run_already_down",
            format!("run {run_id} is already down"),
        ));
    }
    let runtime = PodmanRuntime::bare(&state.manifest.runtime.wsl_distribution);
    let mut order = if state.manifest.teardown.order.is_empty() {
        let mut order = state.manifest.service_order()?;
        order.reverse();
        order
    } else {
        state.manifest.teardown.order.clone()
    };
    for service in order.drain(..) {
        if let Some(id) = state.containers.get(&service) {
            match runtime.status(&ContainerId::new(id)).map_err(map_runtime_error)? {
                ContainerStatus::NotFound | ContainerStatus::Exited => {}
                _ => runtime.stop(&ContainerId::new(id)).map_err(map_runtime_error)?,
            }
        }
    }
    state.lifecycle = RunLifecycle::Down;
    save_state(state_dir, &state)?;
    let services = state
        .containers
        .keys()
        .map(|name| (name.clone(), "down".to_owned()))
        .collect();
    Ok(StatusOutput {
        run_id: state.run_id,
        lifecycle: state.lifecycle,
        services,
    })
}

pub fn run_action(state_dir: &Path, run_id: &str, action: &str, job_id: &str) -> Result<JobProvenance> {
    run_action_with_client(state_dir, run_id, action, job_id, &ProcessEvaluationClient)
}

pub fn run_action_with_client(
    state_dir: &Path,
    run_id: &str,
    action_name: &str,
    job_id: &str,
    client: &dyn EvaluationClient,
) -> Result<JobProvenance> {
    let workspace_base = env::current_dir().map_err(|error| CliError::io("current_dir", error))?;
    run_action_with_client_at(&workspace_base, state_dir, run_id, action_name, job_id, client)
}

pub fn run_action_with_client_at(
    workspace_base: &Path,
    state_dir: &Path,
    run_id: &str,
    action_name: &str,
    job_id: &str,
    client: &dyn EvaluationClient,
) -> Result<JobProvenance> {
    validate_slug("run_id", run_id)?;
    validate_slug("job_id", job_id)?;
    let state = load_state(state_dir, run_id)?;
    if state.lifecycle != RunLifecycle::Running {
        return Err(CliError::conflict(
            "run_not_running",
            format!("run {run_id} is not running"),
        ));
    }
    ensure_action_route(&state)?;
    let action = state.manifest.actions.get(action_name).ok_or_else(|| {
        CliError::not_found(
            "action_not_found",
            format!("run {run_id} has no action named {action_name}"),
        )
    })?;
    let service = &state.manifest.services[&action.service];
    let (dependency_services, gpu_bindings) = action_gpu_bindings(&state.manifest, &action.service)?;
    let command = action.command.clone();
    let mut job = JobProvenance {
        provenance_version: "phenocompose.job-provenance/v1".to_owned(),
        run_id: run_id.to_owned(),
        job_id: job_id.to_owned(),
        manifest_sha256: state.manifest_sha256.clone(),
        effective_engine: "podman".to_owned(),
        resolved_provider: "podman".to_owned(),
        execution_plane: "nanovms".to_owned(),
        action: action_name.to_owned(),
        service: action.service.clone(),
        command: command.clone(),
        image: service.image.clone(),
        dependency_services,
        gpu_bindings: gpu_bindings.clone(),
        timeout_millis: 300_000,
        max_output_bytes: 1_048_576,
        output_root: String::new(),
        output_root_created: false,
        output_root_available_bytes: None,
        toolkit_version: None,
        toolkit_root: None,
        toolkit_executable: None,
        lifecycle: Some(nvms::Lifecycle::local_failure(b"", b"")),
        success: false,
        error_code: String::new(),
        error_message: String::new(),
    };
    if job_path(state_dir, run_id, job_id).exists() {
        return Err(CliError::conflict(
            "job_exists",
            format!("job {job_id} already has persisted provenance"),
        ));
    }

    let request = match build_evaluation_request(workspace_base, state_dir, &state, &job, service) {
        Ok(request) => request,
        Err(error) => return persist_job_failure(state_dir, job, error),
    };
    job.output_root.clone_from(&request.output_root);
    let result = match client.execute(&request) {
        Ok(result) => result,
        Err(failure) => {
            let nvms::EvaluationFailure { error, lifecycle } = *failure;
            job.lifecycle = Some(trustworthy_failure_lifecycle(lifecycle, request.max_output_bytes));
            return persist_job_failure(state_dir, job, error);
        }
    };
    job.output_root_created = result.provenance.output_root_created;
    job.output_root_available_bytes = result.provenance.output_root_available_bytes;
    job.toolkit_version = result.provenance.toolkit_version.clone();
    job.toolkit_root = result.provenance.toolkit_root.clone();
    job.toolkit_executable = result.provenance.toolkit_executable.clone();
    job.lifecycle = Some(trustworthy_failure_lifecycle(
        result.lifecycle.clone(),
        request.max_output_bytes,
    ));
    job.error_code = result.error_code.clone();
    job.error_message = result.error_message.clone();
    if let Err(error) = validate_evaluation_result(&request, &result) {
        return persist_job_failure(state_dir, job, error);
    }
    if !result.success {
        let code = if result.error_code.is_empty() {
            "nvms_action_failed"
        } else {
            &result.error_code
        };
        let message = if result.error_message.is_empty() {
            "NanoVMS rejected the evaluation action".to_owned()
        } else {
            result.error_message
        };
        return persist_job_failure(state_dir, job, CliError::backend(code, message));
    }
    job.success = true;
    save_job_provenance(state_dir, &job)?;
    Ok(job)
}

pub fn export_provenance(state_dir: &Path, run_id: &str) -> Result<Provenance> {
    let state = load_state(state_dir, run_id)?;
    if !state.manifest.provenance.required {
        return Err(CliError::unsupported(
            "provenance_not_requested",
            "the persisted manifest did not require provenance",
            "provenance.export",
            &state.provider,
        ));
    }
    Ok(Provenance {
        provenance_version: "phenocompose.provenance/v0".to_owned(),
        run_id: state.run_id,
        manifest_sha256: state.manifest_sha256,
        effective_engine: state.provider,
        created_unix_seconds: state.created_unix_seconds,
        containers: state.containers,
        jobs: load_jobs(state_dir, run_id)?,
    })
}

fn action_gpu_bindings(manifest: &CompositionManifest, root_service: &str) -> Result<(Vec<String>, Vec<GpuBinding>)> {
    fn visit(manifest: &CompositionManifest, name: &str, visited: &mut BTreeSet<String>, output: &mut Vec<String>) {
        if !visited.insert(name.to_owned()) {
            return;
        }
        for dependency in &manifest.services[name].depends_on {
            visit(manifest, dependency, visited, output);
        }
        output.push(name.to_owned());
    }

    let mut closure = Vec::new();
    visit(manifest, root_service, &mut BTreeSet::new(), &mut closure);
    let toolkit = &manifest.environment.toolkit;
    let mut uuids = BTreeSet::new();
    for name in &closure {
        let Some(gpu) = manifest.services[name]
            .resources
            .as_ref()
            .and_then(|resources| resources.gpu.as_ref())
        else {
            continue;
        };
        if gpu.vendor != GpuVendor::Nvidia {
            return Err(CliError::unsupported(
                "gpu_vendor_unsupported",
                format!("action dependency {name} requires a non-NVIDIA GPU"),
                "evaluation.gpu_uuid",
                "nanovms",
            ));
        }
        if !toolkit.name.eq_ignore_ascii_case("cuda") || toolkit.version.trim().is_empty() {
            return Err(CliError::validation(
                "toolkit_missing",
                format!("GPU-bearing action dependency {name} requires an explicit CUDA toolkit"),
            ));
        }
        for uuid in &gpu.uuids {
            let canonical = canonical_gpu_uuid(uuid).ok_or_else(|| {
                CliError::validation(
                    "gpu_selector_invalid",
                    format!("action dependency {name} has invalid GPU UUID {uuid:?}"),
                )
            })?;
            uuids.insert(canonical);
        }
    }
    if uuids.is_empty() {
        return Err(CliError::validation(
            "gpu_binding_missing",
            "NanoVMS evaluation actions require at least one GPU in the service dependency closure",
        ));
    }
    let bindings = uuids
        .into_iter()
        .map(|uuid| GpuBinding {
            cdi_device: format!("nvidia.com/gpu={uuid}"),
            uuid,
            cuda_toolkit: toolkit.version.clone(),
        })
        .collect();
    Ok((closure, bindings))
}

fn ensure_action_route(state: &RunState) -> Result<()> {
    let compatibility = state.manifest.runtime.portage_compatibility.as_ref();
    let exact_route = state.provider == "podman"
        && state.manifest.runtime.provider == RuntimeProvider::Podman
        && compatibility.is_some_and(|value| {
            value.external_engine_token == ExternalEngineToken::Docker
                && value.effective_engine == EffectiveEngine::Podman
        });
    if exact_route {
        Ok(())
    } else {
        Err(CliError::unsupported(
            "action_route_rejected",
            "evaluation requires the exact Podman route and historical docker schema token without fallback",
            "evaluation.action",
            &state.provider,
        ))
    }
}

fn build_evaluation_request(
    workspace_base: &Path,
    state_dir: &Path,
    state: &RunState,
    job: &JobProvenance,
    service: &Service,
) -> Result<EvaluationRequest> {
    let (executable, argv) = job
        .command
        .split_first()
        .ok_or_else(|| CliError::validation("action_command_empty", "action command must not be empty"))?;
    let output_root = state
        .manifest
        .actions
        .get(&job.action)
        .and_then(|action| action.output_root.as_deref())
        .ok_or_else(|| {
            CliError::validation(
                "action_output_root_missing",
                format!("action {} must declare output_root", job.action),
            )
        })
        .and_then(|root| resolve_output_root(workspace_base, root))?;
    let state_dir = absolute_path(state_dir)?;
    let reservation_path = state_dir.join("nanovms-gpu-reservations.json");
    let mut environment = state.manifest.environment.variables.clone();
    environment.extend(service.environment.clone());
    let gpus = job
        .gpu_bindings
        .iter()
        .map(|binding| ResourceGpu {
            uuid: binding.uuid.clone(),
            name: None,
            architecture: None,
            compute_capability: None,
            driver_version: String::new(),
            driver_cuda_ceiling: String::new(),
            observations: Vec::new(),
        })
        .collect();
    Ok(EvaluationRequest {
        version: ACTION_VERSION.to_owned(),
        backend: "podman".to_owned(),
        fallback_backends: Vec::new(),
        manifest_sha256: state.manifest_sha256.clone(),
        executable: executable.clone(),
        argv: argv.to_vec(),
        environment,
        external_engine_token: "docker".to_owned(),
        podman_pipe: DEFAULT_PODMAN_PIPE.to_owned(),
        wsl_distribution: state.manifest.runtime.wsl_distribution.clone().unwrap_or_default(),
        output_root: path_string(&output_root)?,
        reservation_path: path_string(&reservation_path)?,
        lock_invocation: job.command.clone(),
        resource_manifest: ResourceManifest {
            version: RESOURCE_VERSION.to_owned(),
            gpus,
            artifact: ArtifactRequirements {
                cuda_toolkit: job.gpu_bindings[0].cuda_toolkit.clone(),
                compiled_kernels: true,
            },
        },
        gpu_bindings: job.gpu_bindings.clone(),
        timeout_millis: job.timeout_millis,
        max_output_bytes: job.max_output_bytes,
    })
}

fn validate_evaluation_result(request: &EvaluationRequest, result: &EvaluationResult) -> Result<()> {
    if result.version != ACTION_VERSION {
        return Err(CliError::backend(
            "nvms_version_mismatch",
            format!("unexpected NanoVMS action version {:?}", result.version),
        ));
    }
    let provenance = &result.provenance;
    if !provenance
        .manifest_sha256
        .eq_ignore_ascii_case(&request.manifest_sha256)
    {
        return Err(CliError::backend(
            "nvms_manifest_mismatch",
            "NanoVMS returned a different manifest digest",
        ));
    }
    if provenance.effective_engine != "podman"
        || provenance.resolved_provider != "podman"
        || provenance.execution_plane != "nanovms"
    {
        return Err(CliError::backend(
            "nvms_route_mismatch",
            "NanoVMS did not attest the exact podman provider through the nanovms execution plane",
        ));
    }
    if provenance.podman_pipe != request.podman_pipe {
        return Err(CliError::backend(
            "nvms_pipe_mismatch",
            "NanoVMS returned a different Podman pipe",
        ));
    }
    if !provenance.job_directory.is_empty() {
        let output_root = Path::new(&request.output_root);
        let job_directory = Path::new(&provenance.job_directory);
        if !job_directory.is_absolute() || job_directory.parent() != Some(output_root) {
            return Err(CliError::backend(
                "nvms_output_root_mismatch",
                "NanoVMS returned a job directory outside the requested output root",
            ));
        }
    }
    let expected: BTreeSet<_> = request.gpu_bindings.iter().map(|binding| &binding.uuid).collect();
    let actual: BTreeSet<_> = provenance.gpu_uuids.iter().collect();
    if expected != actual || actual.len() != provenance.gpu_uuids.len() {
        return Err(CliError::backend(
            "nvms_gpu_binding_mismatch",
            "NanoVMS returned different or duplicate GPU UUID bindings",
        ));
    }
    let lifecycle = &result.lifecycle;
    if lifecycle.duration_ms < 0
        || lifecycle.duration_ms > request.timeout_millis
        || lifecycle.stdout.len() > request.max_output_bytes
        || lifecycle.stderr.len() > request.max_output_bytes
    {
        return Err(CliError::backend(
            "nvms_lifecycle_limits",
            "NanoVMS lifecycle evidence exceeded the requested bounds",
        ));
    }
    validate_evidence_hash("stdout", lifecycle.stdout.as_bytes(), &lifecycle.stdout_sha256)?;
    validate_evidence_hash("stderr", lifecycle.stderr.as_bytes(), &lifecycle.stderr_sha256)?;
    if result.success
        && (!result.released
            || lifecycle.exit_code != 0
            || lifecycle.timed_out
            || lifecycle.truncated
            || !result.error_code.is_empty()
            || !result.error_message.is_empty())
    {
        return Err(CliError::backend(
            "nvms_success_inconsistent",
            "NanoVMS success result contains failure lifecycle evidence",
        ));
    }
    Ok(())
}

fn validate_evidence_hash(label: &str, bytes: &[u8], claimed: &str) -> Result<()> {
    if claimed.len() != 64 || !claimed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CliError::backend(
            "nvms_evidence_hash_invalid",
            format!("NanoVMS {label} hash is not a SHA-256 hex digest"),
        ));
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(claimed) {
        return Err(CliError::backend(
            "nvms_evidence_hash_mismatch",
            format!("NanoVMS {label} hash does not match bounded evidence"),
        ));
    }
    Ok(())
}

fn trustworthy_failure_lifecycle(mut lifecycle: nvms::Lifecycle, max_output_bytes: usize) -> nvms::Lifecycle {
    lifecycle.stdout = bounded_string(lifecycle.stdout, max_output_bytes, &mut lifecycle.truncated);
    lifecycle.stderr = bounded_string(lifecycle.stderr, max_output_bytes, &mut lifecycle.truncated);
    if validate_evidence_hash("stdout", lifecycle.stdout.as_bytes(), &lifecycle.stdout_sha256).is_err() {
        lifecycle.stdout_sha256 = format!("{:x}", Sha256::digest(lifecycle.stdout.as_bytes()));
    }
    if validate_evidence_hash("stderr", lifecycle.stderr.as_bytes(), &lifecycle.stderr_sha256).is_err() {
        lifecycle.stderr_sha256 = format!("{:x}", Sha256::digest(lifecycle.stderr.as_bytes()));
    }
    lifecycle
}

fn bounded_string(mut value: String, limit: usize, truncated: &mut bool) -> String {
    if value.len() <= limit {
        return value;
    }
    let mut boundary = limit;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value.truncate(boundary);
    *truncated = true;
    value
}

fn persist_job_failure<T>(state_dir: &Path, mut job: JobProvenance, error: CliError) -> Result<T> {
    job.success = false;
    if job.error_code.is_empty() {
        job.error_code = error.code.clone();
    }
    if job.error_message.is_empty() {
        job.error_message = error.message.clone();
    }
    save_job_provenance(state_dir, &job)?;
    Err(error)
}

fn save_job_provenance(state_dir: &Path, job: &JobProvenance) -> Result<()> {
    let path = job_path(state_dir, &job.run_id, &job.job_id);
    let parent = path
        .parent()
        .ok_or_else(|| CliError::validation("job_path", "job provenance path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| CliError::io("job_dir_create", error))?;
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(job).map_err(CliError::json)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| CliError::io("job_write", error))?;
    let write_result = file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| CliError::io("job_write", error));
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        CliError::io("job_commit", error)
    })
}

pub fn load_job_provenance(state_dir: &Path, run_id: &str, job_id: &str) -> Result<JobProvenance> {
    validate_slug("run_id", run_id)?;
    validate_slug("job_id", job_id)?;
    let input =
        fs::read_to_string(job_path(state_dir, run_id, job_id)).map_err(|error| CliError::io("job_read", error))?;
    serde_json::from_str(&input).map_err(CliError::json)
}

fn load_jobs(state_dir: &Path, run_id: &str) -> Result<BTreeMap<String, JobProvenance>> {
    let directory = jobs_path(state_dir, run_id);
    if !directory.exists() {
        return Ok(BTreeMap::new());
    }
    let mut jobs = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(|error| CliError::io("jobs_read", error))? {
        let entry = entry.map_err(|error| CliError::io("jobs_read", error))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let input = fs::read_to_string(&path).map_err(|error| CliError::io("job_read", error))?;
        let job: JobProvenance = serde_json::from_str(&input).map_err(CliError::json)?;
        if job.run_id != run_id || path.file_stem().and_then(|value| value.to_str()) != Some(&job.job_id) {
            return Err(CliError::validation(
                "job_identity_mismatch",
                format!("job provenance identity does not match {}", path.display()),
            ));
        }
        jobs.insert(job.job_id.clone(), job);
    }
    Ok(jobs)
}

fn validate_slug(field: &str, value: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value.starts_with(|character: char| character.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(CliError::validation(
            format!("{field}_invalid"),
            format!("{field} must be a path-safe ASCII slug"),
        ))
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        env::current_dir()
            .map(|directory| directory.join(path))
            .map_err(|error| CliError::io("current_dir", error))
    }
}

fn resolve_output_root(workspace_base: &Path, declared: &str) -> Result<PathBuf> {
    let root = Path::new(declared);
    if declared.trim().is_empty() {
        return Err(CliError::validation(
            "action_output_root_empty",
            "action output_root must not be empty",
        ));
    }
    if root.has_root() && !root.is_absolute() {
        return Err(CliError::validation(
            "action_output_root_ambiguous",
            "action output_root must be fully absolute or relative to the workspace",
        ));
    }
    if root
        .components()
        .any(|component| matches!(component, Component::Prefix(_)))
        && !root.is_absolute()
    {
        return Err(CliError::validation(
            "action_output_root_ambiguous",
            "action output_root must not use a drive-relative path",
        ));
    }
    let base = absolute_path(workspace_base)?;
    let resolved = if root.is_absolute() {
        root.to_owned()
    } else {
        base.join(root)
    };
    normalize_output_root(&resolved)
}

fn normalize_output_root(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(CliError::validation(
                    "action_output_root_traversal",
                    "action output_root must not contain parent traversal",
                ));
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    if !normalized.is_absolute() {
        return Err(CliError::validation(
            "action_output_root_ambiguous",
            "resolved action output_root must be absolute",
        ));
    }
    Ok(normalized)
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CliError::validation("path_encoding", "NanoVMS paths must be valid UTF-8"))
}

fn ensure_apply_capabilities(manifest: &CompositionManifest) -> Result<()> {
    match manifest.runtime.provider {
        RuntimeProvider::Podman => {}
        RuntimeProvider::Nvms => {
            return Err(CliError::unsupported(
                "nvms_persistence_unsupported",
                "the NVMS driver cannot reattach to persisted instance IDs; apply was not attempted",
                "runtime.persistence",
                "nvms",
            ));
        }
        RuntimeProvider::Placeholder => {
            return Err(CliError::unsupported(
                "runtime_provider_placeholder",
                "the selected runtime is a capability placeholder",
                &manifest.runtime.capability,
                "placeholder",
            ));
        }
    }
    for (name, provider) in &manifest.providers {
        if provider.capability == SERVICE_LIFECYCLE_CAPABILITY {
            continue;
        }
        if provider.status != ProviderStatus::Available {
            return Err(CliError::unsupported(
                "provider_unavailable",
                format!("provider declaration {name} is not available"),
                &provider.capability,
                provider.implementation.as_deref().unwrap_or("placeholder"),
            ));
        }
    }
    let lifecycle_available = manifest.providers.values().any(|provider| {
        provider.capability == SERVICE_LIFECYCLE_CAPABILITY && provider.status == ProviderStatus::Available
    });
    if lifecycle_available && manifest.services.values().any(|service| service.health_check.is_some()) {
        return Err(CliError::unsupported(
            "health_check_unsupported",
            "health-check enforcement is not represented by the current Runtime port",
            "runtime.health_check",
            "podman",
        ));
    }
    if !manifest.artifacts.is_empty() {
        return Err(CliError::unsupported(
            "artifact_publication_unsupported",
            "declared artifact publication is not implemented in Slice 1",
            "publisher.artifacts",
            "podman-local",
        ));
    }
    if manifest.teardown.remove_volumes {
        return Err(CliError::unsupported(
            "volume_teardown_unsupported",
            "volume removal is not represented by the current Runtime port",
            "runtime.volume_teardown",
            "podman",
        ));
    }
    for service in manifest.services.values() {
        if let Some(gpu) = service.resources.as_ref().and_then(|r| r.gpu.as_ref()) {
            if gpu.vendor != GpuVendor::Nvidia {
                return Err(CliError::unsupported(
                    "gpu_vendor_unsupported",
                    "Slice 1 only maps NVIDIA UUID selectors to Podman CDI devices",
                    "runtime.gpu_uuid",
                    "podman",
                ));
            }
        }
    }
    Ok(())
}

fn runtime_provider_name(provider: &RuntimeProvider) -> &'static str {
    match provider {
        RuntimeProvider::Podman => "podman",
        RuntimeProvider::Nvms => "nvms",
        RuntimeProvider::Placeholder => "placeholder",
    }
}

fn ensure_persisted_provider(state: &RunState) -> Result<()> {
    if state.provider == "podman" {
        Ok(())
    } else {
        Err(CliError::unsupported(
            "persisted_provider_unsupported",
            format!("cannot operate on persisted provider {}", state.provider),
            "runtime.persistence",
            &state.provider,
        ))
    }
}

fn rollback_containers_from_map(distribution: &Option<String>, containers: &BTreeMap<String, String>) {
    let runtime = PodmanRuntime::bare(distribution);
    for id in containers.values().rev() {
        let _ = runtime.stop(&ContainerId::new(id));
    }
}

fn build_lifecycle_request(
    manifest: &CompositionManifest,
    plan: &LifecyclePlan,
    run_id: &str,
    manifest_sha256: &str,
) -> Result<ServiceLifecycleRequest> {
    let mut services = BTreeMap::new();
    for (name, service) in &manifest.services {
        let resources = service.resources.as_ref();
        services.insert(
            name.clone(),
            ServiceDefinition {
                image: service.image.clone(),
                depends_on: service.depends_on.clone(),
                command: service.command.clone(),
                environment: service.environment.clone(),
                cpu_millis: resources.and_then(|value| value.cpu_millis),
                memory_bytes: resources.and_then(|value| value.memory_bytes),
                gpu_uuids: resources
                    .and_then(|value| value.gpu.as_ref())
                    .map(|gpu| gpu.uuids.clone())
                    .unwrap_or_default(),
            },
        );
    }
    let intents = plan
        .intents
        .iter()
        .map(|intent| ServiceLifecycleIntent {
            phase: match intent.phase {
                IntentPhase::Create => "create".to_owned(),
                IntentPhase::Start => "start".to_owned(),
            },
            service: intent.service.clone(),
            image: intent.image.clone(),
            depends_on: intent.depends_on.clone(),
        })
        .collect();
    Ok(ServiceLifecycleRequest {
        version: SERVICE_LIFECYCLE_VERSION.to_owned(),
        schema_version: LIFECYCLE_SCHEMA_VERSION.to_owned(),
        manifest_sha256: manifest_sha256.to_owned(),
        run_id: run_id.to_owned(),
        wsl_distribution: manifest.runtime.wsl_distribution.clone().unwrap_or_default(),
        podman_pipe: DEFAULT_PODMAN_PIPE.to_owned(),
        order: plan.order.clone(),
        intents,
        services,
    })
}

fn validate_lifecycle_result(request: &ServiceLifecycleRequest, result: &ServiceLifecycleResult) -> Result<()> {
    if result.version != SERVICE_LIFECYCLE_VERSION {
        return Err(CliError::backend(
            "nvms_version_mismatch",
            format!("unexpected NanoVMS lifecycle version {:?}", result.version),
        ));
    }
    if !result.success {
        let code = if result.error_code.is_empty() {
            "nvms_lifecycle_failed".to_owned()
        } else {
            result.error_code.clone()
        };
        return Err(CliError::backend(code, result.error_message.clone()));
    }
    if result.effective_engine != "podman" || result.resolved_provider != "podman" {
        return Err(CliError::backend(
            "nvms_route_mismatch",
            "NanoVMS did not attest the podman provider for service lifecycle",
        ));
    }
    if result.podman_pipe != request.podman_pipe {
        return Err(CliError::backend(
            "nvms_pipe_mismatch",
            "NanoVMS returned a different Podman pipe",
        ));
    }
    for service in &request.order {
        if !result.containers.contains_key(service) {
            return Err(CliError::backend(
                "nvms_container_missing",
                format!("NanoVMS lifecycle result is missing container for service {service}"),
            ));
        }
    }
    Ok(())
}

fn state_path(state_dir: &Path, run_id: &str) -> PathBuf {
    state_dir.join(format!("{run_id}.json"))
}

fn jobs_path(state_dir: &Path, run_id: &str) -> PathBuf {
    state_dir.join(format!("{run_id}.jobs"))
}

fn job_path(state_dir: &Path, run_id: &str, job_id: &str) -> PathBuf {
    jobs_path(state_dir, run_id).join(format!("{job_id}.json"))
}

fn load_state(state_dir: &Path, run_id: &str) -> Result<RunState> {
    validate_slug("run_id", run_id)?;
    let path = state_path(state_dir, run_id);
    let input = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::not_found("run_not_found", format!("no persisted run state for {run_id}"))
        } else {
            CliError::io("state_read", error)
        }
    })?;
    serde_json::from_str(&input).map_err(CliError::json)
}

fn save_state(state_dir: &Path, state: &RunState) -> Result<()> {
    fs::create_dir_all(state_dir).map_err(|error| CliError::io("state_dir_create", error))?;
    let path = state_path(state_dir, &state.run_id);
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(state).map_err(CliError::json)?;
    fs::write(&temporary, bytes).map_err(|error| CliError::io("state_write", error))?;
    fs::rename(&temporary, &path).map_err(|error| CliError::io("state_commit", error))
}

fn unix_seconds() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| CliError::backend("clock", error.to_string()))
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

#[derive(Debug)]
struct PrebuiltImageComposer;

impl Composer for PrebuiltImageComposer {
    fn compose(&self, manifest: &Manifest) -> std::result::Result<ComposedArtifact, ComposeError> {
        let image = manifest
            .tags
            .iter()
            .find(|(key, _)| key == "image")
            .map(|(_, value)| value)
            .ok_or_else(|| ComposeError::validation("prebuilt image tag is required"))?;
        Ok(ComposedArtifact::new(
            format!("{}:prebuilt", manifest.name),
            ImageRef::new(image),
        ))
    }

    fn name(&self) -> &str {
        "prebuilt-image"
    }
}

#[derive(Debug)]
struct PodmanLocalPublisher {
    distribution: Option<String>,
}

impl PodmanLocalPublisher {
    fn new(distribution: &Option<String>) -> Self {
        Self {
            distribution: distribution.clone(),
        }
    }

    fn probe(&self) -> Result<()> {
        let output = podman_output(&self.distribution, ["version", "--format", "json"])?;
        require_success("podman_unavailable", output).map(|_| ())
    }
}

impl Publisher for PodmanLocalPublisher {
    fn publish(
        &self,
        artifact: &ComposedArtifact,
        target: &PublishTarget,
    ) -> std::result::Result<PublishReceipt, PublishError> {
        if target.kind != "podman-local" || target.locator != artifact.image.as_ref() {
            return Err(PublishError::validation(
                "podman-local target must match the composed image",
            ));
        }
        let output = podman_output(&self.distribution, ["image", "exists", artifact.image.as_ref()])
            .map_err(|error| PublishError::transport(error.to_string()))?;
        if !output.status.success() {
            return Err(PublishError::transport(format!(
                "image {} is not present in Podman local storage",
                artifact.image
            )));
        }
        Ok(PublishReceipt::new(
            &artifact.id,
            target.clone(),
            format!("podman-local://{}", artifact.image),
        ))
    }

    fn name(&self) -> &str {
        "podman-local"
    }
}

#[derive(Debug)]
struct PodmanRuntime {
    distribution: Option<String>,
    container_name: Option<String>,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
    cpu_millis: Option<u32>,
    memory_bytes: Option<u64>,
    gpu_uuids: Vec<String>,
}

impl PodmanRuntime {
    fn bare(distribution: &Option<String>) -> Self {
        Self {
            distribution: distribution.clone(),
            container_name: None,
            command: Vec::new(),
            environment: BTreeMap::new(),
            cpu_millis: None,
            memory_bytes: None,
            gpu_uuids: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn for_service(distribution: &Option<String>, run_id: &str, service_name: &str, service: &Service) -> Self {
        let resources = service.resources.as_ref();
        Self {
            distribution: distribution.clone(),
            container_name: Some(format!("{run_id}-{service_name}")),
            command: service.command.clone(),
            environment: service.environment.clone(),
            cpu_millis: resources.and_then(|value| value.cpu_millis),
            memory_bytes: resources.and_then(|value| value.memory_bytes),
            gpu_uuids: resources
                .and_then(|value| value.gpu.as_ref())
                .map(|gpu| gpu.uuids.clone())
                .unwrap_or_default(),
        }
    }
}

impl Runtime for PodmanRuntime {
    fn spawn(&self, image: &ImageRef) -> std::result::Result<ContainerId, RuntimeError> {
        let name = self
            .container_name
            .as_ref()
            .ok_or_else(|| RuntimeError::validation("service configuration is required"))?;
        let mut arguments = vec![
            "run".to_owned(),
            "--detach".to_owned(),
            "--name".to_owned(),
            name.clone(),
        ];
        for (key, value) in &self.environment {
            arguments.extend(["--env".to_owned(), format!("{key}={value}")]);
        }
        if let Some(cpu_millis) = self.cpu_millis {
            arguments.extend(["--cpus".to_owned(), format!("{:.3}", f64::from(cpu_millis) / 1000.0)]);
        }
        if let Some(memory_bytes) = self.memory_bytes {
            arguments.extend(["--memory".to_owned(), memory_bytes.to_string()]);
        }
        for uuid in &self.gpu_uuids {
            arguments.extend(["--device".to_owned(), format!("nvidia.com/gpu={uuid}")]);
        }
        arguments.push(image.to_string());
        arguments.extend(self.command.iter().cloned());
        let output = podman_output_owned(&self.distribution, &arguments)
            .map_err(|error| RuntimeError::backend(error.to_string()))?;
        if !output.status.success() {
            return Err(RuntimeError::backend(output_message(&output)));
        }
        let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if id.is_empty() {
            return Err(RuntimeError::backend(
                "podman run succeeded without returning a container ID",
            ));
        }
        Ok(ContainerId::new(id))
    }

    fn stop(&self, id: &ContainerId) -> std::result::Result<(), RuntimeError> {
        let output = podman_output(&self.distribution, ["stop", id.as_ref()])
            .map_err(|error| RuntimeError::backend(error.to_string()))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(RuntimeError::backend(output_message(&output)))
        }
    }

    fn status(&self, id: &ContainerId) -> std::result::Result<ContainerStatus, RuntimeError> {
        let output = podman_output(
            &self.distribution,
            ["inspect", "--format", "{{.State.Status}}", id.as_ref()],
        )
        .map_err(|error| RuntimeError::backend(error.to_string()))?;
        if !output.status.success() {
            let message = output_message(&output);
            if message.to_ascii_lowercase().contains("no such") {
                return Ok(ContainerStatus::NotFound);
            }
            return Err(RuntimeError::backend(message));
        }
        match String::from_utf8_lossy(&output.stdout).trim() {
            "running" => Ok(ContainerStatus::Running),
            "paused" => Ok(ContainerStatus::Paused),
            "exited" | "stopped" | "configured" | "created" => Ok(ContainerStatus::Exited),
            status => Err(RuntimeError::backend(format!("unrecognized Podman status {status:?}"))),
        }
    }

    fn name(&self) -> &str {
        "podman"
    }
}

fn podman_output<'a>(distribution: &Option<String>, arguments: impl IntoIterator<Item = &'a str>) -> Result<Output> {
    let arguments: Vec<String> = arguments.into_iter().map(str::to_owned).collect();
    podman_output_owned(distribution, &arguments)
}

fn podman_output_owned(distribution: &Option<String>, arguments: &[String]) -> Result<Output> {
    let mut command = if let Some(distribution) = distribution {
        let mut command = Command::new("wsl.exe");
        command.args(["-d", distribution, "--", "podman"]);
        command
    } else {
        Command::new("podman")
    };
    command
        .args(arguments)
        .output()
        .map_err(|error| CliError::io("podman_spawn", error))
}

fn require_success(code: &str, output: Output) -> Result<Output> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(CliError::backend(code, output_message(&output)))
    }
}

fn output_message(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        stderr
    }
}

fn map_compose_error(error: ComposeError) -> CliError {
    CliError::backend("compose_failed", error.to_string())
}

fn map_publish_error(error: PublishError) -> CliError {
    CliError::backend("publish_failed", error.to_string())
}

pub fn map_runtime_error(error: RuntimeError) -> CliError {
    match error {
        RuntimeError::NotFound(message) => CliError::not_found("runtime_not_found", message),
        RuntimeError::Validation(message) => CliError::validation("runtime_validation", message),
        RuntimeError::Backend(message) => CliError::backend("runtime_backend", message),
        other => CliError::backend("runtime_unknown", other.to_string()),
    }
}

#[cfg(test)]
mod lifecycle_bridge_tests {
    use super::*;
    use crate::model::CompositionManifest;

    #[test]
    fn lifecycle_request_maps_manifest_services_and_intents() {
        let manifest = CompositionManifest::parse(include_str!("../../../examples/composition-v0.yaml")).unwrap();
        let plan = manifest.plan().unwrap();
        let lifecycle = plan_lifecycle(&manifest, &plan.manifest_sha256).unwrap();
        let request = build_lifecycle_request(&manifest, &lifecycle, "run-id", &plan.manifest_sha256).unwrap();
        assert_eq!(request.version, SERVICE_LIFECYCLE_VERSION);
        assert_eq!(request.schema_version, LIFECYCLE_SCHEMA_VERSION);
        assert_eq!(request.intents[0].phase, "create");
        assert_eq!(request.services["worker"].gpu_uuids[0], "GPU-123e4567-e89b-12d3-a456-426614174000");
    }

    #[test]
    fn lifecycle_result_validation_requires_podman_attestation() {
        let request = ServiceLifecycleRequest {
            version: SERVICE_LIFECYCLE_VERSION.to_owned(),
            schema_version: LIFECYCLE_SCHEMA_VERSION.to_owned(),
            manifest_sha256: "0".repeat(64),
            run_id: "run".to_owned(),
            wsl_distribution: String::new(),
            podman_pipe: DEFAULT_PODMAN_PIPE.to_owned(),
            order: vec!["worker".to_owned()],
            intents: Vec::new(),
            services: BTreeMap::new(),
        };
        let result = ServiceLifecycleResult {
            version: SERVICE_LIFECYCLE_VERSION.to_owned(),
            success: true,
            error_code: String::new(),
            error_message: String::new(),
            containers: BTreeMap::from([("worker".to_owned(), "abc".to_owned())]),
            effective_engine: "gvisor".to_owned(),
            resolved_provider: "podman".to_owned(),
            podman_pipe: DEFAULT_PODMAN_PIPE.to_owned(),
        };
        let error = validate_lifecycle_result(&request, &result).unwrap_err();
        assert_eq!(error.code, "nvms_route_mismatch");
    }
}
