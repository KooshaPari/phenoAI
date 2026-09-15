use phenocompose_cli::model::CompositionManifest;
use phenocompose_cli::{
    apply, ensure_service_lifecycle_capability, plan_lifecycle, ErrorKind, IntentPhase, RollbackContract,
    SERVICE_LIFECYCLE_CAPABILITY,
};
use phenocompose_port_runtime::{Runtime, RuntimeError};
use phenocompose_port_types::{ContainerId, ContainerStatus, ImageRef};
use std::sync::{Arc, Mutex};

const DUAL_GPU: &str = include_str!("../../../examples/dual-gpu-inference-v0.yaml");

fn dual_gpu() -> CompositionManifest {
    CompositionManifest::parse(DUAL_GPU).unwrap()
}

#[test]
fn dual_gpu_manifest_parses_four_service_dependency_graph() {
    let manifest = dual_gpu();
    assert_eq!(manifest.services.len(), 4);
    assert_eq!(
        manifest.services["pheno-serve"].depends_on,
        vec!["sglang-primary", "llama-primary", "llama-helper"]
    );
    assert_eq!(
        manifest.services["sglang-primary"]
            .resources
            .as_ref()
            .unwrap()
            .gpu
            .as_ref()
            .unwrap()
            .uuids[0],
        "GPU-8d337a84-43de-158d-7526-7175288a6064"
    );
}

#[test]
fn service_order_is_topological_and_deterministic() {
    let manifest = dual_gpu();
    let order = manifest.service_order().unwrap();
    assert_eq!(
        order,
        vec!["llama-helper", "llama-primary", "sglang-primary", "pheno-serve",]
    );
    let plan = plan_lifecycle(&manifest, "digest").unwrap();
    assert_eq!(plan.intents.len(), 8);
}

#[test]
fn dependency_cycle_fails_closed() {
    let mut manifest = dual_gpu();
    manifest
        .services
        .get_mut("sglang-primary")
        .unwrap()
        .depends_on
        .push("pheno-serve".to_owned());
    assert_eq!(manifest.service_order().unwrap_err().code, "dependency_cycle");
}

#[test]
fn dry_run_emits_ordered_create_and_start_intents_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let output = apply(dual_gpu(), directory.path(), true).unwrap();
    assert!(output.dry_run);
    assert_eq!(output.lifecycle.intents[0].phase, IntentPhase::Create);
    assert_eq!(output.lifecycle.intents[0].service, "llama-helper");
    assert!(directory.path().read_dir().unwrap().next().is_none());
}

#[test]
fn mutating_apply_fails_closed_for_dual_gpu_stack() {
    let directory = tempfile::tempdir().unwrap();
    let error = apply(dual_gpu(), directory.path(), false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unsupported);
    assert_eq!(error.code, "service_lifecycle_placeholder");
    assert_eq!(error.capability.as_deref(), Some(SERVICE_LIFECYCLE_CAPABILITY));
}

#[test]
fn service_lifecycle_gate_requires_declaration() {
    let mut manifest = dual_gpu();
    manifest
        .providers
        .retain(|_, provider| provider.capability != SERVICE_LIFECYCLE_CAPABILITY);
    assert_eq!(
        ensure_service_lifecycle_capability(&manifest).unwrap_err().code,
        "service_lifecycle_undeclared"
    );
}

struct PartialFailRuntime {
    spawn_count: Arc<Mutex<usize>>,
    fail_on: usize,
    stopped: Arc<Mutex<Vec<String>>>,
}

impl PartialFailRuntime {
    fn new(fail_on: usize) -> Self {
        Self {
            spawn_count: Arc::new(Mutex::new(0)),
            fail_on,
            stopped: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Runtime for PartialFailRuntime {
    fn spawn(&self, image: &ImageRef) -> std::result::Result<ContainerId, RuntimeError> {
        let mut count = self.spawn_count.lock().unwrap();
        *count += 1;
        if *count == self.fail_on {
            return Err(RuntimeError::backend("injected mid-graph failure"));
        }
        Ok(ContainerId::new(format!("id-{}", image.as_ref().replace(':', "-"))))
    }

    fn stop(&self, id: &ContainerId) -> std::result::Result<(), RuntimeError> {
        self.stopped.lock().unwrap().push(id.as_ref().to_owned());
        Ok(())
    }

    fn status(&self, id: &ContainerId) -> std::result::Result<ContainerStatus, RuntimeError> {
        if id.as_ref().starts_with("id-") {
            Ok(ContainerStatus::Running)
        } else {
            Ok(ContainerStatus::NotFound)
        }
    }

    fn name(&self) -> &str {
        "partial-fail"
    }
}

#[test]
fn rollback_contract_only_reverses_created_resources() {
    let runtime = PartialFailRuntime::new(3);
    let mut contract = RollbackContract::default();
    contract.record_create("first", runtime.spawn(&ImageRef::new("a:1")).unwrap());
    contract.record_create("second", runtime.spawn(&ImageRef::new("b:2")).unwrap());
    assert!(runtime.spawn(&ImageRef::new("c:3")).is_err());
    contract.rollback(&runtime);
    assert_eq!(runtime.stopped.lock().unwrap().clone(), vec!["id-b-2", "id-a-1"]);
}
