//! Sandbox audit + compliance (KDesktopVirt Phase D / D2).
//!
//! Structured [`AuditEntry`] append log, optional SHA-256 integrity chain,
//! retention purge by age, honest [`ComplianceReport`] from stored entries.
//! Always-on types + [`AuditEngine`]; feature `sandbox-audit` adds
//! [`FileAuditStore`] + integrity hashing.
//! See `docs/EXTRACTION_PLAN.md` Phase D.

mod audit_index;
mod audit_retention_policy;

pub mod query;
pub mod store;
pub(crate) mod verify;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub use audit_index::{ComplianceReport, ComplianceSummary};
pub use audit_retention_policy::RetentionPolicy;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
pub use query::QueryFilter;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sandbox-audit")]
pub use store::FileAuditStore;
pub use store::{AuditStore, MemoryAuditStore, SharedAuditStore, UnavailableAuditStore};
use verify::{chain_hash, entry_content_hash};

pub use crate::audit_index::{index_sidecar_path, AuditQueryIndexes, INDEX_VERSION};

/// Unix seconds since epoch (UTC).
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// High-level audit event kind (sandbox-relevant subset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    SessionManagement,
    SystemAccess,
    ConfigurationChange,
    SecurityViolation,
    DataAccess,
    ComplianceCheck,
    Other,
}

/// Severity for filtering / reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Outcome of the audited action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeResult {
    Success,
    Failure,
    Denied,
    Partial,
}

/// One durable audit record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp_unix: u64,
    pub event_type: AuditEventType,
    pub severity: AuditSeverity,
    pub source: String,
    pub actor: String,
    pub action: String,
    pub outcome: OutcomeResult,
    pub target: Option<String>,
    pub detail: Option<String>,
    pub correlation_id: Option<String>,
    pub chain_hash: Option<String>,
}

impl AuditEntry {
    pub fn builder(action: impl Into<String>) -> AuditEntryBuilder {
        AuditEntryBuilder::new(action)
    }
}

/// Builder for [`AuditEntry`].
#[derive(Debug, Clone)]
pub struct AuditEntryBuilder {
    action: String,
    event_type: AuditEventType,
    severity: AuditSeverity,
    source: String,
    actor: String,
    outcome: OutcomeResult,
    target: Option<String>,
    detail: Option<String>,
    correlation_id: Option<String>,
}

impl AuditEntryBuilder {
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            event_type: AuditEventType::Other,
            severity: AuditSeverity::Info,
            source: "eidolon-sandbox".into(),
            actor: "system".into(),
            outcome: OutcomeResult::Success,
            target: None,
            detail: None,
            correlation_id: None,
        }
    }
    pub fn event_type(mut self, t: AuditEventType) -> Self {
        self.event_type = t;
        self
    }
    pub fn severity(mut self, s: AuditSeverity) -> Self {
        self.severity = s;
        self
    }
    pub fn source(mut self, s: impl Into<String>) -> Self {
        self.source = s.into();
        self
    }
    pub fn actor(mut self, a: impl Into<String>) -> Self {
        self.actor = a.into();
        self
    }
    pub fn outcome(mut self, o: OutcomeResult) -> Self {
        self.outcome = o;
        self
    }
    pub fn target(mut self, t: impl Into<String>) -> Self {
        self.target = Some(t.into());
        self
    }
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    pub fn correlation_id(mut self, c: impl Into<String>) -> Self {
        self.correlation_id = Some(c.into());
        self
    }

    pub fn build(self) -> Result<AuditEntry> {
        if self.action.is_empty() {
            return Err(PhenoError::BadRequest(
                "audit action must be non-empty".into(),
            ));
        }
        Ok(AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp_unix: unix_now(),
            event_type: self.event_type,
            severity: self.severity,
            source: self.source,
            actor: self.actor,
            action: self.action,
            outcome: self.outcome,
            target: self.target,
            detail: self.detail,
            correlation_id: self.correlation_id,
            chain_hash: None,
        })
    }
}

/// Result of an integrity chain verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntegrityResult {
    pub valid: bool,
    pub entries_verified: usize,
    pub tampered_entry_ids: Vec<String>,
    pub verified_at_unix: u64,
}

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub integrity_verification: bool,
    pub retention: RetentionPolicy,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            integrity_verification: true,
            retention: RetentionPolicy::default(),
        }
    }
}

/// Map a core [`eidolon_core::AutomationEvent`] into an [`AuditEntry`].
pub fn audit_entry_from_automation(
    actor: &str,
    event: &eidolon_core::AutomationEvent,
) -> Result<AuditEntry> {
    let event_type = match &event.event_type {
        eidolon_core::EventType::Custom(s)
            if ["pointer", "text", "screenshot"].contains(&s.as_str()) =>
        {
            AuditEventType::DataAccess
        }
        eidolon_core::EventType::Custom(s) if s == "navigate" => AuditEventType::SystemAccess,
        eidolon_core::EventType::Custom(s) if s == "assertion" => AuditEventType::ComplianceCheck,
        eidolon_core::EventType::TextInput => AuditEventType::DataAccess,
        _ => AuditEventType::Other,
    };
    let detail = match &event.payload {
        eidolon_core::event::EventPayload::Screenshot { path } => Some(path.clone()),
        eidolon_core::event::EventPayload::Assertion {
            condition,
            expected,
        } => Some(format!("{condition} => {expected}")),
        eidolon_core::event::EventPayload::Navigate { url } => Some(url.clone()),
        other => Some(serde_json::to_string(other).unwrap_or_else(|_| format!("{other:?}"))),
    };
    let mut entry = AuditEntry::builder(format!("automation.{}", event.event_type))
        .event_type(event_type)
        .source("eidolon-sandbox")
        .actor(actor)
        .outcome(OutcomeResult::Success)
        .target(actor)
        .correlation_id(&event.id)
        .build()?;
    if let Some(d) = detail {
        entry.detail = Some(d);
    }
    entry.id = event.id.clone();
    entry.timestamp_unix = event.timestamp;
    Ok(entry)
}

/// Audit engine: log, query, retain, verify, report.
#[derive(Debug, Clone)]
pub struct AuditEngine<S: AuditStore> {
    store: Arc<S>,
    config: AuditConfig,
}

impl<S: AuditStore> AuditEngine<S> {
    pub fn new(store: S, config: AuditConfig) -> Self {
        Self {
            store: Arc::new(store),
            config,
        }
    }
    pub fn store(&self) -> &S {
        &self.store
    }
    pub fn config(&self) -> &AuditConfig {
        &self.config
    }

    pub async fn append_entry(&self, mut entry: AuditEntry) -> Result<String> {
        if self.config.integrity_verification {
            let previous = self
                .store
                .all()
                .await?
                .last()
                .and_then(|e| e.chain_hash.clone());
            let content = entry_content_hash(&entry)?;
            entry.chain_hash = Some(chain_hash(previous.as_deref(), &content));
        }
        let id = entry.id.clone();
        self.store.append(entry).await?;
        Ok(id)
    }
    pub async fn log_event(&self, builder: AuditEntryBuilder) -> Result<String> {
        self.append_entry(builder.build()?).await
    }
    pub async fn record_automation_event(
        &self,
        actor: &str,
        event: &eidolon_core::AutomationEvent,
    ) -> Result<String> {
        self.append_entry(audit_entry_from_automation(actor, event)?)
            .await
    }
    pub async fn flush(&self) -> Result<()> {
        self.store.flush().await
    }

    pub async fn verify_integrity(&self) -> Result<IntegrityResult> {
        let entries = self.store.all().await?;
        if entries.is_empty() {
            return Ok(IntegrityResult {
                valid: true,
                entries_verified: 0,
                tampered_entry_ids: Vec::new(),
                verified_at_unix: unix_now(),
            });
        }
        let mut tampered = Vec::new();
        let mut prev_hash: Option<String> = None;
        for entry in &entries {
            let Some(ref stored) = entry.chain_hash else {
                if self.config.integrity_verification {
                    tampered.push(entry.id.clone());
                }
                continue;
            };
            let content = entry_content_hash(entry)?;
            let expected = chain_hash(prev_hash.as_deref(), &content);
            if stored != &expected {
                tampered.push(entry.id.clone());
            }
            prev_hash = Some(stored.clone());
        }
        Ok(IntegrityResult {
            valid: tampered.is_empty(),
            entries_verified: entries.len(),
            tampered_entry_ids: tampered,
            verified_at_unix: unix_now(),
        })
    }
}

impl AuditEngine<MemoryAuditStore> {
    pub fn memory(config: AuditConfig) -> Self {
        Self::new(MemoryAuditStore::new(), config)
    }
}
impl AuditEngine<UnavailableAuditStore> {
    pub fn unavailable(config: AuditConfig) -> Self {
        Self::new(UnavailableAuditStore, config)
    }
}
impl AuditEngine<SharedAuditStore> {
    pub fn from_arc(store: Arc<dyn AuditStore>, config: AuditConfig) -> Self {
        Self::new(SharedAuditStore::new(store), config)
    }
    pub fn shared_memory(config: AuditConfig) -> Self {
        Self::from_arc(Arc::new(MemoryAuditStore::new()), config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;

    #[tokio::test]
    async fn integrity_chain_detects_tamper() {
        let engine = AuditEngine::memory(AuditConfig::default());
        engine
            .log_event(AuditEntry::builder("a").outcome(OutcomeResult::Success))
            .await
            .unwrap();
        engine
            .log_event(AuditEntry::builder("b").outcome(OutcomeResult::Success))
            .await
            .unwrap();
        let ok = engine.verify_integrity().await.unwrap();
        assert!(ok.valid);
        assert_eq!(ok.entries_verified, 2);
        let mut entries = engine.store().all().await.unwrap();
        entries[1].action = "tampered".into();
        engine.store().replace_all(entries).await.unwrap();
        let bad = engine.verify_integrity().await.unwrap();
        assert!(!bad.valid);
        assert_eq!(bad.tampered_entry_ids.len(), 1);
    }

    #[tokio::test]
    async fn unavailable_audit_fails_loud() {
        let engine = AuditEngine::unavailable(AuditConfig::default());
        assert!(!engine.store().backend_ready());
        let err = engine
            .log_event(AuditEntry::builder("x"))
            .await
            .unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::SANDBOX_AUDIT_BACKEND));
        assert_eq!(err.status_code(), 501);
    }

    #[tokio::test]
    async fn empty_action_is_bad_request() {
        let err = AuditEntry::builder("").build().unwrap_err();
        assert!(matches!(err, PhenoError::BadRequest(_)));
    }

    #[test]
    fn builder_creates_valid_entry() {
        let entry = AuditEntry::builder("sandbox.exec")
            .event_type(AuditEventType::SystemAccess)
            .severity(AuditSeverity::Warning)
            .actor("agent-42")
            .outcome(OutcomeResult::Success)
            .target("sandbox-1")
            .detail("ran ls -la")
            .correlation_id("corr-abc")
            .build()
            .expect("build should succeed");
        assert_eq!(entry.action, "sandbox.exec");
        assert_eq!(entry.event_type, AuditEventType::SystemAccess);
        assert_eq!(entry.severity, AuditSeverity::Warning);
        assert_eq!(entry.actor, "agent-42");
        assert_eq!(entry.outcome, OutcomeResult::Success);
        assert_eq!(entry.target.as_deref(), Some("sandbox-1"));
        assert_eq!(entry.detail.as_deref(), Some("ran ls -la"));
        assert_eq!(entry.correlation_id.as_deref(), Some("corr-abc"));
        assert!(!entry.id.is_empty());
        assert!(entry.chain_hash.is_none());
    }

    #[test]
    fn builder_defaults_are_sensible() {
        let entry = AuditEntry::builder("test.action")
            .build()
            .expect("build ok");
        assert_eq!(entry.event_type, AuditEventType::Other);
        assert_eq!(entry.severity, AuditSeverity::Info);
        assert_eq!(entry.source, "eidolon-sandbox");
        assert_eq!(entry.actor, "system");
        assert_eq!(entry.outcome, OutcomeResult::Success);
        assert!(entry.target.is_none());
        assert!(entry.detail.is_none());
        assert!(entry.correlation_id.is_none());
    }

    #[test]
    fn audit_config_default_has_sensible_values() {
        let config = AuditConfig::default();
        assert!(config.integrity_verification);
        assert_eq!(config.retention.max_age_secs, 365 * 24 * 3600);
    }

    #[tokio::test]
    async fn builder_entry_can_be_logged() {
        let engine = AuditEngine::memory(AuditConfig::default());
        let id = engine
            .log_event(AuditEntry::builder("test.log"))
            .await
            .unwrap();
        assert!(!id.is_empty());
        let entries = engine.query(QueryFilter::default()).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "test.log");
    }
}
