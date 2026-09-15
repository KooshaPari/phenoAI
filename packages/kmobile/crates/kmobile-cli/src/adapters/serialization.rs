//! Concrete CLI adapter implementing [`kmobile_core::ports::SerializationPort`].
//!
//! Pretty-printed **TOML** on the local filesystem. The CLI uses TOML for
//! its project configuration, test reports, and any other domain state
//! that needs to be human-editable.
//!
//! `kmobile-core` ships a JSON adapter ([`kmobile_core::ports::JsonFileSerializer`])
//! as the canonical adapter; this one is the CLI-specific TOML variant.

use std::path::Path;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use toml::Value as TomlValue;

use kmobile_core::error::KMobileError;
use kmobile_core::ports::SerializationPort;

/// TOML-on-disk serializer.
#[derive(Debug, Default, Clone, Copy)]
pub struct CliSerializationAdapter;

impl CliSerializationAdapter {
    /// Build a new instance.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SerializationPort for CliSerializationAdapter {
    async fn save<T: Serialize + Send + Sync>(
        &self,
        value: &T,
        path: &Path,
    ) -> Result<(), KMobileError> {
        // Round-trip through JSON to guarantee TOML compatibility:
        // `toml::to_string` requires every type to be TOML-compatible,
        // which excludes untagged enums and certain other shapes that
        // JSON handles fine. Going through JSON keeps the adapter
        // ergonomic for any serializable domain type.
        let json = serde_json::to_vec_pretty(value)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        let value: serde_json::Value = serde_json::from_slice(&json)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        let toml_value =
            json_to_toml(&value).map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        let body = toml::to_string_pretty(&toml_value)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    KMobileError::FileSystemError(format!(
                        "create_dir_all {}: {e}",
                        parent.display()
                    ))
                })?;
            }
        }
        tokio::fs::write(path, body)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        Ok(())
    }

    async fn load<T: DeserializeOwned>(&self, path: &Path) -> Result<T, KMobileError> {
        let body = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        let toml_value: TomlValue =
            toml::from_str(&body).map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        let json_value = toml_to_json(toml_value);
        let bytes = serde_json::to_vec(&json_value)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| KMobileError::SerializationError(e.to_string()))
    }

    fn format_id(&self) -> &'static str {
        "kmobile-toml-v1"
    }
}

/// Convert a `serde_json::Value` into a `toml::Value`, lossy in the
/// sense that JSON nulls become TOML strings (TOML has no null).
fn json_to_toml(value: &serde_json::Value) -> Result<TomlValue, String> {
    match value {
        serde_json::Value::Null => Ok(TomlValue::String(String::new())),
        serde_json::Value::Bool(b) => Ok(TomlValue::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(TomlValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(TomlValue::Float(f))
            } else {
                Err("unrepresentable JSON number".to_string())
            }
        }
        serde_json::Value::String(s) => Ok(TomlValue::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                out.push(json_to_toml(v)?);
            }
            Ok(TomlValue::Array(out))
        }
        serde_json::Value::Object(obj) => {
            let mut table = toml::value::Table::new();
            for (k, v) in obj {
                table.insert(k.clone(), json_to_toml(v)?);
            }
            Ok(TomlValue::Table(table))
        }
    }
}

/// Convert a `toml::Value` back into a `serde_json::Value`.
fn toml_to_json(value: TomlValue) -> serde_json::Value {
    match value {
        TomlValue::String(s) => serde_json::Value::String(s),
        TomlValue::Integer(i) => serde_json::Value::Number(i.into()),
        TomlValue::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        TomlValue::Boolean(b) => serde_json::Value::Bool(b),
        TomlValue::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        TomlValue::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(toml_to_json).collect())
        }
        TomlValue::Table(t) => {
            let mut obj = serde_json::Map::with_capacity(t.len());
            for (k, v) in t {
                obj.insert(k, toml_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
    }
}
