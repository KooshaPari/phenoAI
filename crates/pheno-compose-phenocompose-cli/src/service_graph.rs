use crate::model::{CompositionManifest, HealthCheck, Service};
use crate::{CliError, Result};
use phenocompose_port_runtime::Runtime;
use phenocompose_port_types::{ContainerId, ImageRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LIFECYCLE_SCHEMA_VERSION: &str = "phenocompose.lifecycle/v0";
pub const SERVICE_LIFECYCLE_CAPABILITY: &str = "runtime.service_lifecycle";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LifecyclePlan {
    pub schema_version: String,
    pub manifest_sha256: String,
    pub order: Vec<String>,
    pub intents: Vec<ServiceIntent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ServiceIntent {
    pub phase: IntentPhase,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check: Option<HealthCheckIntent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentPhase {
    Create,
    Start,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheckIntent {
    pub command: Vec<String>,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub retries: u32,
}

/// Tracks resources created during a single mutating apply so rollback
/// only stops containers recorded in this run (partial rollback contract).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RollbackContract {
    created: Vec<(String, ContainerId)>,
}

impl RollbackContract {
    pub fn record_create(&mut self, service: impl Into<String>, id: ContainerId) {
        self.created.push((service.into(), id));
    }

    pub fn created_services(&self) -> Vec<&str> {
        self.created.iter().map(|(name, _)| name.as_str()).collect()
    }

    pub fn rollback<R: Runtime + ?Sized>(&self, runtime: &R) {
        for (_, id) in self.created.iter().rev() {
            let _ = runtime.stop(id);
        }
    }
}

pub fn plan_lifecycle(manifest: &CompositionManifest, manifest_sha256: &str) -> Result<LifecyclePlan> {
    let order = manifest.service_order()?;
    let mut intents = Vec::with_capacity(order.len() * 2);
    for name in &order {
        let service = &manifest.services[name];
        intents.push(ServiceIntent {
            phase: IntentPhase::Create,
            service: name.clone(),
            image: Some(service.image.clone()),
            depends_on: service.depends_on.clone(),
            health_check: None,
        });
        intents.push(ServiceIntent {
            phase: IntentPhase::Start,
            service: name.clone(),
            image: None,
            depends_on: service.depends_on.clone(),
            health_check: service.health_check.as_ref().map(health_check_intent),
        });
    }
    Ok(LifecyclePlan {
        schema_version: LIFECYCLE_SCHEMA_VERSION.to_owned(),
        manifest_sha256: manifest_sha256.to_owned(),
        order,
        intents,
    })
}

fn health_check_intent(check: &HealthCheck) -> HealthCheckIntent {
    HealthCheckIntent {
        command: check.command.clone(),
        interval_seconds: check.interval_seconds,
        timeout_seconds: check.timeout_seconds,
        retries: check.retries,
    }
}

pub fn ensure_service_lifecycle_capability(manifest: &CompositionManifest) -> Result<()> {
    let Some(declaration) = manifest
        .providers
        .values()
        .find(|provider| provider.capability == SERVICE_LIFECYCLE_CAPABILITY)
    else {
        return Err(CliError::unsupported(
            "service_lifecycle_undeclared",
            "mutating apply requires a runtime.service_lifecycle provider declaration",
            SERVICE_LIFECYCLE_CAPABILITY,
            "undeclared",
        ));
    };
    match declaration.status {
        crate::model::ProviderStatus::Available => Ok(()),
        crate::model::ProviderStatus::Placeholder => Err(CliError::unsupported(
            "service_lifecycle_placeholder",
            "runtime.service_lifecycle is declared but not yet available; mutating apply was not attempted",
            SERVICE_LIFECYCLE_CAPABILITY,
            declaration.implementation.as_deref().unwrap_or("placeholder"),
        )),
        crate::model::ProviderStatus::Unsupported => Err(CliError::unsupported(
            "service_lifecycle_unsupported",
            "runtime.service_lifecycle is unsupported on this host",
            SERVICE_LIFECYCLE_CAPABILITY,
            declaration.implementation.as_deref().unwrap_or("unsupported"),
        )),
    }
}

pub fn execute_lifecycle_plan<'a, R, F>(
    plan: &'a LifecyclePlan,
    services: &'a BTreeMap<String, Service>,
    mut spawn: F,
) -> std::result::Result<BTreeMap<String, String>, (RollbackContract, CliError)>
where
    R: Runtime + ?Sized + 'a,
    F: FnMut(&'a str, &'a Service) -> &'a R,
{
    let mut rollback = RollbackContract::default();
    let mut containers = BTreeMap::new();
    for intent in &plan.intents {
        if intent.phase != IntentPhase::Create {
            continue;
        }
        let service = &services[&intent.service];
        let runtime = spawn(&intent.service, service);
        let image = ImageRef::new(service.image.as_str());
        match runtime.spawn(&image) {
            Ok(container_id) => {
                rollback.record_create(&intent.service, container_id.clone());
                containers.insert(intent.service.clone(), container_id.id);
            }
            Err(error) => {
                return Err((rollback, crate::map_runtime_error(error)));
            }
        }
    }
    Ok(containers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ManifestKind, Metadata, Platform, ProvenanceFormat, ProvenanceRequirements, RuntimeProvider, RuntimeSpec,
        Service, Teardown, Toolkit,
    };
    use phenocompose_port_runtime::{Runtime, RuntimeError};
    use phenocompose_port_types::{ContainerId, ContainerStatus, ImageRef};
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::{Arc, Mutex};

    struct FakeRuntime {
        name: String,
        spawn_count: Arc<Mutex<usize>>,
        fail_after: Option<usize>,
        stopped: Arc<Mutex<Vec<String>>>,
        ids: Arc<Mutex<BTreeMap<String, String>>>,
    }

    impl FakeRuntime {
        fn new(name: &str, fail_after: Option<usize>) -> Self {
            Self {
                name: name.to_owned(),
                spawn_count: Arc::new(Mutex::new(0)),
                fail_after,
                stopped: Arc::new(Mutex::new(Vec::new())),
                ids: Arc::new(Mutex::new(BTreeMap::new())),
            }
        }

        fn stopped_ids(&self) -> Vec<String> {
            self.stopped.lock().unwrap().clone()
        }
    }

    impl Runtime for FakeRuntime {
        fn spawn(&self, image: &ImageRef) -> std::result::Result<ContainerId, RuntimeError> {
            let mut count = self.spawn_count.lock().unwrap();
            *count += 1;
            if self.fail_after.is_some_and(|limit| *count > limit) {
                return Err(RuntimeError::backend("injected spawn failure"));
            }
            let id = format!("{}-{}", self.name, image.as_ref().replace(':', "-"));
            self.ids.lock().unwrap().insert(self.name.clone(), id.clone());
            Ok(ContainerId::new(id))
        }

        fn stop(&self, id: &ContainerId) -> std::result::Result<(), RuntimeError> {
            self.stopped.lock().unwrap().push(id.as_ref().to_owned());
            Ok(())
        }

        fn status(&self, id: &ContainerId) -> std::result::Result<ContainerStatus, RuntimeError> {
            if self.ids.lock().unwrap().values().any(|value| value == id.as_ref()) {
                Ok(ContainerStatus::Running)
            } else {
                Ok(ContainerStatus::NotFound)
            }
        }

        fn name(&self) -> &str {
            "fake"
        }
    }

    fn minimal_manifest(services: BTreeMap<String, Service>) -> CompositionManifest {
        CompositionManifest {
            api_version: crate::model::API_VERSION.to_owned(),
            kind: ManifestKind::Composition,
            metadata: Metadata {
                name: "test".to_owned(),
                labels: BTreeMap::new(),
            },
            environment: crate::model::Environment {
                platform: Platform::Linux,
                toolkit: Toolkit {
                    name: "cuda".to_owned(),
                    version: "12.8".to_owned(),
                },
                variables: BTreeMap::new(),
            },
            runtime: RuntimeSpec {
                provider: RuntimeProvider::Podman,
                capability: "runtime.podman".to_owned(),
                wsl_distribution: None,
                portage_compatibility: None,
            },
            providers: BTreeMap::new(),
            services,
            actions: BTreeMap::new(),
            artifacts: Vec::new(),
            teardown: Teardown {
                order: Vec::new(),
                remove_volumes: false,
            },
            provenance: ProvenanceRequirements {
                required: false,
                format: ProvenanceFormat::PhenocomposeV0,
                include: BTreeSet::new(),
            },
        }
    }

    #[test]
    fn rollback_only_stops_resources_created_in_current_run() {
        let mut services = BTreeMap::new();
        services.insert(
            "a".to_owned(),
            Service {
                image: "a:image".to_owned(),
                depends_on: Vec::new(),
                command: Vec::new(),
                environment: BTreeMap::new(),
                resources: None,
                health_check: None,
            },
        );
        services.insert(
            "b".to_owned(),
            Service {
                image: "b:image".to_owned(),
                depends_on: vec!["a".to_owned()],
                command: Vec::new(),
                environment: BTreeMap::new(),
                resources: None,
                health_check: None,
            },
        );
        let manifest = minimal_manifest(services);
        let plan = plan_lifecycle(&manifest, "digest").unwrap();
        let runtime_a = FakeRuntime::new("a", None);
        let runtime_b = FakeRuntime::new("b", Some(0));

        let error = execute_lifecycle_plan(&plan, &manifest.services, |name, _| {
            if name == "a" {
                &runtime_a
            } else {
                &runtime_b
            }
        })
        .unwrap_err();
        let (contract, cli_error) = error;
        assert_eq!(cli_error.code, "runtime_backend");
        assert_eq!(contract.created_services(), vec!["a"]);
        contract.rollback(&runtime_a);
        assert_eq!(runtime_a.stopped_ids(), vec!["a-a-image"]);
        assert!(runtime_b.stopped_ids().is_empty());
    }

    #[test]
    fn lifecycle_plan_emits_create_then_start_in_topological_order() {
        let mut services = BTreeMap::new();
        services.insert(
            "root".to_owned(),
            Service {
                image: "root:image".to_owned(),
                depends_on: Vec::new(),
                command: Vec::new(),
                environment: BTreeMap::new(),
                resources: None,
                health_check: None,
            },
        );
        services.insert(
            "leaf".to_owned(),
            Service {
                image: "leaf:image".to_owned(),
                depends_on: vec!["root".to_owned()],
                command: Vec::new(),
                environment: BTreeMap::new(),
                resources: None,
                health_check: Some(crate::model::HealthCheck {
                    command: vec!["/bin/true".to_owned()],
                    interval_seconds: 5,
                    timeout_seconds: 1,
                    retries: 2,
                }),
            },
        );
        let manifest = minimal_manifest(services);
        let plan = plan_lifecycle(&manifest, "abc").unwrap();
        assert_eq!(plan.order, vec!["root", "leaf"]);
        assert_eq!(plan.intents.len(), 4);
        assert_eq!(plan.intents[0].phase, IntentPhase::Create);
        assert_eq!(plan.intents[1].phase, IntentPhase::Start);
        assert_eq!(plan.intents[3].health_check.as_ref().unwrap().retries, 2);
    }
}
