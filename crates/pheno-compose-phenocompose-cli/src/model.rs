use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::{CliError, Result};

pub const API_VERSION: &str = "phenocompose.dev/v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompositionManifest {
    pub api_version: String,
    pub kind: ManifestKind,
    pub metadata: Metadata,
    pub environment: Environment,
    pub runtime: RuntimeSpec,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderDeclaration>,
    pub services: BTreeMap<String, Service>,
    #[serde(default)]
    pub actions: BTreeMap<String, Action>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    pub teardown: Teardown,
    pub provenance: ProvenanceRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManifestKind {
    Composition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub name: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    pub platform: Platform,
    pub toolkit: Toolkit,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Linux,
    WindowsWsl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Toolkit {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSpec {
    pub provider: RuntimeProvider,
    pub capability: String,
    #[serde(default)]
    pub wsl_distribution: Option<String>,
    #[serde(default)]
    pub portage_compatibility: Option<PortageCompatibility>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeProvider {
    Podman,
    Nvms,
    Placeholder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PortageCompatibility {
    pub external_engine_token: ExternalEngineToken,
    pub effective_engine: EffectiveEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalEngineToken {
    Docker,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveEngine {
    Podman,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderDeclaration {
    pub capability: String,
    pub status: ProviderStatus,
    #[serde(default)]
    pub implementation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Available,
    Placeholder,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub image: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub resources: Option<ResourceRequirements>,
    #[serde(default)]
    pub health_check: Option<HealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequirements {
    #[serde(default)]
    pub cpu_millis: Option<u32>,
    #[serde(default)]
    pub memory_bytes: Option<u64>,
    #[serde(default)]
    pub gpu: Option<GpuRequirements>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GpuRequirements {
    pub vendor: GpuVendor,
    pub uuids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheck {
    pub command: Vec<String>,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub service: String,
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub name: String,
    pub kind: ArtifactKind,
    pub source: String,
    pub publisher: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    OciImage,
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Teardown {
    #[serde(default)]
    pub order: Vec<String>,
    pub remove_volumes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceRequirements {
    pub required: bool,
    pub format: ProvenanceFormat,
    #[serde(default)]
    pub include: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceFormat {
    PhenocomposeV0,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: String,
    pub manifest_sha256: String,
    pub normalized: CompositionManifest,
}

impl CompositionManifest {
    pub fn parse(input: &str) -> Result<Self> {
        let manifest: Self =
            serde_yaml::from_str(input).map_err(|error| CliError::validation("manifest_parse", error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.api_version != API_VERSION {
            return Err(CliError::validation(
                "schema_version",
                format!("api_version must be {API_VERSION}"),
            ));
        }
        if self.metadata.name.trim().is_empty() {
            return Err(CliError::validation("metadata_name", "metadata.name must not be empty"));
        }
        if self.services.is_empty() {
            return Err(CliError::validation(
                "services_empty",
                "at least one service is required",
            ));
        }
        for (name, service) in &self.services {
            if name.trim().is_empty() || service.image.trim().is_empty() {
                return Err(CliError::validation(
                    "service_invalid",
                    format!("service {name:?} must have a non-empty name and image"),
                ));
            }
            for dependency in &service.depends_on {
                if dependency == name || !self.services.contains_key(dependency) {
                    return Err(CliError::validation(
                        "dependency_invalid",
                        format!("service {name} has invalid dependency {dependency}"),
                    ));
                }
            }
            if let Some(gpu) = service.resources.as_ref().and_then(|r| r.gpu.as_ref()) {
                if gpu.uuids.is_empty() {
                    return Err(CliError::validation(
                        "gpu_selector_empty",
                        format!("service {name} must select at least one GPU UUID"),
                    ));
                }
                for uuid in &gpu.uuids {
                    if canonical_gpu_uuid(uuid).is_none() {
                        return Err(CliError::validation(
                            "gpu_selector_invalid",
                            format!(
                                "service {name} GPU selector {uuid:?} is not a UUID; ordinal indices are forbidden"
                            ),
                        ));
                    }
                }
            }
        }
        self.service_order()?;
        for (name, action) in &self.actions {
            if !self.services.contains_key(&action.service) || action.command.is_empty() {
                return Err(CliError::validation(
                    "action_invalid",
                    format!("action {name} must reference a service and have a command"),
                ));
            }
            if action.output_root.is_none() {
                return Err(CliError::validation(
                    "action_output_root_missing",
                    format!("action {name} must declare output_root"),
                ));
            }
            if action.output_root.as_ref().is_some_and(|root| root.trim().is_empty()) {
                return Err(CliError::validation(
                    "action_output_root_empty",
                    format!("action {name} output_root must not be empty"),
                ));
            }
        }
        for service in &self.teardown.order {
            if !self.services.contains_key(service) {
                return Err(CliError::validation(
                    "teardown_invalid",
                    format!("teardown references unknown service {service}"),
                ));
            }
        }
        Ok(())
    }

    pub fn plan(&self) -> Result<Plan> {
        let mut manifest = self.clone();
        manifest.normalize_gpu_uuids()?;
        let normalized_value = canonicalize(serde_json::to_value(&manifest).map_err(CliError::json)?);
        let normalized_bytes = serde_json::to_vec(&normalized_value).map_err(CliError::json)?;
        let digest = format!("{:x}", Sha256::digest(normalized_bytes));
        let normalized = serde_json::from_value(normalized_value).map_err(CliError::json)?;
        Ok(Plan {
            schema_version: API_VERSION.to_owned(),
            manifest_sha256: digest,
            normalized,
        })
    }

    pub fn service_order(&self) -> Result<Vec<String>> {
        fn visit(
            name: &str,
            services: &BTreeMap<String, Service>,
            visiting: &mut BTreeSet<String>,
            visited: &mut BTreeSet<String>,
            output: &mut Vec<String>,
        ) -> Result<()> {
            if visited.contains(name) {
                return Ok(());
            }
            if !visiting.insert(name.to_owned()) {
                return Err(CliError::validation(
                    "dependency_cycle",
                    format!("service dependency cycle includes {name}"),
                ));
            }
            for dependency in &services[name].depends_on {
                visit(dependency, services, visiting, visited, output)?;
            }
            visiting.remove(name);
            visited.insert(name.to_owned());
            output.push(name.to_owned());
            Ok(())
        }

        let mut output = Vec::new();
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for name in self.services.keys() {
            visit(name, &self.services, &mut visiting, &mut visited, &mut output)?;
        }
        Ok(output)
    }

    fn normalize_gpu_uuids(&mut self) -> Result<()> {
        for (name, service) in &mut self.services {
            let Some(gpu) = service
                .resources
                .as_mut()
                .and_then(|resources| resources.gpu.as_mut())
            else {
                continue;
            };
            for uuid in &mut gpu.uuids {
                let canonical = canonical_gpu_uuid(uuid).ok_or_else(|| {
                    CliError::validation(
                        "gpu_selector_invalid",
                        format!("service {name} GPU selector {uuid:?} is not a UUID; ordinal indices are forbidden"),
                    )
                })?;
                uuid.clone_from(&canonical);
            }
        }
        Ok(())
    }
}

pub fn canonical_gpu_uuid(value: &str) -> Option<String> {
    let body = value
        .strip_prefix("GPU-")
        .or_else(|| value.strip_prefix("gpu-"))
        .unwrap_or(value);
    if body.len() != 36
        || !body.chars().enumerate().all(|(index, character)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                character == '-'
            } else {
                character.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(format!("GPU-{}", body.to_ascii_lowercase()))
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let sorted: BTreeMap<_, _> = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        other => other,
    }
}
