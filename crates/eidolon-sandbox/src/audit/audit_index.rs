//! Audit compliance reporting types and query/generate_report methods on AuditEngine.

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde::{Deserialize, Serialize};

use super::{
    unix_now, AuditEngine, AuditEntry, AuditSeverity, AuditStore, OutcomeResult, QueryFilter,
};

/// Honest compliance summary derived from stored entries (no fabricated scores).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComplianceSummary {
    pub total_events: usize,
    pub success_events: usize,
    pub failure_events: usize,
    pub denied_events: usize,
    pub critical_events: usize,
    /// `success_events / total_events` when total > 0; else `0.0`.
    pub success_ratio: f64,
}

/// Lightweight report over a time window — **not** a SOC2/GDPR attestation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComplianceReport {
    pub report_id: String,
    pub label: String,
    pub period_start_unix: u64,
    pub period_end_unix: u64,
    pub generated_at_unix: u64,
    pub summary: ComplianceSummary,
    pub entry_ids: Vec<String>,
    pub notes: Vec<String>,
}

impl<S: AuditStore> AuditEngine<S> {
    pub async fn query(&self, filter: QueryFilter) -> Result<Vec<AuditEntry>> {
        self.store.query(&filter).await
    }

    /// Build an honest report over `[start, end]` from stored entries.
    pub async fn generate_report(
        &self,
        label: impl Into<String>,
        start_unix: u64,
        end_unix: u64,
    ) -> Result<ComplianceReport> {
        if start_unix > end_unix {
            return Err(PhenoError::BadRequest(
                "compliance report start_unix must be <= end_unix".into(),
            ));
        }
        let entries = self
            .query(QueryFilter {
                start_unix: Some(start_unix),
                end_unix: Some(end_unix),
                ..Default::default()
            })
            .await?;

        let total = entries.len();
        let success = entries
            .iter()
            .filter(|e| e.outcome == OutcomeResult::Success)
            .count();
        let failure = entries
            .iter()
            .filter(|e| e.outcome == OutcomeResult::Failure)
            .count();
        let denied = entries
            .iter()
            .filter(|e| e.outcome == OutcomeResult::Denied)
            .count();
        let critical = entries
            .iter()
            .filter(|e| e.severity == AuditSeverity::Critical)
            .count();
        let success_ratio = if total == 0 {
            0.0
        } else {
            success as f64 / total as f64
        };

        let mut notes = vec![
            "Report is derived from stored audit entries only — not a regulatory attestation."
                .into(),
        ];
        if critical > 0 {
            notes.push(format!("{critical} critical-severity event(s) in window"));
        }

        Ok(ComplianceReport {
            report_id: uuid::Uuid::new_v4().to_string(),
            label: label.into(),
            period_start_unix: start_unix,
            period_end_unix: end_unix,
            generated_at_unix: unix_now(),
            summary: ComplianceSummary {
                total_events: total,
                success_events: success,
                failure_events: failure,
                denied_events: denied,
                critical_events: critical,
                success_ratio,
            },
            entry_ids: entries.into_iter().map(|e| e.id).collect(),
            notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        AuditConfig, AuditEntry, AuditEventType, AuditSeverity, OutcomeResult, RetentionPolicy,
    };
    use super::*;

    // Traces to: FR-EIDOLON-004
    #[tokio::test]
    async fn log_query_and_honest_report() {
        let engine = super::super::AuditEngine::memory(AuditConfig {
            integrity_verification: true,
            retention: RetentionPolicy {
                max_age_secs: 86_400,
            },
        });

        engine
            .log_event(
                AuditEntry::builder("session.create")
                    .event_type(AuditEventType::SessionManagement)
                    .actor("agent-1")
                    .outcome(OutcomeResult::Success),
            )
            .await
            .unwrap();
        engine
            .log_event(
                AuditEntry::builder("sandbox.exec")
                    .event_type(AuditEventType::SystemAccess)
                    .severity(AuditSeverity::Warning)
                    .outcome(OutcomeResult::Failure)
                    .detail("exit 1"),
            )
            .await
            .unwrap();

        let all = engine.query(QueryFilter::default()).await.unwrap();
        assert_eq!(all.len(), 2);

        let report = engine
            .generate_report("sandbox-window", 0, super::super::unix_now() + 10)
            .await
            .unwrap();
        assert_eq!(report.summary.total_events, 2);
        assert_eq!(report.summary.success_events, 1);
        assert_eq!(report.summary.failure_events, 1);
        assert!((report.summary.success_ratio - 0.5).abs() < f64::EPSILON);
        assert!(report
            .notes
            .iter()
            .any(|n| n.contains("not a regulatory attestation")));
    }

    #[test]
    fn query_filter_default_matches_all_entries() {
        let filter = QueryFilter::default();
        let entry = AuditEntry::builder("any.action").build().unwrap();
        assert!(
            filter.matches(&entry),
            "default filter should match any entry"
        );
    }

    #[test]
    fn query_filter_by_event_type() {
        let filter = QueryFilter {
            event_type: Some(AuditEventType::SecurityViolation),
            ..Default::default()
        };

        let matching = AuditEntry::builder("x")
            .event_type(AuditEventType::SecurityViolation)
            .build()
            .unwrap();
        let non_matching = AuditEntry::builder("y")
            .event_type(AuditEventType::DataAccess)
            .build()
            .unwrap();

        assert!(filter.matches(&matching));
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn query_filter_by_actor() {
        let filter = QueryFilter {
            actor: Some("agent-1".into()),
            ..Default::default()
        };

        let matching = AuditEntry::builder("x").actor("agent-1").build().unwrap();
        let non_matching = AuditEntry::builder("y").actor("agent-2").build().unwrap();

        assert!(filter.matches(&matching));
        assert!(!filter.matches(&non_matching));
    }

    #[test]
    fn query_filter_by_time_range() {
        let filter = QueryFilter {
            start_unix: Some(100),
            end_unix: Some(200),
            ..Default::default()
        };

        let inside = AuditEntry::builder("x").build().unwrap();
        let mut inside = inside;
        inside.timestamp_unix = 150;

        let mut before = AuditEntry::builder("y").build().unwrap();
        before.timestamp_unix = 50;

        let mut after = AuditEntry::builder("z").build().unwrap();
        after.timestamp_unix = 250;

        assert!(filter.matches(&inside));
        assert!(!filter.matches(&before));
        assert!(!filter.matches(&after));
    }

    #[tokio::test]
    async fn indexed_query_by_actor_and_event_type() {
        let engine = super::super::AuditEngine::memory(AuditConfig::default());
        engine
            .log_event(
                AuditEntry::builder("session.create")
                    .event_type(AuditEventType::SessionManagement)
                    .actor("agent-1")
                    .target("sandbox-a")
                    .correlation_id("sess-1"),
            )
            .await
            .unwrap();
        engine
            .log_event(
                AuditEntry::builder("sandbox.exec")
                    .event_type(AuditEventType::SystemAccess)
                    .actor("agent-1")
                    .target("sandbox-a"),
            )
            .await
            .unwrap();
        engine
            .log_event(
                AuditEntry::builder("session.create")
                    .event_type(AuditEventType::SessionManagement)
                    .actor("agent-2")
                    .target("sandbox-b"),
            )
            .await
            .unwrap();

        let hits = engine
            .query(QueryFilter {
                actor: Some("agent-1".into()),
                event_type: Some(AuditEventType::SessionManagement),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].action, "session.create");

        let hits = engine
            .query(QueryFilter {
                target: Some("sandbox-a".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);

        assert!(engine
            .store()
            .indexes()
            .await
            .validate(&engine.store().all().await.unwrap())
            .is_ok());
    }

    #[tokio::test]
    async fn corrupt_index_fails_loud_on_query() {
        let engine = super::super::AuditEngine::memory(AuditConfig::default());
        engine
            .log_event(AuditEntry::builder("x").actor("a"))
            .await
            .unwrap();
        {
            engine.store().corrupt_indexes_for_test().await;
        }
        let err = engine
            .query(QueryFilter {
                actor: Some("a".into()),
                ..Default::default()
            })
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains(crate::codes::SANDBOX_AUDIT_INDEX));
    }
}
