//! Serialization port: project / session save and load.
//!
//! Hexagonal port for persisting the domain state of a KMobile session
//! (project metadata, test results, build artifacts, simulator snapshots).
//! Adapters include:
//!
//! - [`JsonFileSerializer`] — pretty-printed JSON on the local filesystem.
//! - Production adapters (CRDT, network-sync, …) live in other crates and
//!   implement the same trait.
//!
//! The domain depends only on the [`SerializationPort`] trait.

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

use crate::error::KMobileError;

/// Hexagonal port: save / load arbitrary serializable domain state.
#[async_trait]
pub trait SerializationPort: Send + Sync {
    /// Serialize `value` and write it to `path`.
    async fn save<T: Serialize + Send + Sync>(
        &self,
        value: &T,
        path: &Path,
    ) -> Result<(), KMobileError>;

    /// Read `path` and deserialize into `T`.
    async fn load<T: DeserializeOwned>(&self, path: &Path) -> Result<T, KMobileError>;

    /// Stable identifier for the format (e.g. `"kmobile-json-v1"`).
    fn format_id(&self) -> &'static str;
}

// ────────────────────────────────────────────────────────────────────────────
// Adapter: JsonFileSerializer
// ────────────────────────────────────────────────────────────────────────────

/// JSON-on-disk adapter.  Used by the CLI to persist project state, by the
/// API server to checkpoint sessions, and by tests as the canonical adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonFileSerializer;

impl JsonFileSerializer {
    /// Build a new instance.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SerializationPort for JsonFileSerializer {
    async fn save<T: Serialize + Send + Sync>(
        &self,
        value: &T,
        path: &Path,
    ) -> Result<(), KMobileError> {
        let bytes = serde_json::to_vec_pretty(value)
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
        tokio::fs::write(path, bytes)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        Ok(())
    }

    async fn load<T: DeserializeOwned>(&self, path: &Path) -> Result<T, KMobileError> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| KMobileError::FileSystemError(e.to_string()))?;
        let value: T = serde_json::from_slice(&bytes)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        Ok(value)
    }

    fn format_id(&self) -> &'static str {
        "kmobile-json-v1"
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Test mock
// ────────────────────────────────────────────────────────────────────────────

/// In-memory mock that records the latest `save` payload and replays a
/// pre-loaded value on `load`. Used by domain tests that need to assert
/// "the domain saved a project with this exact payload".
#[derive(Debug, Default, Clone)]
pub struct MockSerializationPort {
    last_saved: Option<Vec<u8>>,
    staged_load: Option<Vec<u8>>,
    save_count: usize,
    load_count: usize,
}

impl MockSerializationPort {
    /// Stage a payload to be returned by the next `load`.
    pub fn stage_load_bytes(&mut self, bytes: Vec<u8>) {
        self.staged_load = Some(bytes);
    }

    /// Returns the bytes captured by the most recent `save` call.
    pub fn last_saved_bytes(&self) -> Option<&[u8]> {
        self.last_saved.as_deref()
    }

    /// Number of times `save` was called.
    pub fn save_count(&self) -> usize {
        self.save_count
    }

    /// Number of times `load` was called.
    pub fn load_count(&self) -> usize {
        self.load_count
    }
}

#[async_trait]
impl SerializationPort for MockSerializationPort {
    async fn save<T: Serialize + Send + Sync>(
        &self,
        value: &T,
        _path: &Path,
    ) -> Result<(), KMobileError> {
        // &self means we can't mutate `last_saved`. Tests that need to
        // assert on saved bytes use `record_save` from a &mut self context.
        let _ = serde_json::to_vec_pretty(value)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        Ok(())
    }

    async fn load<T: DeserializeOwned>(&self, _path: &Path) -> Result<T, KMobileError> {
        match &self.staged_load {
            Some(bytes) => serde_json::from_slice(bytes)
                .map_err(|e| KMobileError::SerializationError(e.to_string())),
            None => Err(KMobileError::SerializationError(
                "MockSerializationPort: nothing staged for load".into(),
            )),
        }
    }

    fn format_id(&self) -> &'static str {
        "mock-v0"
    }
}

impl MockSerializationPort {
    /// Record a save call (used by tests holding `&mut self`).
    pub fn record_save<T: Serialize>(&mut self, value: &T) -> Result<(), KMobileError> {
        self.save_count += 1;
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| KMobileError::SerializationError(e.to_string()))?;
        self.last_saved = Some(bytes);
        Ok(())
    }

    /// Record a load call (used by tests holding `&mut self`).
    pub fn record_load<T: DeserializeOwned>(&mut self) -> Result<T, KMobileError> {
        self.load_count += 1;
        match self.staged_load.take() {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| KMobileError::SerializationError(e.to_string())),
            None => Err(KMobileError::SerializationError(
                "MockSerializationPort: nothing staged for load".into(),
            )),
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Unit tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        count: u32,
    }

    /// FR-KMOBILE-PORT-SERIAL-000 — JSON adapter round-trips a value
    /// losslessly through a real file.
    #[tokio::test]
    async fn json_adapter_roundtrips() {
        let tmp = TempDir::new().expect("tempdir");
        let path: PathBuf = tmp.path().join("sample.json");
        let ser = JsonFileSerializer::new();

        let original = Sample {
            name: "alpha".into(),
            count: 7,
        };
        ser.save(&original, &path).await.expect("save");
        assert_eq!(ser.format_id(), "kmobile-json-v1");

        let recovered: Sample = ser.load(&path).await.expect("load");
        assert_eq!(recovered, original);
    }

    /// FR-KMOBILE-PORT-SERIAL-001 — JSON adapter returns `FileSystemError`
    /// when the file does not exist.
    #[tokio::test]
    async fn json_adapter_load_missing_file_errors() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("does-not-exist.json");
        let ser = JsonFileSerializer::new();
        let err: Result<Sample, _> = ser.load(&path).await;
        assert!(matches!(err, Err(KMobileError::FileSystemError(_))));
    }

    /// FR-KMOBILE-PORT-SERIAL-002 — mock records call counts and replays a
    /// staged payload.
    #[tokio::test]
    async fn mock_records_and_replays() {
        let mut mock = MockSerializationPort::default();
        let staged = Sample {
            name: "beta".into(),
            count: 42,
        };
        let bytes = serde_json::to_vec_pretty(&staged).unwrap();
        mock.stage_load_bytes(bytes);

        mock.record_save(&staged).expect("record_save");
        let recovered: Sample = mock.record_load().expect("record_load");
        assert_eq!(recovered, staged);

        assert_eq!(mock.save_count(), 1);
        assert_eq!(mock.load_count(), 1);
        assert!(mock.last_saved_bytes().is_some());
    }

    /// FR-KMOBILE-PORT-SERIAL-003 — load on an empty mock returns a
    /// `SerializationError`.
    #[tokio::test]
    async fn mock_load_without_stage_errors() {
        let mock = MockSerializationPort::default();
        let err: Result<Sample, _> = mock.load(Path::new("/no/path")).await;
        assert!(matches!(err, Err(KMobileError::SerializationError(_))));
    }
}
