//! Query filter for audit entries.

use super::{AuditEntry, AuditEventType, AuditSeverity};

/// Query filter for audit entries.
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub start_unix: Option<u64>,
    pub end_unix: Option<u64>,
    pub event_type: Option<AuditEventType>,
    pub severity: Option<AuditSeverity>,
    pub actor: Option<String>,
    /// Match [`AuditEntry::target`] (sandbox / session resource id).
    pub target: Option<String>,
    /// Match [`AuditEntry::correlation_id`] (session / trace correlation).
    pub correlation_id: Option<String>,
    pub action_prefix: Option<String>,
}

impl QueryFilter {
    pub(crate) fn matches(&self, entry: &AuditEntry) -> bool {
        if let Some(start) = self.start_unix {
            if entry.timestamp_unix < start {
                return false;
            }
        }
        if let Some(end) = self.end_unix {
            if entry.timestamp_unix > end {
                return false;
            }
        }
        if let Some(ref t) = self.event_type {
            if &entry.event_type != t {
                return false;
            }
        }
        if let Some(ref s) = self.severity {
            if &entry.severity != s {
                return false;
            }
        }
        if let Some(ref a) = self.actor {
            if &entry.actor != a {
                return false;
            }
        }
        if let Some(ref t) = self.target {
            if entry.target.as_deref() != Some(t.as_str()) {
                return false;
            }
        }
        if let Some(ref c) = self.correlation_id {
            if entry.correlation_id.as_deref() != Some(c.as_str()) {
                return false;
            }
        }
        if let Some(ref p) = self.action_prefix {
            if !entry.action.starts_with(p.as_str()) {
                return false;
            }
        }
        true
    }
}
