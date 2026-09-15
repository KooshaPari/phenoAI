//! Secondary indexes for audit query filters (Phase D+).
//!
//! Maintains in-memory indexes keyed by time range, event type, actor, target
//! (sandbox/session resource id), and correlation id. Persisted as a sidecar
//! JSON file for [`super::FileAuditStore`] (`*.jsonl.idx.json`). Corrupt or
//! mismatched indexes fail loud via [`codes::SANDBOX_AUDIT_INDEX`] — never
//! silently fall back to full scans without rebuilding from an absent sidecar.

use crate::audit::{AuditEntry, AuditEventType, QueryFilter};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const INDEX_VERSION: u32 = 1;

fn audit_index_corrupt(detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::Internal(format!(
        "[{}] audit query index corrupt: {detail}",
        codes::SANDBOX_AUDIT_INDEX
    ))
}

/// Lex-sortable time index key: zero-padded unix seconds + entry id.
fn time_index_key(ts: u64, id: &str) -> String {
    format!("{ts:020}:{id}")
}

fn parse_time_index_key(key: &str) -> Option<(u64, String)> {
    let (ts, id) = key.split_once(':')?;
    let ts = ts.parse().ok()?;
    Some((ts, id.to_string()))
}

fn event_type_key(t: &AuditEventType) -> String {
    match t {
        AuditEventType::SessionManagement => "session_management",
        AuditEventType::SystemAccess => "system_access",
        AuditEventType::ConfigurationChange => "configuration_change",
        AuditEventType::SecurityViolation => "security_violation",
        AuditEventType::DataAccess => "data_access",
        AuditEventType::ComplianceCheck => "compliance_check",
        AuditEventType::Other => "other",
    }
    .into()
}

fn index_bucket(map: &mut HashMap<String, HashSet<String>>, key: &str, id: &str) {
    map.entry(key.to_string()).or_default().insert(id.to_string());
}

/// Secondary indexes for [`QueryFilter`] dimensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditQueryIndexes {
    pub version: u32,
    /// Zero-padded `{timestamp}:{id}` keys sorted for range scans (JSON-safe).
    pub by_time: BTreeMap<String, ()>,
    pub by_event_type: HashMap<String, HashSet<String>>,
    pub by_actor: HashMap<String, HashSet<String>>,
    /// `AuditEntry::target` — sandbox / session resource id.
    pub by_target: HashMap<String, HashSet<String>>,
    /// `AuditEntry::correlation_id` — cross-event session / trace correlation.
    pub by_correlation: HashMap<String, HashSet<String>>,
}

impl AuditQueryIndexes {
    pub fn rebuild(entries: &[AuditEntry]) -> Self {
        let mut indexes = Self {
            version: INDEX_VERSION,
            ..Default::default()
        };
        for entry in entries {
            indexes.insert(entry);
        }
        indexes
    }

    pub fn clear(&mut self) {
        self.version = INDEX_VERSION;
        self.by_time.clear();
        self.by_event_type.clear();
        self.by_actor.clear();
        self.by_target.clear();
        self.by_correlation.clear();
    }

    pub fn insert(&mut self, entry: &AuditEntry) {
        self.version = INDEX_VERSION;
        self.by_time
            .insert(time_index_key(entry.timestamp_unix, &entry.id), ());
        index_bucket(
            &mut self.by_event_type,
            &event_type_key(&entry.event_type),
            &entry.id,
        );
        index_bucket(&mut self.by_actor, &entry.actor, &entry.id);
        if let Some(ref target) = entry.target {
            index_bucket(&mut self.by_target, target, &entry.id);
        }
        if let Some(ref corr) = entry.correlation_id {
            index_bucket(&mut self.by_correlation, corr, &entry.id);
        }
    }

    pub fn all_ids(&self) -> HashSet<String> {
        self.by_time
            .keys()
            .filter_map(|key| parse_time_index_key(key).map(|(_, id)| id))
            .collect()
    }

    pub fn ids_in_time_range(&self, start: Option<u64>, end: Option<u64>) -> HashSet<String> {
        let lo = start.unwrap_or(0);
        let hi = end.unwrap_or(u64::MAX);
        let range_start = format!("{lo:020}:");
        let range_end = format!("{hi:020}:\u{10FFFF}");
        self.by_time
            .range(range_start..=range_end)
            .filter_map(|(key, _)| parse_time_index_key(key).map(|(_, id)| id))
            .collect()
    }

    fn lookup_bucket(map: &HashMap<String, HashSet<String>>, key: &str) -> HashSet<String> {
        map.get(key).cloned().unwrap_or_default()
    }

    fn narrow(current: &mut Option<HashSet<String>>, set: HashSet<String>) {
        *current = Some(match current.take() {
            None => set,
            Some(prev) => prev.intersection(&set).cloned().collect(),
        });
    }

    /// Candidate entry ids matching indexed filter dimensions (intersection).
    pub fn query_ids(&self, filter: &QueryFilter) -> HashSet<String> {
        let mut candidates: Option<HashSet<String>> = None;

        if filter.start_unix.is_some() || filter.end_unix.is_some() {
            Self::narrow(
                &mut candidates,
                self.ids_in_time_range(filter.start_unix, filter.end_unix),
            );
        }
        if let Some(ref t) = filter.event_type {
            Self::narrow(
                &mut candidates,
                Self::lookup_bucket(&self.by_event_type, &event_type_key(t)),
            );
        }
        if let Some(ref actor) = filter.actor {
            Self::narrow(
                &mut candidates,
                Self::lookup_bucket(&self.by_actor, actor),
            );
        }
        if let Some(ref target) = filter.target {
            Self::narrow(
                &mut candidates,
                Self::lookup_bucket(&self.by_target, target),
            );
        }
        if let Some(ref corr) = filter.correlation_id {
            Self::narrow(
                &mut candidates,
                Self::lookup_bucket(&self.by_correlation, corr),
            );
        }

        candidates.unwrap_or_else(|| self.all_ids())
    }

    /// Fail loud when persisted indexes disagree with the entry log.
    pub fn validate(&self, entries: &[AuditEntry]) -> Result<()> {
        if self.version != INDEX_VERSION {
            return Err(audit_index_corrupt(format!(
                "expected version {INDEX_VERSION}, got {}",
                self.version
            )));
        }
        if self.by_time.len() != entries.len() {
            return Err(audit_index_corrupt(format!(
                "by_time has {} keys but log has {} entries",
                self.by_time.len(),
                entries.len()
            )));
        }

        let mut seen_ids = HashSet::new();
        for entry in entries {
            if !seen_ids.insert(entry.id.clone()) {
                return Err(audit_index_corrupt(format!(
                    "duplicate entry id {}",
                    entry.id
                )));
            }
            let key = time_index_key(entry.timestamp_unix, &entry.id);
            if !self.by_time.contains_key(&key) {
                return Err(audit_index_corrupt(format!(
                    "by_time missing ({}, {})",
                    entry.timestamp_unix, entry.id
                )));
            }
            let et = event_type_key(&entry.event_type);
            if !Self::lookup_bucket(&self.by_event_type, &et).contains(&entry.id) {
                return Err(audit_index_corrupt(format!(
                    "by_event_type missing id {} for {et}",
                    entry.id
                )));
            }
            if !Self::lookup_bucket(&self.by_actor, &entry.actor).contains(&entry.id) {
                return Err(audit_index_corrupt(format!(
                    "by_actor missing id {} for {}",
                    entry.id, entry.actor
                )));
            }
            if let Some(ref target) = entry.target {
                if !Self::lookup_bucket(&self.by_target, target).contains(&entry.id) {
                    return Err(audit_index_corrupt(format!(
                        "by_target missing id {} for {target}",
                        entry.id
                    )));
                }
            }
            if let Some(ref corr) = entry.correlation_id {
                if !Self::lookup_bucket(&self.by_correlation, corr).contains(&entry.id) {
                    return Err(audit_index_corrupt(format!(
                        "by_correlation missing id {} for {corr}",
                        entry.id
                    )));
                }
            }
        }

        for key in self.by_time.keys() {
            let Some((_, id)) = parse_time_index_key(key) else {
                return Err(audit_index_corrupt(format!("by_time invalid key {key}")));
            };
            if !seen_ids.contains(&id) {
                return Err(audit_index_corrupt(format!("by_time orphan {key}")));
            }
        }

        Ok(())
    }
}

/// Sidecar path for index JSON adjacent to a JSONL audit log.
pub fn index_sidecar_path(log_path: &Path) -> PathBuf {
    let file_name = log_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audit.jsonl");
    log_path.with_file_name(format!("{file_name}.idx.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditSeverity, OutcomeResult};

    fn sample(id: &str, ts: u64, actor: &str, event_type: AuditEventType) -> AuditEntry {
        AuditEntry {
            id: id.into(),
            timestamp_unix: ts,
            event_type,
            severity: AuditSeverity::Info,
            source: "test".into(),
            actor: actor.into(),
            action: "test.action".into(),
            outcome: OutcomeResult::Success,
            target: Some(format!("sandbox-{actor}")),
            detail: None,
            correlation_id: Some(format!("corr-{id}")),
            chain_hash: None,
        }
    }

    #[test]
    fn rebuild_and_validate_round_trip() {
        let entries = vec![
            sample("a", 100, "agent-1", AuditEventType::SessionManagement),
            sample("b", 200, "agent-2", AuditEventType::SystemAccess),
        ];
        let indexes = AuditQueryIndexes::rebuild(&entries);
        indexes.validate(&entries).expect("valid");
    }

    #[test]
    fn query_ids_intersects_dimensions() {
        let entries = vec![
            sample("a", 100, "agent-1", AuditEventType::SessionManagement),
            sample("b", 150, "agent-1", AuditEventType::SystemAccess),
            sample("c", 200, "agent-2", AuditEventType::SessionManagement),
        ];
        let indexes = AuditQueryIndexes::rebuild(&entries);

        let ids = indexes.query_ids(&QueryFilter {
            actor: Some("agent-1".into()),
            event_type: Some(AuditEventType::SessionManagement),
            ..Default::default()
        });
        assert_eq!(ids, HashSet::from(["a".to_string()]));

        let ids = indexes.query_ids(&QueryFilter {
            start_unix: Some(120),
            end_unix: Some(220),
            ..Default::default()
        });
        assert_eq!(ids, HashSet::from(["b".to_string(), "c".to_string()]));

        let ids = indexes.query_ids(&QueryFilter {
            target: Some("sandbox-agent-2".into()),
            ..Default::default()
        });
        assert_eq!(ids, HashSet::from(["c".to_string()]));

        let ids = indexes.query_ids(&QueryFilter {
            correlation_id: Some("corr-b".into()),
            ..Default::default()
        });
        assert_eq!(ids, HashSet::from(["b".to_string()]));
    }

    #[test]
    fn validate_detects_corrupt_by_time() {
        let entries = vec![sample("a", 100, "agent-1", AuditEventType::Other)];
        let mut indexes = AuditQueryIndexes::rebuild(&entries);
        indexes.by_time.clear();
        let err = indexes.validate(&entries).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains(codes::SANDBOX_AUDIT_INDEX));
    }

    #[test]
    fn validate_detects_mismatched_by_time_key() {
        let entries = vec![sample("a", 100, "agent-1", AuditEventType::Other)];
        let mut indexes = AuditQueryIndexes::rebuild(&entries);
        indexes.by_time.clear();
        indexes
            .by_time
            .insert(time_index_key(100, "wrong-id"), ());
        let err = indexes.validate(&entries).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains(codes::SANDBOX_AUDIT_INDEX));
        assert!(msg.contains("by_time missing"));
    }
}
