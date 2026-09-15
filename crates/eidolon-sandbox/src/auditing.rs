//! Composition-time audit wiring for [`SandboxAutomator`].
//!
//! [`AuditingSandbox`] mirrors [`crate::EnforcingSandbox`]: a hexagonal
//! decorator that routes [`SandboxAutomator::record_event`] through
//! [`crate::AuditEngine`] instead of leaving backends as log-only stubs.
//!
//! # Honesty
//!
//! | Composition | `record_event` behavior |
//! |---|---|
//! | Default [`crate::SandboxClient`] (no audit) | Log + `Ok` — does **not** claim durable audit |
//! | [`crate::SandboxClient::with_memory_audit`] / [`with_audit`] | Append via engine; unavailable store → fail-loud |
//! | [`AuditingSandbox`] over any inner | Inner first, then engine append; fail-loud if store unavailable |
//!
//! Wrap live backends (`DockerSandboxClient`, nanoVM/KVM) with
//! [`AuditingSandbox`] at composition — do not pretend Docker already audits.

use crate::audit::{AuditConfig, AuditEngine, AuditStore, SharedAuditStore};
use eidolon_core::security::validate_sandbox_id;
use eidolon_core::traits::{ResourceUsage, SandboxAutomator, SandboxMetadata};
use eidolon_core::{AutomationEvent, Result};
use std::sync::Arc;

/// Decorator that appends [`AutomationEvent`]s to an [`AuditEngine`].
///
/// Opt-in at composition time. Fail-loud when the injected store is
/// [`crate::UnavailableAuditStore`] (or any store that rejects append).
pub struct AuditingSandbox<I> {
    inner: I,
    sandbox_id: String,
    engine: AuditEngine<SharedAuditStore>,
}

impl<I> std::fmt::Debug for AuditingSandbox<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditingSandbox")
            .field("sandbox_id", &self.sandbox_id)
            .field("engine", &self.engine)
            .finish_non_exhaustive()
    }
}

impl<I> AuditingSandbox<I> {
    /// Wrap `inner` so `record_event` appends to `store` via [`AuditEngine`].
    ///
    /// Returns [`PhenoError::BadRequest`](eidolon_core::PhenoError::BadRequest)
    /// if `sandbox_id` fails validation.
    pub fn wrap(
        inner: I,
        sandbox_id: &str,
        store: Arc<dyn AuditStore>,
        config: AuditConfig,
    ) -> Result<Self> {
        validate_sandbox_id(sandbox_id)?;
        Ok(Self {
            inner,
            sandbox_id: sandbox_id.to_string(),
            engine: AuditEngine::from_arc(store, config),
        })
    }

    /// Convenience: wrap with [`crate::MemoryAuditStore`] (default for tests).
    pub fn wrap_memory(inner: I, sandbox_id: &str) -> Result<Self> {
        Self::wrap(
            inner,
            sandbox_id,
            Arc::new(crate::MemoryAuditStore::new()),
            AuditConfig::default(),
        )
    }

    /// Declared sandbox id used as audit actor / target.
    pub fn sandbox_id(&self) -> &str {
        &self.sandbox_id
    }

    /// Borrow the shared audit engine (query / verify / report).
    pub fn engine(&self) -> &AuditEngine<SharedAuditStore> {
        &self.engine
    }

    /// Borrow the inner automator.
    pub fn inner(&self) -> &I {
        &self.inner
    }

    /// Consume the decorator and return the inner automator.
    pub fn into_inner(self) -> I {
        self.inner
    }
}

#[async_trait::async_trait]
impl<I> SandboxAutomator for AuditingSandbox<I>
where
    I: SandboxAutomator + Send + Sync,
{
    async fn get_metadata(&self) -> Result<SandboxMetadata> {
        self.inner.get_metadata().await
    }

    async fn start(&self) -> Result<()> {
        self.inner.start().await
    }

    async fn stop(&self) -> Result<()> {
        self.inner.stop().await
    }

    async fn exec(&self, cmd: &str) -> Result<String> {
        self.inner.exec(cmd).await
    }

    async fn resource_usage(&self) -> Result<ResourceUsage> {
        self.inner.resource_usage().await
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        // Inner may perform its own sink (log-only stubs return Ok).
        self.inner.record_event(event.clone()).await?;
        self.engine
            .record_automation_event(&self.sandbox_id, &event)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{codes, MemoryAuditStore, UnavailableAuditStore};
    use eidolon_core::error::PhenoError;
    use eidolon_core::event::Platform;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingInner {
        records: AtomicUsize,
    }

    impl CountingInner {
        fn new() -> Self {
            Self {
                records: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl SandboxAutomator for CountingInner {
        async fn get_metadata(&self) -> Result<SandboxMetadata> {
            Ok(SandboxMetadata {
                id: "counting".into(),
                image: "test:inner".into(),
                cpu_limit: 1,
                memory_limit_mb: 128,
                disk_limit_mb: None,
            })
        }

        async fn start(&self) -> Result<()> {
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn exec(&self, _cmd: &str) -> Result<String> {
            Ok(String::new())
        }

        async fn resource_usage(&self) -> Result<ResourceUsage> {
            Ok(ResourceUsage {
                cpu_percent: 0.0,
                memory_mb: 0,
                disk_mb: None,
            })
        }

        async fn record_event(&self, _event: AutomationEvent) -> Result<()> {
            self.records.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    // Traces to: FR-EIDOLON-004
    #[tokio::test]
    async fn wrap_memory_round_trips_event() {
        let store = Arc::new(MemoryAuditStore::new());
        let outer = AuditingSandbox::wrap(
            CountingInner::new(),
            "audit-wire-1",
            store.clone(),
            AuditConfig::default(),
        )
        .expect("valid id");

        let event = AutomationEvent::screenshot(Platform::Unknown, "/wire.png");
        let event_id = event.id.clone();
        outer.record_event(event).await.expect("append");

        assert_eq!(outer.inner().records.load(Ordering::SeqCst), 1);
        let entries = outer
            .engine()
            .query(crate::QueryFilter::default())
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, event_id);
        assert_eq!(entries[0].action, "automation.screenshot");
        assert_eq!(entries[0].actor, "audit-wire-1");
        assert_eq!(store.len().await, 1);
    }

    // Traces to: FR-EIDOLON-004
    #[tokio::test]
    async fn wrap_unavailable_fails_loud() {
        let outer = AuditingSandbox::wrap(
            CountingInner::new(),
            "audit-fail",
            Arc::new(UnavailableAuditStore),
            AuditConfig::default(),
        )
        .unwrap();

        let err = outer
            .record_event(AutomationEvent::screenshot(Platform::Unknown, "/x.png"))
            .await
            .unwrap_err();
        assert_eq!(err.status_code(), 501);
        assert_eq!(
            err.unsupported_code().expect("501 code"),
            codes::SANDBOX_AUDIT_BACKEND
        );
        // Inner ran before engine append failed.
        assert_eq!(outer.inner().records.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn wrap_rejects_empty_sandbox_id() {
        let err = AuditingSandbox::wrap_memory(CountingInner::new(), "");
        assert!(matches!(err, Err(PhenoError::BadRequest(_))));
    }
}
