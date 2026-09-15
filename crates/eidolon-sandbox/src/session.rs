//! Sandbox session persistence (KDesktopVirt Phase D / D1).
//!
//! # Status
//!
//! Hexagonal port of archived `session_storage.rs` patterns — **not** a line
//! copy. Strips the fake connection-pool / background-batch theatre; keeps
//! durable session records, name↔id index, TTL cleanup, and fail-loud I/O.
//!
//! - Always-on: [`SessionRecord`], [`SessionStore`] trait, [`MemorySessionStore`],
//!   fail-loud [`UnavailableSessionStore`].
//! - Feature `sandbox-session`: [`FileSessionStore`] (explicit path required).
//! - Feature `sandbox-session-redis`: [`RedisSessionStore`] (explicit Redis URL;
//!   wraps `redis` 1.4 — connect + `PING` must succeed).
//!
//! Do **not** unarchive KDesktopVirt for routine work.
//! See `docs/EXTRACTION_PLAN.md` Phase D and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md`.

use crate::codes;
use async_trait::async_trait;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "sandbox-session")]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

#[cfg(feature = "sandbox-session-redis")]
pub use crate::session_redis::RedisSessionStore;

/// Unix seconds since epoch (UTC).
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Durable sandbox session record (replaces KDesktopVirt `SessionInfo`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    /// Stable session id (UUID string).
    pub id: String,
    /// Human / caller-assigned name (unique within a store).
    pub name: String,
    /// Optional sandbox / automator id this session is bound to.
    pub sandbox_id: Option<String>,
    /// Optional container / VM id when a live backend is attached.
    pub container_id: Option<String>,
    /// Desktop / runtime label (e.g. `"docker"`, `"playcua"`).
    pub desktop: String,
    /// Lifecycle status string (`"created"`, `"running"`, `"stopped"`, …).
    pub status: String,
    /// Creation time (unix seconds).
    pub created_at_unix: u64,
    /// Optional TTL expiry (unix seconds). `None` = no expiry.
    pub expires_at_unix: Option<u64>,
    /// Resource hints requested for the session.
    pub resources: SessionResources,
    /// Free-form metadata.
    pub metadata: HashMap<String, String>,
}

impl SessionRecord {
    /// Build a new session with a fresh UUID and `created` status.
    pub fn new(name: impl Into<String>, desktop: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            sandbox_id: None,
            container_id: None,
            desktop: desktop.into(),
            status: "created".to_string(),
            created_at_unix: unix_now(),
            expires_at_unix: None,
            resources: SessionResources::default(),
            metadata: HashMap::new(),
        }
    }

    /// Whether this session is expired at `now_unix`.
    pub fn is_expired(&self, now_unix: u64) -> bool {
        self.expires_at_unix
            .map(|exp| now_unix >= exp)
            .unwrap_or(false)
    }

    /// Apply a TTL from `created_at_unix` (seconds).
    pub fn with_ttl_secs(mut self, ttl_secs: u64) -> Self {
        self.expires_at_unix = Some(self.created_at_unix.saturating_add(ttl_secs));
        self
    }
}

/// Resource hints for a session (mirrors KDesktopVirt `SessionResources`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionResources {
    pub memory_mb: u64,
    pub cpu_cores: u32,
    pub disk_gb: u32,
}

impl Default for SessionResources {
    fn default() -> Self {
        Self {
            memory_mb: 512,
            cpu_cores: 1,
            disk_gb: 10,
        }
    }
}

/// On-disk / serialisable session map.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SessionData {
    pub sessions: HashMap<String, SessionRecord>,
    /// name → session id for O(1) existence checks.
    pub session_index: HashMap<String, String>,
    pub last_updated_unix: u64,
}

impl SessionData {
    fn touch(&mut self) {
        self.last_updated_unix = unix_now();
    }

    #[cfg(feature = "sandbox-session")]
    fn rebuild_index(&mut self) {
        self.session_index.clear();
        for (name, session) in &self.sessions {
            self.session_index.insert(name.clone(), session.id.clone());
        }
    }

    fn upsert(&mut self, record: SessionRecord) {
        let name = record.name.clone();
        let id = record.id.clone();
        self.sessions.insert(name.clone(), record);
        self.session_index.insert(name, id);
        self.touch();
    }

    fn remove(&mut self, name: &str) -> Option<SessionRecord> {
        self.session_index.remove(name);
        let removed = self.sessions.remove(name);
        if removed.is_some() {
            self.touch();
        }
        removed
    }
}

/// Hexagonal session persistence port.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Insert or replace a session by name.
    async fn put(&self, record: SessionRecord) -> Result<()>;

    /// Lookup by name.
    async fn get(&self, name: &str) -> Result<Option<SessionRecord>>;

    /// Lookup by session id.
    async fn get_by_id(&self, session_id: &str) -> Result<Option<SessionRecord>>;

    /// Remove by name; returns the removed record if present.
    async fn remove(&self, name: &str) -> Result<Option<SessionRecord>>;

    /// List all sessions (including expired — callers may filter).
    async fn list(&self) -> Result<Vec<SessionRecord>>;

    /// Remove expired sessions; returns count removed.
    async fn cleanup_expired(&self, now_unix: u64) -> Result<usize>;

    /// Persist buffered state (no-op for pure memory).
    async fn flush(&self) -> Result<()>;

    /// Whether this store can accept writes.
    fn backend_ready(&self) -> bool {
        true
    }
}

#[cfg(feature = "sandbox-session")]
fn session_io(op: &str, err: impl std::fmt::Display) -> PhenoError {
    PhenoError::Internal(format!(
        "[{}] session storage {op} failed: {err}",
        codes::SANDBOX_SESSION_IO
    ))
}

fn session_backend(detail: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_SESSION_BACKEND, detail.into())
}

/// In-memory session store (always available; hermetic tests).
#[derive(Debug, Default)]
pub struct MemorySessionStore {
    data: Arc<RwLock<SessionData>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn len(&self) -> usize {
        self.data.read().await.sessions.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn put(&self, record: SessionRecord) -> Result<()> {
        if record.name.is_empty() {
            return Err(PhenoError::BadRequest(
                "session name must be non-empty".into(),
            ));
        }
        if record.id.is_empty() {
            return Err(PhenoError::BadRequest("session id must be non-empty".into()));
        }
        let mut guard = self.data.write().await;
        guard.upsert(record);
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<SessionRecord>> {
        Ok(self.data.read().await.sessions.get(name).cloned())
    }

    async fn get_by_id(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let guard = self.data.read().await;
        Ok(guard
            .sessions
            .values()
            .find(|s| s.id == session_id)
            .cloned())
    }

    async fn remove(&self, name: &str) -> Result<Option<SessionRecord>> {
        Ok(self.data.write().await.remove(name))
    }

    async fn list(&self) -> Result<Vec<SessionRecord>> {
        Ok(self.data.read().await.sessions.values().cloned().collect())
    }

    async fn cleanup_expired(&self, now_unix: u64) -> Result<usize> {
        let mut guard = self.data.write().await;
        let stale: Vec<String> = guard
            .sessions
            .iter()
            .filter(|(_, s)| s.is_expired(now_unix))
            .map(|(n, _)| n.clone())
            .collect();
        let count = stale.len();
        for name in stale {
            guard.remove(&name);
        }
        Ok(count)
    }

    async fn flush(&self) -> Result<()> {
        Ok(())
    }
}

/// Fail-loud store when no backend was configured.
///
/// Every mutating / reading call returns
/// [`codes::SANDBOX_SESSION_BACKEND`] — never silent empty success.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableSessionStore;

#[async_trait]
impl SessionStore for UnavailableSessionStore {
    async fn put(&self, _record: SessionRecord) -> Result<()> {
        Err(session_backend(
            "eidolon-sandbox::UnavailableSessionStore — no session backend configured; \
             use MemorySessionStore, feature `sandbox-session` + FileSessionStore \
             (explicit path), or feature `sandbox-session-redis` + RedisSessionStore \
             (explicit Redis URL; see docs/EXTRACTION_PLAN.md Phase D)",
        ))
    }

    async fn get(&self, _name: &str) -> Result<Option<SessionRecord>> {
        Err(session_backend(
            "eidolon-sandbox::UnavailableSessionStore::get — no session backend configured",
        ))
    }

    async fn get_by_id(&self, _session_id: &str) -> Result<Option<SessionRecord>> {
        Err(session_backend(
            "eidolon-sandbox::UnavailableSessionStore::get_by_id — no session backend configured",
        ))
    }

    async fn remove(&self, _name: &str) -> Result<Option<SessionRecord>> {
        Err(session_backend(
            "eidolon-sandbox::UnavailableSessionStore::remove — no session backend configured",
        ))
    }

    async fn list(&self) -> Result<Vec<SessionRecord>> {
        Err(session_backend(
            "eidolon-sandbox::UnavailableSessionStore::list — no session backend configured",
        ))
    }

    async fn cleanup_expired(&self, _now_unix: u64) -> Result<usize> {
        Err(session_backend(
            "eidolon-sandbox::UnavailableSessionStore::cleanup_expired — no session backend",
        ))
    }

    async fn flush(&self) -> Result<()> {
        Err(session_backend(
            "eidolon-sandbox::UnavailableSessionStore::flush — no session backend configured",
        ))
    }

    fn backend_ready(&self) -> bool {
        false
    }
}

/// JSON file-backed session store (feature `sandbox-session`).
///
/// Requires an **explicit** path — no silent `~/.…` default. Missing parent
/// dirs are created on first flush; read/write failures surface as
/// [`codes::SANDBOX_SESSION_IO`] via [`PhenoError::Internal`].
#[cfg(feature = "sandbox-session")]
#[derive(Debug)]
pub struct FileSessionStore {
    path: PathBuf,
    data: Arc<RwLock<SessionData>>,
}

#[cfg(feature = "sandbox-session")]
impl FileSessionStore {
    /// Open (or create empty) session storage at `path`.
    ///
    /// Fails loud if the path exists but cannot be read / parsed.
    pub async fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let data = if path.exists() {
            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| session_io("read", e))?;
            let mut parsed: SessionData =
                serde_json::from_str(&content).map_err(|e| session_io("parse", e))?;
            if parsed.session_index.is_empty() && !parsed.sessions.is_empty() {
                parsed.rebuild_index();
            }
            parsed
        } else {
            SessionData {
                last_updated_unix: unix_now(),
                ..SessionData::default()
            }
        };
        Ok(Self {
            path,
            data: Arc::new(RwLock::new(data)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    async fn write_disk(&self, data: &SessionData) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| session_io("create_dir", e))?;
        }
        let content = serde_json::to_string_pretty(data).map_err(|e| session_io("serialize", e))?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, &content)
            .await
            .map_err(|e| session_io("write_tmp", e))?;
        tokio::fs::rename(&tmp, &self.path)
            .await
            .map_err(|e| session_io("rename", e))?;
        Ok(())
    }
}

#[cfg(feature = "sandbox-session")]
#[async_trait]
impl SessionStore for FileSessionStore {
    async fn put(&self, record: SessionRecord) -> Result<()> {
        if record.name.is_empty() {
            return Err(PhenoError::BadRequest(
                "session name must be non-empty".into(),
            ));
        }
        {
            let mut guard = self.data.write().await;
            guard.upsert(record);
        }
        self.flush().await
    }

    async fn get(&self, name: &str) -> Result<Option<SessionRecord>> {
        Ok(self.data.read().await.sessions.get(name).cloned())
    }

    async fn get_by_id(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let guard = self.data.read().await;
        Ok(guard
            .sessions
            .values()
            .find(|s| s.id == session_id)
            .cloned())
    }

    async fn remove(&self, name: &str) -> Result<Option<SessionRecord>> {
        let removed = self.data.write().await.remove(name);
        if removed.is_some() {
            self.flush().await?;
        }
        Ok(removed)
    }

    async fn list(&self) -> Result<Vec<SessionRecord>> {
        Ok(self.data.read().await.sessions.values().cloned().collect())
    }

    async fn cleanup_expired(&self, now_unix: u64) -> Result<usize> {
        let count = {
            let mut guard = self.data.write().await;
            let stale: Vec<String> = guard
                .sessions
                .iter()
                .filter(|(_, s)| s.is_expired(now_unix))
                .map(|(n, _)| n.clone())
                .collect();
            let count = stale.len();
            for name in stale {
                guard.remove(&name);
            }
            count
        };
        if count > 0 {
            self.flush().await?;
        }
        Ok(count)
    }

    async fn flush(&self) -> Result<()> {
        let snapshot = self.data.read().await.clone();
        self.write_disk(&snapshot).await
    }
}

/// Convenience facade over a [`SessionStore`] backend.
#[derive(Debug, Clone)]
pub struct SessionStorage<S: SessionStore> {
    store: Arc<S>,
}

impl<S: SessionStore> SessionStorage<S> {
    pub fn new(store: S) -> Self {
        Self {
            store: Arc::new(store),
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub async fn add_session(&self, record: SessionRecord) -> Result<()> {
        self.store.put(record).await
    }

    pub async fn get_session(&self, name: &str) -> Result<Option<SessionRecord>> {
        self.store.get(name).await
    }

    pub async fn get_session_by_id(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        self.store.get_by_id(session_id).await
    }

    pub async fn remove_session(&self, name: &str) -> Result<Option<SessionRecord>> {
        self.store.remove(name).await
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        self.store.list().await
    }

    pub async fn session_count(&self) -> Result<usize> {
        Ok(self.store.list().await?.len())
    }

    pub async fn session_exists(&self, name: &str) -> Result<bool> {
        Ok(self.store.get(name).await?.is_some())
    }

    pub async fn cleanup_stale_sessions(&self) -> Result<usize> {
        self.store.cleanup_expired(unix_now()).await
    }

    pub async fn force_save(&self) -> Result<()> {
        self.store.flush().await
    }
}

impl SessionStorage<MemorySessionStore> {
    pub fn memory() -> Self {
        Self::new(MemorySessionStore::new())
    }
}

impl SessionStorage<UnavailableSessionStore> {
    pub fn unavailable() -> Self {
        Self::new(UnavailableSessionStore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn memory_put_get_list_round_trip() {
        let store = MemorySessionStore::new();
        let mut rec = SessionRecord::new("alpha", "docker");
        rec.sandbox_id = Some("sb-1".into());
        store.put(rec.clone()).await.unwrap();

        let got = store.get("alpha").await.unwrap().expect("present");
        assert_eq!(got.id, rec.id);
        assert_eq!(got.sandbox_id.as_deref(), Some("sb-1"));

        let by_id = store.get_by_id(&rec.id).await.unwrap().expect("by id");
        assert_eq!(by_id.name, "alpha");

        assert_eq!(store.list().await.unwrap().len(), 1);
    }

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn ttl_cleanup_removes_expired() {
        let store = MemorySessionStore::new();
        let expired = SessionRecord::new("old", "docker").with_ttl_secs(1);
        // Force expiry in the past.
        let mut expired = expired;
        expired.expires_at_unix = Some(1);
        store.put(expired).await.unwrap();
        store
            .put(SessionRecord::new("fresh", "docker"))
            .await
            .unwrap();

        let removed = store.cleanup_expired(unix_now()).await.unwrap();
        assert_eq!(removed, 1);
        assert!(store.get("old").await.unwrap().is_none());
        assert!(store.get("fresh").await.unwrap().is_some());
    }

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn unavailable_store_fails_loud() {
        let store = UnavailableSessionStore;
        assert!(!store.backend_ready());
        let err = store
            .put(SessionRecord::new("x", "docker"))
            .await
            .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_SESSION_BACKEND)
        );
        assert_eq!(err.status_code(), 501);
    }

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn empty_name_is_bad_request() {
        let store = MemorySessionStore::new();
        let mut rec = SessionRecord::new("ok", "docker");
        rec.name.clear();
        let err = store.put(rec).await.unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }
}
