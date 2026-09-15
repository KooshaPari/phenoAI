use phenocompose_cli::model::{CompositionManifest, GpuRequirements, GpuVendor, ResourceRequirements, Service};
use phenocompose_cli::nvms::{
    EvaluationClient, EvaluationFailure, EvaluationProvenance, EvaluationRequest, EvaluationResult, Lifecycle,
    ResourceGpu, ACTION_VERSION,
};
use phenocompose_cli::{
    export_provenance, load_job_provenance, run_action_with_client, run_action_with_client_at, CliError, RunLifecycle,
    RunState,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tempfile::TempDir;

const RUN_ID: &str = "slice1-run";
const UUID_A: &str = "GPU-123e4567-e89b-12d3-a456-426614174000";
const UUID_B: &str = "GPU-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

struct FakeClient {
    result: Mutex<Option<Result<EvaluationResult, Box<EvaluationFailure>>>>,
    request: Mutex<Option<EvaluationRequest>>,
}

impl FakeClient {
    fn result(result: EvaluationResult) -> Self {
        Self {
            result: Mutex::new(Some(Ok(result))),
            request: Mutex::new(None),
        }
    }

    fn failure(code: &str, stdout: &[u8], stderr: &[u8]) -> Self {
        Self {
            result: Mutex::new(Some(Err(EvaluationFailure::new(
                CliError::backend(code, "injected client failure"),
                stdout,
                stderr,
            )))),
            request: Mutex::new(None),
        }
    }
}

impl EvaluationClient for FakeClient {
    fn execute(&self, request: &EvaluationRequest) -> Result<EvaluationResult, Box<EvaluationFailure>> {
        *self.request.lock().unwrap() = Some(request.clone());
        let result = self.result.lock().unwrap().take().unwrap();
        result.map(|mut result| {
            if result.provenance.job_directory == "job-output" {
                result.provenance.job_directory = Path::new(&request.output_root)
                    .join("job-output")
                    .to_str()
                    .unwrap()
                    .to_owned();
            }
            result
        })
    }
}

fn setup(mut edit: impl FnMut(&mut CompositionManifest)) -> (TempDir, RunState) {
    let mut manifest = CompositionManifest::parse(include_str!("../../../examples/composition-v0.yaml")).unwrap();
    manifest.environment.toolkit.version = "12.8".to_owned();
    manifest.providers.clear();
    manifest.artifacts.clear();
    manifest.services.get_mut("worker").unwrap().health_check = None;
    edit(&mut manifest);
    manifest.validate().unwrap();
    let digest = manifest.plan().unwrap().manifest_sha256;
    let state = RunState {
        state_version: "phenocompose.run/v0".to_owned(),
        run_id: RUN_ID.to_owned(),
        manifest_sha256: digest,
        provider: "podman".to_owned(),
        created_unix_seconds: 1,
        lifecycle: RunLifecycle::Running,
        containers: BTreeMap::from([("worker".to_owned(), "container-id".to_owned())]),
        manifest,
    };
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join(format!("{RUN_ID}.json")),
        serde_json::to_vec_pretty(&state).unwrap(),
    )
    .unwrap();
    (directory, state)
}

fn success_result(state: &RunState, uuids: Vec<String>) -> EvaluationResult {
    let stdout = "ok".to_owned();
    let stderr = "note".to_owned();
    EvaluationResult {
        version: ACTION_VERSION.to_owned(),
        success: true,
        error_code: String::new(),
        error_message: String::new(),
        lifecycle: Lifecycle {
            exit_code: 0,
            duration_ms: 25,
            timed_out: false,
            truncated: false,
            stdout_sha256: hash(&stdout),
            stderr_sha256: hash(&stderr),
            stdout,
            stderr,
        },
        provenance: EvaluationProvenance {
            manifest_sha256: state.manifest_sha256.clone(),
            effective_engine: "podman".to_owned(),
            resolved_provider: "podman".to_owned(),
            execution_plane: "nanovms".to_owned(),
            podman_pipe: "npipe:////./pipe/podman-machine-default".to_owned(),
            gpu_uuids: uuids,
            job_directory: "job-output".to_owned(),
            output_root_created: false,
            output_root_available_bytes: None,
            toolkit_version: None,
            toolkit_root: None,
            toolkit_executable: None,
        },
        released: true,
    }
}

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn result_with_output_root_fields(
    state: &RunState,
    created: bool,
    available_bytes: serde_json::Value,
) -> EvaluationResult {
    let mut value = serde_json::to_value(success_result(state, vec![UUID_A.to_owned()])).unwrap();
    value["provenance"]["output_root_created"] = created.into();
    value["provenance"]["output_root_available_bytes"] = available_bytes;
    serde_json::from_value(value).unwrap()
}

fn result_with_toolkit_fields(
    state: &RunState,
    version: Option<&str>,
    root: Option<&str>,
    executable: Option<&str>,
) -> EvaluationResult {
    let mut value = serde_json::to_value(success_result(state, vec![UUID_A.to_owned()])).unwrap();
    let provenance = &mut value["provenance"];
    if let Some(version) = version {
        provenance["toolkit_version"] = version.into();
    }
    if let Some(root) = root {
        provenance["toolkit_root"] = root.into();
    }
    if let Some(executable) = executable {
        provenance["toolkit_executable"] = executable.into();
    }
    serde_json::from_value(value).unwrap()
}

fn gpu_service(image: &str, uuid: &str) -> Service {
    Service {
        image: image.to_owned(),
        depends_on: Vec::new(),
        command: Vec::new(),
        environment: BTreeMap::new(),
        resources: Some(ResourceRequirements {
            cpu_millis: None,
            memory_bytes: None,
            gpu: Some(GpuRequirements {
                vendor: GpuVendor::Nvidia,
                uuids: vec![uuid.to_owned()],
            }),
        }),
        health_check: None,
    }
}

#[test]
fn maps_action_and_transitive_gpu_closure_without_runtime_commands() {
    let (directory, state) = setup(|manifest| {
        manifest
            .services
            .insert("base".to_owned(), gpu_service("base:image", UUID_B));
        manifest
            .services
            .get_mut("worker")
            .unwrap()
            .depends_on
            .push("base".to_owned());
    });
    let expected = vec![UUID_A.to_owned(), UUID_B.to_owned()];
    let client = FakeClient::result(success_result(&state, expected.clone()));
    let job = run_action_with_client(directory.path(), RUN_ID, "inspect-worker", "mapping", &client).unwrap();
    let request = client.request.lock().unwrap().clone().unwrap();

    assert_eq!(request.executable, "/usr/bin/env");
    assert!(request.argv.is_empty());
    assert_eq!(request.backend, "podman");
    assert!(request.fallback_backends.is_empty());
    assert_eq!(
        request
            .gpu_bindings
            .iter()
            .map(|binding| binding.uuid.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert!(request
        .gpu_bindings
        .iter()
        .all(|binding| binding.cuda_toolkit == "12.8"));
    assert_eq!(job.dependency_services, vec!["base", "worker"]);
    let serialized = serde_json::to_string(&request).unwrap();
    assert!(!serialized.contains("\"podman run\""));
    assert!(!serialized.contains("\"wsl.exe\""));
}

#[test]
fn uuid_and_toolkit_only_requests_omit_unverified_gpu_identity() {
    let (directory, state) = setup(|_| {});
    let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));

    run_action_with_client(directory.path(), RUN_ID, "inspect-worker", "identity-omitted", &client).unwrap();

    let request = client.request.lock().unwrap().clone().unwrap();
    let json = serde_json::to_value(&request).unwrap();
    let gpu = &json["resource_manifest"]["gpus"][0];
    assert_eq!(gpu["uuid"], UUID_A);
    assert_eq!(json["resource_manifest"]["artifact"]["cuda_toolkit"], "12.8");
    for field in ["name", "architecture", "compute_capability"] {
        assert!(gpu.get(field).is_none(), "unexpected synthetic GPU claim {field}");
    }
    let forbidden = ["PhenoCompose", "declared", "GPU"].join(" ");
    assert!(!serde_json::to_string(&request).unwrap().contains(&forbidden));
}

#[test]
fn accepts_bare_gpu_uuid_in_run_action() {
    let bare = "123e4567-e89b-12d3-a456-426614174000";
    let (directory, state) = setup(|manifest| {
        manifest.services.get_mut("worker").unwrap().resources.as_mut().unwrap().gpu.as_mut().unwrap().uuids =
            vec![bare.to_owned()];
    });
    let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));
    run_action_with_client(directory.path(), RUN_ID, "inspect-worker", "bare-uuid", &client).unwrap();
    assert_eq!(client.request.lock().unwrap().clone().unwrap().gpu_bindings[0].uuid, UUID_A);
}

#[test]
fn explicit_gpu_identity_declarations_are_preserved_exactly() {
    let gpu = ResourceGpu {
        uuid: UUID_A.to_owned(),
        name: Some("NVIDIA GeForce RTX 3090 Ti".to_owned()),
        architecture: Some("Ampere".to_owned()),
        compute_capability: Some("8.6".to_owned()),
        driver_version: String::new(),
        driver_cuda_ceiling: String::new(),
        observations: Vec::new(),
    };

    let json = serde_json::to_value(&gpu).unwrap();
    assert_eq!(json["name"], "NVIDIA GeForce RTX 3090 Ti");
    assert_eq!(json["architecture"], "Ampere");
    assert_eq!(json["compute_capability"], "8.6");
    assert_eq!(serde_json::from_value::<ResourceGpu>(json).unwrap(), gpu);
}

#[test]
fn resolves_relative_output_root_against_workspace_and_serializes_exactly() {
    let (directory, state) = setup(|_| {});
    let workspace = tempfile::tempdir().unwrap();
    let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));

    let job = run_action_with_client_at(
        workspace.path(),
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "relative-root",
        &client,
    )
    .unwrap();

    let request = client.request.lock().unwrap().clone().unwrap();
    let expected = workspace.path().join("jobs").join("harbor");
    assert_eq!(request.output_root, expected.to_str().unwrap());
    assert_eq!(job.output_root, request.output_root);
    assert_ne!(job.output_root, directory.path().to_str().unwrap());
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["output_root"], expected.to_str().unwrap());
    assert!(!directory.path().join(format!("{RUN_ID}.outputs")).exists());
}

#[test]
fn accepts_fully_absolute_output_root_without_rebasing() {
    let workspace = tempfile::tempdir().unwrap();
    let absolute = workspace.path().join("absolute-output");
    let (directory, state) = setup(|manifest| {
        manifest.actions.get_mut("inspect-worker").unwrap().output_root = Some(absolute.to_str().unwrap().to_owned());
    });
    let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));

    run_action_with_client_at(
        workspace.path(),
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "absolute-root",
        &client,
    )
    .unwrap();

    assert_eq!(
        client.request.lock().unwrap().as_ref().unwrap().output_root,
        absolute.to_str().unwrap()
    );
}

#[test]
fn accepts_present_missing_zero_and_large_output_root_fields() {
    let (_, state) = setup(|_| {});
    for (created, available) in [
        (true, serde_json::json!(0)),
        (false, serde_json::json!(u64::MAX)),
        (true, serde_json::Value::Null),
    ] {
        let result = result_with_output_root_fields(&state, created, available.clone());
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["provenance"]["output_root_created"], created);
        assert_eq!(value["provenance"]["output_root_available_bytes"], available);
    }

    let result = success_result(&state, vec![UUID_A.to_owned()]);
    let value = serde_json::to_value(&result).unwrap();
    let decoded: EvaluationResult = serde_json::from_value(value).unwrap();
    let decoded = serde_json::to_value(decoded).unwrap();
    assert_eq!(decoded["provenance"]["output_root_created"], false);
    assert!(decoded["provenance"]["output_root_available_bytes"].is_null());
}

#[test]
fn accepts_present_and_missing_toolkit_fields() {
    let (_, state) = setup(|_| {});
    for (version, root, executable) in [
        (Some("12.8"), Some("/opt/cuda/12.8"), Some("/opt/cuda/12.8/bin/nvcc")),
        (Some("12.8"), None, None),
        (None, None, None),
    ] {
        let result = result_with_toolkit_fields(&state, version, root, executable);
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(
            value["provenance"]["toolkit_version"],
            version.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null)
        );
        assert_eq!(
            value["provenance"]["toolkit_root"],
            root.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null)
        );
        assert_eq!(
            value["provenance"]["toolkit_executable"],
            executable
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null)
        );
    }

    let result = success_result(&state, vec![UUID_A.to_owned()]);
    let value = serde_json::to_value(&result).unwrap();
    let decoded: EvaluationResult = serde_json::from_value(value).unwrap();
    let decoded = serde_json::to_value(decoded).unwrap();
    assert!(decoded["provenance"]["toolkit_version"].is_null());
    assert!(decoded["provenance"]["toolkit_root"].is_null());
    assert!(decoded["provenance"]["toolkit_executable"].is_null());
}

#[test]
fn toolkit_fields_remain_strictly_typed_and_unknown_fields_are_rejected() {
    let (_, state) = setup(|_| {});
    let base = serde_json::to_value(success_result(&state, vec![UUID_A.to_owned()])).unwrap();

    let mut unknown = base;
    unknown["provenance"]["unexpected_toolkit_field"] = true.into();
    assert!(serde_json::from_value::<EvaluationResult>(unknown).is_err());
}

#[test]
fn persists_and_exports_toolkit_fields_on_success() {
    let (directory, state) = setup(|_| {});
    let job = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "toolkit-success",
        &FakeClient::result(result_with_toolkit_fields(
            &state,
            Some("12.8"),
            Some("/opt/cuda/12.8"),
            Some("/opt/cuda/12.8/bin/nvcc"),
        )),
    )
    .unwrap();

    let persisted = serde_json::to_value(&job).unwrap();
    assert_eq!(persisted["toolkit_version"], "12.8");
    assert_eq!(persisted["toolkit_root"], "/opt/cuda/12.8");
    assert_eq!(persisted["toolkit_executable"], "/opt/cuda/12.8/bin/nvcc");
    let exported = serde_json::to_value(export_provenance(directory.path(), RUN_ID).unwrap()).unwrap();
    assert_eq!(exported["jobs"]["toolkit-success"]["toolkit_version"], "12.8");
    assert_eq!(exported["jobs"]["toolkit-success"]["toolkit_root"], "/opt/cuda/12.8");
    assert_eq!(
        exported["jobs"]["toolkit-success"]["toolkit_executable"],
        "/opt/cuda/12.8/bin/nvcc"
    );
}

#[test]
fn missing_toolkit_fields_persist_backward_compatible_defaults() {
    let (directory, state) = setup(|_| {});
    let job = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "older-nanovms-toolkit",
        &FakeClient::result(success_result(&state, vec![UUID_A.to_owned()])),
    )
    .unwrap();

    let value = serde_json::to_value(job).unwrap();
    assert!(value.get("toolkit_version").is_none());
    assert!(value.get("toolkit_root").is_none());
    assert!(value.get("toolkit_executable").is_none());
}

#[test]
fn persists_toolkit_fields_on_failure_response() {
    let (directory, state) = setup(|_| {});
    let mut result = result_with_toolkit_fields(&state, Some("12.8"), Some("/opt/cuda/12.8"), None);
    result.success = false;
    result.error_code = "toolkit_probe_failed".to_owned();
    result.error_message = "toolkit unavailable".to_owned();
    result.lifecycle.exit_code = -1;

    let error = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "toolkit-failure",
        &FakeClient::result(result),
    )
    .unwrap_err();
    assert_eq!(error.code, "toolkit_probe_failed");

    let job = load_job_provenance(directory.path(), RUN_ID, "toolkit-failure").unwrap();
    let value = serde_json::to_value(&job).unwrap();
    assert_eq!(value["toolkit_version"], "12.8");
    assert_eq!(value["toolkit_root"], "/opt/cuda/12.8");
    assert!(value.get("toolkit_executable").is_none());
    assert_eq!(value["error_code"], "toolkit_probe_failed");
}

#[test]
fn output_root_fields_remain_strictly_typed_and_unknown_fields_are_rejected() {
    let (_, state) = setup(|_| {});
    let base = serde_json::to_value(success_result(&state, vec![UUID_A.to_owned()])).unwrap();

    let mut negative = base.clone();
    negative["provenance"]["output_root_available_bytes"] = serde_json::json!(-1);
    assert!(serde_json::from_value::<EvaluationResult>(negative).is_err());

    let mut unknown = base;
    unknown["provenance"]["unexpected_output_root_field"] = true.into();
    assert!(serde_json::from_value::<EvaluationResult>(unknown).is_err());
}

#[test]
fn persists_and_exports_output_root_fields_on_success() {
    let (directory, state) = setup(|_| {});
    let job = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "output-root-success",
        &FakeClient::result(result_with_output_root_fields(
            &state,
            true,
            serde_json::json!(u64::MAX),
        )),
    )
    .unwrap();

    let persisted = serde_json::to_value(&job).unwrap();
    assert_eq!(persisted["output_root_created"], true);
    assert_eq!(persisted["output_root_available_bytes"], serde_json::json!(u64::MAX));
    assert!(Path::new(persisted["output_root"].as_str().unwrap()).is_absolute());
    let exported = serde_json::to_value(export_provenance(directory.path(), RUN_ID).unwrap()).unwrap();
    assert_eq!(exported["jobs"]["output-root-success"]["output_root_created"], true);
    assert_eq!(
        exported["jobs"]["output-root-success"]["output_root_available_bytes"],
        serde_json::json!(u64::MAX)
    );
}

#[test]
fn missing_output_root_fields_persist_backward_compatible_defaults() {
    let (directory, state) = setup(|_| {});
    let job = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "older-nanovms",
        &FakeClient::result(success_result(&state, vec![UUID_A.to_owned()])),
    )
    .unwrap();

    let value = serde_json::to_value(job).unwrap();
    assert_eq!(value["output_root_created"], false);
    assert!(value["output_root_available_bytes"].is_null());
}

#[test]
fn persists_output_root_fields_and_complete_evidence_on_failure_response() {
    let (directory, state) = setup(|_| {});
    let mut result = result_with_output_root_fields(&state, true, serde_json::json!(0));
    result.success = false;
    result.error_code = "output_root_space_failed".to_owned();
    result.error_message = "space unavailable".to_owned();
    result.lifecycle.exit_code = -1;

    let error = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "output-root-failure",
        &FakeClient::result(result),
    )
    .unwrap_err();
    assert_eq!(error.code, "output_root_space_failed");

    let job = load_job_provenance(directory.path(), RUN_ID, "output-root-failure").unwrap();
    let value = serde_json::to_value(&job).unwrap();
    assert_eq!(value["output_root_created"], true);
    assert_eq!(value["output_root_available_bytes"], 0);
    assert_eq!(value["error_code"], "output_root_space_failed");
    assert_eq!(value["error_message"], "space unavailable");
    assert!(Path::new(value["output_root"].as_str().unwrap()).is_absolute());
    let lifecycle = job.lifecycle.unwrap();
    assert_eq!(lifecycle.stdout, "ok");
    assert_eq!(lifecycle.stderr, "note");
    assert_eq!(lifecycle.stdout_sha256, hash("ok"));
    assert_eq!(lifecycle.stderr_sha256, hash("note"));
}

#[test]
fn rejects_job_directory_outside_validated_output_root_and_persists_fields() {
    let (directory, state) = setup(|_| {});
    let mut result = result_with_output_root_fields(&state, true, serde_json::json!(42));
    result.provenance.job_directory = directory
        .path()
        .join("different-output-root")
        .join("job-output")
        .to_str()
        .unwrap()
        .to_owned();

    let error = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "output-root-mismatch",
        &FakeClient::result(result),
    )
    .unwrap_err();
    assert_eq!(error.code, "nvms_output_root_mismatch");

    let value =
        serde_json::to_value(load_job_provenance(directory.path(), RUN_ID, "output-root-mismatch").unwrap()).unwrap();
    assert_eq!(value["output_root_created"], true);
    assert_eq!(value["output_root_available_bytes"], 42);
    assert!(Path::new(value["output_root"].as_str().unwrap()).is_absolute());
}

#[test]
fn rejects_output_root_traversal_before_calling_client() {
    let (directory, state) = setup(|manifest| {
        manifest.actions.get_mut("inspect-worker").unwrap().output_root =
            Some("jobs/../escape".to_owned());
    });
    let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));
    let error = run_action_with_client_at(
        directory.path(),
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "parent",
        &client,
    )
    .unwrap_err();
    assert_eq!(error.code, "action_output_root_traversal");
    assert!(client.request.lock().unwrap().is_none());
    let job = load_job_provenance(directory.path(), RUN_ID, "parent").unwrap();
    assert!(!job.success);
    assert_eq!(job.error_code, "action_output_root_traversal");
}

#[test]
fn rejects_missing_output_root_at_validate() {
    let mut manifest = CompositionManifest::parse(include_str!("../../../examples/composition-v0.yaml")).unwrap();
    manifest.actions.get_mut("inspect-worker").unwrap().output_root = None;
    let error = manifest.validate().unwrap_err();
    assert_eq!(error.code, "action_output_root_missing");
}

#[cfg(windows)]
#[test]
fn rejects_ambiguous_windows_output_roots() {
    for (job_id, root) in [("drive-relative", r"C:jobs\harbor"), ("root-relative", r"\jobs\harbor")] {
        let (directory, state) = setup(|manifest| {
            manifest.actions.get_mut("inspect-worker").unwrap().output_root = Some(root.to_owned());
        });
        let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));
        let error = run_action_with_client_at(
            directory.path(),
            directory.path(),
            RUN_ID,
            "inspect-worker",
            job_id,
            &client,
        )
        .unwrap_err();
        assert_eq!(error.code, "action_output_root_ambiguous");
        assert!(client.request.lock().unwrap().is_none());
    }
}

#[test]
fn output_root_changes_plan_digest_deterministically() {
    let manifest = CompositionManifest::parse(include_str!("../../../examples/composition-v0.yaml")).unwrap();
    let first = manifest.plan().unwrap();
    assert_eq!(first, manifest.plan().unwrap());

    let mut changed = manifest;
    changed.actions.get_mut("inspect-worker").unwrap().output_root = Some("jobs/other".to_owned());
    let changed_plan = changed.plan().unwrap();

    assert_ne!(first.manifest_sha256, changed_plan.manifest_sha256);
    assert_eq!(changed_plan, changed.plan().unwrap());
}

#[test]
fn rejects_unsafe_job_ids_before_calling_client() {
    let (directory, state) = setup(|_| {});
    let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));
    for (index, value) in ["../escape", "has/slash", "", ".hidden", "white space"]
        .into_iter()
        .enumerate()
    {
        let error = run_action_with_client(directory.path(), RUN_ID, "inspect-worker", value, &client).unwrap_err();
        assert_eq!(error.code, "job_id_invalid", "case {index}");
    }
    assert!(client.request.lock().unwrap().is_none());
}

#[test]
fn rejects_missing_toolkit_for_gpu_dependency() {
    let (directory, state) = setup(|manifest| manifest.environment.toolkit.version.clear());
    let client = FakeClient::result(success_result(&state, vec![UUID_A.to_owned()]));
    let error =
        run_action_with_client(directory.path(), RUN_ID, "inspect-worker", "missing-toolkit", &client).unwrap_err();
    assert_eq!(error.code, "toolkit_missing");
    assert!(client.request.lock().unwrap().is_none());
}

#[test]
fn persists_malformed_and_empty_response_evidence() {
    for (job_id, stdout, stderr) in [
        ("malformed-response", b"not-json".as_slice(), b"decode error".as_slice()),
        ("empty-response", b"".as_slice(), b"".as_slice()),
    ] {
        let (directory, _) = setup(|_| {});
        let error = run_action_with_client(
            directory.path(),
            RUN_ID,
            "inspect-worker",
            job_id,
            &FakeClient::failure("nvms_malformed_response", stdout, stderr),
        )
        .unwrap_err();
        assert_eq!(error.code, "nvms_malformed_response");
        let job = load_job_provenance(directory.path(), RUN_ID, job_id).unwrap();
        assert!(!job.success);
        assert_eq!(job.error_code, "nvms_malformed_response");
        assert!(!job.output_root.is_empty());
        let lifecycle = job.lifecycle.unwrap();
        assert_eq!(lifecycle.stdout, String::from_utf8_lossy(stdout));
        assert_eq!(lifecycle.stderr, String::from_utf8_lossy(stderr));
        assert_eq!(lifecycle.stdout_sha256, hash(&lifecycle.stdout));
        assert_eq!(lifecycle.stderr_sha256, hash(&lifecycle.stderr));
    }
}

#[test]
fn persists_validation_rejection_with_canonical_empty_hashes() {
    let (directory, _) = setup(|_| {});
    let error = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "validation-rejection",
        &FakeClient::failure("nvms_request_rejected", b"", b""),
    )
    .unwrap_err();
    assert_eq!(error.code, "nvms_request_rejected");
    let job = load_job_provenance(directory.path(), RUN_ID, "validation-rejection").unwrap();
    assert!(!job.output_root.is_empty());
    let lifecycle = job.lifecycle.unwrap();
    assert_eq!(lifecycle.stdout_sha256, hash(""));
    assert_eq!(lifecycle.stderr_sha256, hash(""));
}

#[test]
fn persists_canonical_empty_hashes_for_empty_lifecycle_output() {
    let (directory, state) = setup(|_| {});
    let mut result = success_result(&state, vec![UUID_A.to_owned()]);
    result.lifecycle.stdout.clear();
    result.lifecycle.stderr.clear();
    result.lifecycle.stdout_sha256 = hash("");
    result.lifecycle.stderr_sha256 = hash("");

    let job = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "empty-output",
        &FakeClient::result(result),
    )
    .unwrap();
    let lifecycle = job.lifecycle.unwrap();
    assert_eq!(lifecycle.stdout_sha256, hash(""));
    assert_eq!(lifecycle.stderr_sha256, hash(""));
}

#[test]
fn rejects_digest_route_and_binding_mismatches() {
    for (job_id, mutate, expected_code) in [
        (
            "digest",
            (|result: &mut EvaluationResult| result.provenance.manifest_sha256 = "0".repeat(64))
                as fn(&mut EvaluationResult),
            "nvms_manifest_mismatch",
        ),
        (
            "engine",
            |result: &mut EvaluationResult| result.provenance.effective_engine = "docker".to_owned(),
            "nvms_route_mismatch",
        ),
        (
            "provider",
            |result: &mut EvaluationResult| result.provenance.resolved_provider = "gvisor".to_owned(),
            "nvms_route_mismatch",
        ),
        (
            "binding",
            |result: &mut EvaluationResult| result.provenance.gpu_uuids.clear(),
            "nvms_gpu_binding_mismatch",
        ),
    ] {
        let (directory, state) = setup(|_| {});
        let mut result = success_result(&state, vec![UUID_A.to_owned()]);
        mutate(&mut result);
        let error = run_action_with_client(
            directory.path(),
            RUN_ID,
            "inspect-worker",
            job_id,
            &FakeClient::result(result),
        )
        .unwrap_err();
        assert_eq!(error.code, expected_code);
        let job = load_job_provenance(directory.path(), RUN_ID, job_id).unwrap();
        assert!(!job.success);
        assert_eq!(job.run_id, RUN_ID);
        assert_eq!(job.job_id, job_id);
        assert_eq!(job.manifest_sha256, state.manifest_sha256);
        assert_eq!(job.action, "inspect-worker");
        assert_eq!(job.service, "worker");
        assert!(!job.output_root.is_empty());
    }
}

#[test]
fn preserves_timed_out_lifecycle_and_hashes() {
    let (directory, state) = setup(|_| {});
    let mut result = success_result(&state, vec![UUID_A.to_owned()]);
    result.success = false;
    result.released = true;
    result.error_code = "action_timeout".to_owned();
    result.error_message = "deadline exceeded".to_owned();
    result.lifecycle.exit_code = -1;
    result.lifecycle.timed_out = true;
    let error = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "timeout",
        &FakeClient::result(result),
    )
    .unwrap_err();
    assert_eq!(error.code, "action_timeout");
    let job = load_job_provenance(directory.path(), RUN_ID, "timeout").unwrap();
    let lifecycle = job.lifecycle.unwrap();
    assert!(lifecycle.timed_out);
    assert_eq!(lifecycle.stdout_sha256, hash(&lifecycle.stdout));
    assert_eq!(lifecycle.stderr_sha256, hash(&lifecycle.stderr));
}

#[test]
fn persists_bounded_local_evidence_for_client_timeout() {
    let (directory, _) = setup(|_| {});
    let mut failure = EvaluationFailure::new(
        CliError::backend("nvms_timeout", "outer deadline exceeded"),
        b"partial",
        b"deadline",
    );
    failure.lifecycle.timed_out = true;
    let client = FakeClient {
        result: Mutex::new(Some(Err(failure))),
        request: Mutex::new(None),
    };
    let error =
        run_action_with_client(directory.path(), RUN_ID, "inspect-worker", "client-timeout", &client).unwrap_err();
    assert_eq!(error.code, "nvms_timeout");
    let job = load_job_provenance(directory.path(), RUN_ID, "client-timeout").unwrap();
    let lifecycle = job.lifecycle.unwrap();
    assert!(lifecycle.timed_out);
    assert_eq!(lifecycle.stdout_sha256, hash("partial"));
    assert_eq!(lifecycle.stderr_sha256, hash("deadline"));
}

#[test]
fn rejects_lifecycle_limit_and_hash_mismatch() {
    for (job_id, mutate, expected) in [
        (
            "limit",
            (|result: &mut EvaluationResult| result.lifecycle.duration_ms = 300_001) as fn(&mut EvaluationResult),
            "nvms_lifecycle_limits",
        ),
        (
            "hash",
            |result: &mut EvaluationResult| result.lifecycle.stdout_sha256 = "0".repeat(64),
            "nvms_evidence_hash_mismatch",
        ),
    ] {
        let (directory, state) = setup(|_| {});
        let mut result = success_result(&state, vec![UUID_A.to_owned()]);
        mutate(&mut result);
        let error = run_action_with_client(
            directory.path(),
            RUN_ID,
            "inspect-worker",
            job_id,
            &FakeClient::result(result),
        )
        .unwrap_err();
        assert_eq!(error.code, expected);
    }
}

#[test]
fn rejects_malformed_returned_hash_and_persists_recomputed_evidence() {
    let (directory, state) = setup(|_| {});
    let mut result = success_result(&state, vec![UUID_A.to_owned()]);
    result.lifecycle.stdout_sha256.clear();
    let error = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "invalid-hash",
        &FakeClient::result(result),
    )
    .unwrap_err();
    assert_eq!(error.code, "nvms_evidence_hash_invalid");
    let job = load_job_provenance(directory.path(), RUN_ID, "invalid-hash").unwrap();
    let lifecycle = job.lifecycle.unwrap();
    assert_eq!(lifecycle.stdout_sha256, hash(&lifecycle.stdout));
    assert_eq!(lifecycle.stderr_sha256, hash(&lifecycle.stderr));
}

#[test]
fn successful_provenance_is_atomic_and_exported() {
    let (directory, state) = setup(|_| {});
    let job = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "success",
        &FakeClient::result(success_result(&state, vec![UUID_A.to_owned()])),
    )
    .unwrap();
    assert!(job.success);
    let jobs_dir = directory.path().join(format!("{RUN_ID}.jobs"));
    assert!(jobs_dir.join("success.json").is_file());
    assert!(fs::read_dir(&jobs_dir).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));

    let exported = export_provenance(directory.path(), RUN_ID).unwrap();
    assert_eq!(exported.jobs["success"], job);
}

#[test]
fn failed_atomic_commit_does_not_replace_existing_provenance_or_leave_temporary_files() {
    let (directory, state) = setup(|_| {});
    let jobs_dir = directory.path().join(format!("{RUN_ID}.jobs"));
    fs::create_dir_all(&jobs_dir).unwrap();
    let destination = jobs_dir.join("collision.json");
    fs::write(&destination, b"existing").unwrap();

    let error = run_action_with_client(
        directory.path(),
        RUN_ID,
        "inspect-worker",
        "collision",
        &FakeClient::result(success_result(&state, vec![UUID_A.to_owned()])),
    )
    .unwrap_err();
    assert_eq!(error.code, "job_exists");
    assert_eq!(fs::read(&destination).unwrap(), b"existing");
    assert!(fs::read_dir(&jobs_dir).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
}
