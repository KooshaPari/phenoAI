//! Audit retention policy (age-based purge).

use eidolon_core::Result;
use serde::{Deserialize, Serialize};

use super::{AuditEngine, AuditEntry, AuditStore};

/// Retention policy (age-based purge only — no fake legal-hold engine).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// Drop entries older than this many seconds.
    pub max_age_secs: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            // ~1 year
            max_age_secs: 365 * 24 * 3600,
        }
    }
}

impl<S: AuditStore> AuditEngine<S> {
    /// Drop entries older than retention; returns count removed.
    pub async fn apply_retention(&self, now_unix: u64) -> Result<usize> {
        let max_age = self.config.retention.max_age_secs;
        let cutoff = now_unix.saturating_sub(max_age);
        let kept: Vec<AuditEntry> = self
            .store
            .all()
            .await?
            .into_iter()
            .filter(|e| e.timestamp_unix >= cutoff)
            .collect();
        let removed = {
            let before = self.store.all().await?.len();
            before.saturating_sub(kept.len())
        };
        if removed > 0 {
            self.store.replace_all(kept).await?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AuditConfig, AuditEntry};
    use super::*;

    // Traces to: FR-EIDOLON-004
    #[tokio::test]
    async fn retention_purges_old_entries() {
        let engine = super::super::AuditEngine::memory(AuditConfig {
            integrity_verification: false,
            retention: RetentionPolicy { max_age_secs: 100 },
        });
        let mut old = AuditEntry::builder("old").build().unwrap();
        old.timestamp_unix = 10;
        engine.store().append(old).await.unwrap();
        engine.log_event(AuditEntry::builder("new")).await.unwrap();

        let removed = engine.apply_retention(1000).await.unwrap();
        assert_eq!(removed, 1);
        assert_eq!(engine.store().len().await, 1);
    }
}
