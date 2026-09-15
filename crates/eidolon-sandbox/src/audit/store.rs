//! Audit store implementations (hexagonal persistence port).

#[cfg(feature = "sandbox-audit")]
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use eidolon_core::Result;
use tokio::sync::RwLock;

use super::query::QueryFilter;
use super::AuditEntry;
use crate::audit_index::AuditQueryIndexes;
use crate::codes;

#[cfg(feature = "sandbox-audit")]
fn audit_io(op: &str, err: impl std::fmt::Display) -> eidolon_core::error::PhenoError {
    eidolon_core::error::PhenoError::Internal(format!(
        "[{}] audit storage {op} failed: {err}",
        codes::SANDBOX_AUDIT_IO
    ))
}

fn audit_backend(detail: impl Into<String>) -> eidolon_core::error::PhenoError {
    eidolon_core::error::PhenoError::unsupported_platform(
        codes::SANDBOX_AUDIT_BACKEND,
        detail.into(),
    )
}

/// Hexagonal audit persistence port.
#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append(&self, entry: AuditEntry) -> Result<()>;
    async fn query(&self, filter: &QueryFilter) -> Result<Vec<AuditEntry>>;
    async fn all(&self) -> Result<Vec<AuditEntry>>;
    async fn replace_all(&self, entries: Vec<AuditEntry>) -> Result<()>;
    async fn flush(&self) -> Result<()>;
    fn backend_ready(&self) -> bool {
        true
    }
}

/// In-memory entry log + secondary query indexes.
#[derive(Debug, Default, Clone)]
struct AuditLogState {
    entries: Vec<AuditEntry>,
    indexes: AuditQueryIndexes,
}

impl AuditLogState {
    fn query(&self, filter: &QueryFilter) -> Result<Vec<AuditEntry>> {
        self.indexes.validate(&self.entries)?;
        let ids = self.indexes.query_ids(filter);
        Ok(self
            .entries
            .iter()
            .filter(|e| ids.contains(&e.id) && filter.matches(e))
            .cloned()
            .collect())
    }

    fn append(&mut self, entry: AuditEntry) {
        self.indexes.insert(&entry);
        self.entries.push(entry);
    }

    fn replace_all(&mut self, entries: Vec<AuditEntry>) {
        self.entries = entries;
        self.indexes = AuditQueryIndexes::rebuild(&self.entries);
    }
}

/// In-memory audit store.
#[derive(Debug, Default)]
pub struct MemoryAuditStore {
    state: Arc<RwLock<AuditLogState>>,
}

impl MemoryAuditStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn len(&self) -> usize {
        self.state.read().await.entries.len()
    }

    /// Secondary indexes (for tests / introspection).
    pub async fn indexes(&self) -> AuditQueryIndexes {
        self.state.read().await.indexes.clone()
    }

    #[cfg(test)]
    pub(crate) async fn corrupt_indexes_for_test(&self) {
        self.state.write().await.indexes.by_actor.clear();
    }
}

#[async_trait]
impl AuditStore for MemoryAuditStore {
    async fn append(&self, entry: AuditEntry) -> Result<()> {
        self.state.write().await.append(entry);
        Ok(())
    }

    async fn query(&self, filter: &QueryFilter) -> Result<Vec<AuditEntry>> {
        self.state.read().await.query(filter)
    }

    async fn all(&self) -> Result<Vec<AuditEntry>> {
        Ok(self.state.read().await.entries.clone())
    }

    async fn replace_all(&self, entries: Vec<AuditEntry>) -> Result<()> {
        self.state.write().await.replace_all(entries);
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

/// Fail-loud store when no audit backend is configured.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableAuditStore;

#[async_trait]
impl AuditStore for UnavailableAuditStore {
    async fn append(&self, _entry: AuditEntry) -> Result<()> {
        Err(audit_backend(
            "eidolon-sandbox::UnavailableAuditStore — no audit backend configured; \
             use MemoryAuditStore or enable feature `sandbox-audit` + FileAuditStore \
             (see docs/EXTRACTION_PLAN.md Phase D)",
        ))
    }

    async fn query(&self, _filter: &QueryFilter) -> Result<Vec<AuditEntry>> {
        Err(audit_backend(
            "eidolon-sandbox::UnavailableAuditStore::query — no audit backend configured",
        ))
    }

    async fn all(&self) -> Result<Vec<AuditEntry>> {
        Err(audit_backend(
            "eidolon-sandbox::UnavailableAuditStore::all — no audit backend configured",
        ))
    }

    async fn replace_all(&self, _entries: Vec<AuditEntry>) -> Result<()> {
        Err(audit_backend(
            "eidolon-sandbox::UnavailableAuditStore::replace_all — no audit backend",
        ))
    }

    async fn flush(&self) -> Result<()> {
        Err(audit_backend(
            "eidolon-sandbox::UnavailableAuditStore::flush — no audit backend configured",
        ))
    }

    fn backend_ready(&self) -> bool {
        false
    }
}

/// JSONL file-backed audit store (feature `sandbox-audit`).
#[cfg(feature = "sandbox-audit")]
#[derive(Debug)]
pub struct FileAuditStore {
    path: PathBuf,
    index_path: PathBuf,
    state: Arc<RwLock<AuditLogState>>,
}

#[cfg(feature = "sandbox-audit")]
impl FileAuditStore {
    /// Open (or create empty) JSONL audit log at `path`.
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let index_path = super::super::audit_index::index_sidecar_path(&path);
        let entries = if path.exists() {
            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| audit_io("read", e))?;
            let mut parsed = Vec::new();
            for (line_no, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let entry: AuditEntry = serde_json::from_str(line)
                    .map_err(|e| audit_io("parse", format!("line {}: {e}", line_no + 1)))?;
                parsed.push(entry);
            }
            parsed
        } else {
            Vec::new()
        };

        let indexes = if index_path.exists() {
            let content = tokio::fs::read_to_string(&index_path)
                .await
                .map_err(|e| audit_io("read_index", e))?;
            let loaded: AuditQueryIndexes =
                serde_json::from_str(&content).map_err(|e| audit_io("parse_index", e))?;
            loaded.validate(&entries)?;
            loaded
        } else if entries.is_empty() {
            AuditQueryIndexes::default()
        } else {
            AuditQueryIndexes::rebuild(&entries)
        };

        Ok(Self {
            path,
            index_path,
            state: Arc::new(RwLock::new(AuditLogState { entries, indexes })),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    async fn write_disk(&self, state: &AuditLogState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| audit_io("create_dir", e))?;
        }
        let mut body = String::new();
        for entry in &state.entries {
            let line = serde_json::to_string(entry).map_err(|e| audit_io("serialize", e))?;
            body.push_str(&line);
            body.push('\n');
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        tokio::fs::write(&tmp, body)
            .await
            .map_err(|e| audit_io("write_tmp", e))?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .map_err(|e| audit_io("rename", e))?;

        state.indexes.validate(&state.entries)?;
        let index_json = serde_json::to_string_pretty(&state.indexes)
            .map_err(|e| audit_io("serialize_index", e))?;
        let idx_tmp = self.index_path.with_extension("idx.json.tmp");
        tokio::fs::write(&idx_tmp, index_json)
            .await
            .map_err(|e| audit_io("write_index_tmp", e))?;
        tokio::fs::rename(&idx_tmp, &self.index_path)
            .await
            .map_err(|e| audit_io("rename_index", e))?;
        Ok(())
    }
}

#[cfg(feature = "sandbox-audit")]
#[async_trait]
impl AuditStore for FileAuditStore {
    async fn append(&self, entry: AuditEntry) -> Result<()> {
        {
            self.state.write().await.append(entry);
        }
        self.flush().await
    }

    async fn query(&self, filter: &QueryFilter) -> Result<Vec<AuditEntry>> {
        self.state.read().await.query(filter)
    }

    async fn all(&self) -> Result<Vec<AuditEntry>> {
        Ok(self.state.read().await.entries.clone())
    }

    async fn replace_all(&self, entries: Vec<AuditEntry>) -> Result<()> {
        self.state.write().await.replace_all(entries);
        self.flush().await
    }

    async fn flush(&self) -> Result<()> {
        let snapshot = self.state.read().await.clone();
        self.write_disk(&snapshot).await
    }
}

/// `Arc<dyn AuditStore>` wrapper so [`super::AuditEngine`] can be injected at
/// composition time without monomorphizing every automator.
#[derive(Clone)]
pub struct SharedAuditStore(pub(crate) Arc<dyn AuditStore>);

impl SharedAuditStore {
    pub fn new(store: Arc<dyn AuditStore>) -> Self {
        Self(store)
    }

    pub fn inner(&self) -> &Arc<dyn AuditStore> {
        &self.0
    }
}

impl std::fmt::Debug for SharedAuditStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedAuditStore")
            .field("backend_ready", &self.0.backend_ready())
            .finish()
    }
}

#[async_trait]
impl AuditStore for SharedAuditStore {
    async fn append(&self, entry: AuditEntry) -> Result<()> {
        self.0.append(entry).await
    }

    async fn query(&self, filter: &QueryFilter) -> Result<Vec<AuditEntry>> {
        self.0.query(filter).await
    }

    async fn all(&self) -> Result<Vec<AuditEntry>> {
        self.0.all().await
    }

    async fn replace_all(&self, entries: Vec<AuditEntry>) -> Result<()> {
        self.0.replace_all(entries).await
    }

    async fn flush(&self) -> Result<()> {
        self.0.flush().await
    }

    fn backend_ready(&self) -> bool {
        self.0.backend_ready()
    }
}
