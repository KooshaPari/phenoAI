use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use wait_timeout::ChildExt;

use crate::CliError;

pub const ACTION_VERSION: &str = "nanovms.io/evaluation-action/v1";
pub const SERVICE_LIFECYCLE_VERSION: &str = "nanovms.io/service-lifecycle/v1";
pub const RESOURCE_VERSION: &str = "nanovms.io/resources/v1";
pub const DEFAULT_PODMAN_PIPE: &str = "npipe:////./pipe/podman-machine-default";
const STDERR_LIMIT: usize = 64 * 1024;
const RESPONSE_LIMIT: usize = 34 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationRequest {
    pub version: String,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_backends: Vec<String>,
    pub manifest_sha256: String,
    pub executable: String,
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    pub external_engine_token: String,
    pub podman_pipe: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wsl_distribution: String,
    pub output_root: String,
    pub reservation_path: String,
    pub lock_invocation: Vec<String>,
    pub resource_manifest: ResourceManifest,
    pub gpu_bindings: Vec<GpuBinding>,
    pub timeout_millis: i64,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GpuBinding {
    pub uuid: String,
    pub cuda_toolkit: String,
    pub cdi_device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceManifest {
    pub version: String,
    pub gpus: Vec<ResourceGpu>,
    pub artifact: ArtifactRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceGpu {
    pub uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compute_capability: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub driver_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub driver_cuda_ceiling: String,
    pub observations: Vec<ResourceObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceObservation {
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope_id: String,
    pub observed_index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRequirements {
    pub cuda_toolkit: String,
    pub compiled_kernels: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationResult {
    pub version: String,
    pub success: bool,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub error_message: String,
    pub lifecycle: Lifecycle,
    pub provenance: EvaluationProvenance,
    pub released: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Lifecycle {
    pub exit_code: i32,
    pub duration_ms: i64,
    pub timed_out: bool,
    pub truncated: bool,
    pub stdout: String,
    pub stderr: String,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvaluationProvenance {
    pub manifest_sha256: String,
    pub effective_engine: String,
    pub resolved_provider: String,
    pub execution_plane: String,
    pub podman_pipe: String,
    pub gpu_uuids: Vec<String>,
    #[serde(default)]
    pub job_directory: String,
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
}

#[derive(Debug)]
pub struct EvaluationFailure {
    pub error: CliError,
    pub lifecycle: Lifecycle,
}

impl EvaluationFailure {
    pub fn new(error: CliError, stdout: &[u8], stderr: &[u8]) -> Box<Self> {
        Box::new(Self {
            error,
            lifecycle: Lifecycle::local_failure(stdout, stderr),
        })
    }
}

impl Lifecycle {
    pub fn local_failure(stdout: &[u8], stderr: &[u8]) -> Self {
        Self {
            exit_code: -1,
            duration_ms: 0,
            timed_out: false,
            truncated: false,
            stdout: String::from_utf8_lossy(stdout).into_owned(),
            stderr: String::from_utf8_lossy(stderr).into_owned(),
            stdout_sha256: sha256(stdout),
            stderr_sha256: sha256(stderr),
        }
    }
}

pub trait EvaluationClient {
    fn execute(&self, request: &EvaluationRequest) -> std::result::Result<EvaluationResult, Box<EvaluationFailure>>;
}

#[derive(Debug, Default)]
pub struct ProcessEvaluationClient;

impl EvaluationClient for ProcessEvaluationClient {
    fn execute(&self, request: &EvaluationRequest) -> std::result::Result<EvaluationResult, Box<EvaluationFailure>> {
        let request_bytes =
            serde_json::to_vec(request).map_err(|error| EvaluationFailure::new(CliError::json(error), b"", b""))?;
        let executable = resolve_nvms_bin();
        let mut child = Command::new(&executable)
            .args(["action", "--request", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| EvaluationFailure::new(CliError::io("nvms_spawn", error), b"", b""))?;

        let Some(mut stdin) = child.stdin.take() else {
            terminate(&mut child);
            return Err(EvaluationFailure::new(
                CliError::backend("nvms_stdin", "NanoVMS stdin was unavailable"),
                b"",
                b"",
            ));
        };
        if let Err(error) = stdin.write_all(&request_bytes).and_then(|()| stdin.write_all(b"\n")) {
            drop(stdin);
            terminate(&mut child);
            return Err(EvaluationFailure::new(
                CliError::io("nvms_request_write", error),
                b"",
                b"",
            ));
        }
        drop(stdin);

        let Some(stdout) = child.stdout.take() else {
            terminate(&mut child);
            return Err(EvaluationFailure::new(
                CliError::backend("nvms_stdout", "NanoVMS stdout was unavailable"),
                b"",
                b"",
            ));
        };
        let Some(stderr) = child.stderr.take() else {
            terminate(&mut child);
            return Err(EvaluationFailure::new(
                CliError::backend("nvms_stderr", "NanoVMS stderr was unavailable"),
                b"",
                b"",
            ));
        };
        let stdout_reader = thread::spawn(move || read_bounded(stdout, RESPONSE_LIMIT));
        let stderr_reader = thread::spawn(move || read_bounded(stderr, STDERR_LIMIT));

        let outer_timeout = Duration::from_millis(
            u64::try_from(request.timeout_millis)
                .unwrap_or(0)
                .saturating_add(10_000),
        );
        let waited = match child.wait_timeout(outer_timeout) {
            Ok(waited) => waited,
            Err(error) => {
                terminate(&mut child);
                let stdout = join_reader(stdout_reader, "nvms_stdout_read")?;
                let stderr = join_reader(stderr_reader, "nvms_stderr_read")?;
                return Err(EvaluationFailure::new(
                    CliError::io("nvms_wait", error),
                    &stdout.bytes,
                    &stderr.bytes,
                ));
            }
        };
        let status = match waited {
            Some(status) => status,
            None => {
                terminate(&mut child);
                let stdout = join_reader(stdout_reader, "nvms_stdout_read")?;
                let stderr = join_reader(stderr_reader, "nvms_stderr_read")?;
                let mut failure = EvaluationFailure::new(
                    CliError::backend(
                        "nvms_timeout",
                        format!("NanoVMS exceeded outer timeout; stderr={}", render_stderr(&stderr)),
                    ),
                    &stdout.bytes,
                    &stderr.bytes,
                );
                failure.lifecycle.timed_out = true;
                failure.lifecycle.truncated = stdout.truncated || stderr.truncated;
                return Err(failure);
            }
        };
        let stdout = join_reader(stdout_reader, "nvms_stdout_read")?;
        let stderr = join_reader(stderr_reader, "nvms_stderr_read")?;
        if stdout.truncated {
            let mut failure = EvaluationFailure::new(
                CliError::backend(
                    "nvms_response_too_large",
                    "NanoVMS response exceeded the bounded response size",
                ),
                &stdout.bytes,
                &stderr.bytes,
            );
            failure.lifecycle.truncated = true;
            return Err(failure);
        }

        let result: EvaluationResult = serde_json::from_slice(&stdout.bytes).map_err(|error| {
            EvaluationFailure::new(
                CliError::backend(
                    "nvms_malformed_response",
                    format!("{error}; stderr={}", render_stderr(&stderr)),
                ),
                &stdout.bytes,
                &stderr.bytes,
            )
        })?;
        match status.code() {
            Some(0) | Some(4) => Ok(result),
            Some(2) => Err(result_failure(exit_error("nvms_usage", status.code(), &stderr), result)),
            Some(3) => Err(result_failure(
                exit_error("nvms_request_rejected", status.code(), &stderr),
                result,
            )),
            Some(5) => Err(result_failure(
                exit_error("nvms_encode_failed", status.code(), &stderr),
                result,
            )),
            code => Err(result_failure(exit_error("nvms_process_failed", code, &stderr), result)),
        }
    }
}

fn result_failure(error: CliError, result: EvaluationResult) -> Box<EvaluationFailure> {
    Box::new(EvaluationFailure {
        error,
        lifecycle: result.lifecycle,
    })
}

fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn resolve_nvms_bin() -> OsString {
    env::var_os("NVMS_BIN")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "nvms".into())
}

struct BoundedBytes {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<BoundedBytes> {
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..count.min(remaining)]);
        truncated |= count > remaining;
    }
    Ok(BoundedBytes { bytes, truncated })
}

fn join_reader(
    handle: thread::JoinHandle<std::io::Result<BoundedBytes>>,
    code: &str,
) -> std::result::Result<BoundedBytes, Box<EvaluationFailure>> {
    handle
        .join()
        .map_err(|_| EvaluationFailure::new(CliError::backend(code, "NanoVMS output reader panicked"), b"", b""))?
        .map_err(|error| EvaluationFailure::new(CliError::io(code, error), b"", b""))
}

fn render_stderr(stderr: &BoundedBytes) -> String {
    let mut value = String::from_utf8_lossy(&stderr.bytes).trim().to_owned();
    if stderr.truncated {
        value.push_str("…[truncated]");
    }
    value
}

fn exit_error(code: &str, status: Option<i32>, stderr: &BoundedBytes) -> CliError {
    CliError::backend(
        code,
        format!(
            "NanoVMS exited with code {}; stderr={}",
            status.map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
            render_stderr(stderr)
        ),
    )
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceLifecycleRequest {
    pub version: String,
    pub schema_version: String,
    pub manifest_sha256: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wsl_distribution: String,
    pub podman_pipe: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    pub intents: Vec<ServiceLifecycleIntent>,
    pub services: BTreeMap<String, ServiceDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceLifecycleIntent {
    pub phase: String,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceDefinition {
    pub image: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_millis: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpu_uuids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceLifecycleResult {
    pub version: String,
    pub success: bool,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub error_message: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub containers: BTreeMap<String, String>,
    pub effective_engine: String,
    pub resolved_provider: String,
    pub podman_pipe: String,
}

#[derive(Debug)]
pub struct ServiceLifecycleFailure {
    pub error: CliError,
    pub result: ServiceLifecycleResult,
}

pub trait LifecycleClient {
    fn execute(
        &self,
        request: &ServiceLifecycleRequest,
    ) -> std::result::Result<ServiceLifecycleResult, Box<ServiceLifecycleFailure>>;
}

#[derive(Debug, Default)]
pub struct ProcessLifecycleClient;

impl LifecycleClient for ProcessLifecycleClient {
    fn execute(
        &self,
        request: &ServiceLifecycleRequest,
    ) -> std::result::Result<ServiceLifecycleResult, Box<ServiceLifecycleFailure>> {
        let request_bytes =
            serde_json::to_vec(request).map_err(|error| Box::new(ServiceLifecycleFailure {
                error: CliError::json(error),
                result: ServiceLifecycleResult {
                    version: SERVICE_LIFECYCLE_VERSION.to_owned(),
                    success: false,
                    error_code: String::new(),
                    error_message: String::new(),
                    containers: BTreeMap::new(),
                    effective_engine: String::new(),
                    resolved_provider: String::new(),
                    podman_pipe: request.podman_pipe.clone(),
                },
            }))?;
        let executable = resolve_nvms_bin();
        let mut child = Command::new(&executable)
            .args(["lifecycle", "--request", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| Box::new(ServiceLifecycleFailure {
                error: CliError::io("nvms_spawn", error),
                result: empty_lifecycle_result(request),
            }))?;

        let Some(mut stdin) = child.stdin.take() else {
            terminate(&mut child);
            return Err(Box::new(ServiceLifecycleFailure {
                error: CliError::backend("nvms_stdin", "NanoVMS stdin was unavailable"),
                result: empty_lifecycle_result(request),
            }));
        };
        if let Err(error) = stdin.write_all(&request_bytes).and_then(|()| stdin.write_all(b"\n")) {
            drop(stdin);
            terminate(&mut child);
            return Err(Box::new(ServiceLifecycleFailure {
                error: CliError::io("nvms_request_write", error),
                result: empty_lifecycle_result(request),
            }));
        }
        drop(stdin);

        let output = child
            .wait_with_output()
            .map_err(|error| Box::new(ServiceLifecycleFailure {
                error: CliError::io("nvms_wait", error),
                result: empty_lifecycle_result(request),
            }))?;
        let result: ServiceLifecycleResult = serde_json::from_slice(&output.stdout).map_err(|error| {
            Box::new(ServiceLifecycleFailure {
                error: CliError::backend(
                    "nvms_malformed_response",
                    format!(
                        "{error}; stderr={}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ),
                ),
                result: empty_lifecycle_result(request),
            })
        })?;
        match output.status.code() {
            Some(0) | Some(4) => Ok(result),
            Some(2) => Err(lifecycle_failure(
                exit_error("nvms_usage", output.status.code(), &BoundedBytes {
                    bytes: output.stderr.clone(),
                    truncated: false,
                }),
                result,
            )),
            Some(3) => Err(lifecycle_failure(
                exit_error("nvms_request_rejected", output.status.code(), &BoundedBytes {
                    bytes: output.stderr.clone(),
                    truncated: false,
                }),
                result,
            )),
            Some(5) => Err(lifecycle_failure(
                exit_error("nvms_encode_failed", output.status.code(), &BoundedBytes {
                    bytes: output.stderr.clone(),
                    truncated: false,
                }),
                result,
            )),
            code => Err(lifecycle_failure(
                exit_error("nvms_process_failed", code, &BoundedBytes {
                    bytes: output.stderr.clone(),
                    truncated: false,
                }),
                result,
            )),
        }
    }
}

fn empty_lifecycle_result(request: &ServiceLifecycleRequest) -> ServiceLifecycleResult {
    ServiceLifecycleResult {
        version: SERVICE_LIFECYCLE_VERSION.to_owned(),
        success: false,
        error_code: String::new(),
        error_message: String::new(),
        containers: BTreeMap::new(),
        effective_engine: String::new(),
        resolved_provider: String::new(),
        podman_pipe: request.podman_pipe.clone(),
    }
}

fn lifecycle_failure(error: CliError, result: ServiceLifecycleResult) -> Box<ServiceLifecycleFailure> {
    Box::new(ServiceLifecycleFailure { error, result })
}
