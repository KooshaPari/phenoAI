//! Composition wiring: `record_event` → `AuditEngine` / `MemoryAuditStore`.
//!
//! Traces to: FR-EIDOLON-004

use std::sync::Arc;

use eidolon_core::error::PhenoError;
use eidolon_core::event::Platform;
use eidolon_core::traits::SandboxAutomator;
use eidolon_core::AutomationEvent;
use eidolon_sandbox::{
    codes, AuditConfig, AuditingSandbox, MemoryAuditStore, QueryFilter, SandboxClient,
    UnavailableAuditStore,
};
use Platform::*;

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn sandbox_client_memory_audit_round_trip() {
    let store = Arc::new(MemoryAuditStore::new());
    let client = SandboxClient::new("client-audit")
        .expect("valid id")
        .with_audit(store.clone(), AuditConfig::default());

    let event = AutomationEvent::screenshot(Unknown, "/client.png");
    let event_id = event.id.clone();
    client.record_event(event).await.expect("append");

    let engine = client.audit_engine().expect("wired");
    let entries = engine.query(QueryFilter::default()).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, event_id);
    assert_eq!(entries[0].action, "automation.screenshot");
    assert_eq!(entries[0].actor, "client-audit");
    assert_eq!(entries[0].detail.as_deref(), Some("/client.png"));
    assert_eq!(store.len().await, 1);
}

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn sandbox_client_with_memory_audit_helper() {
    let client = SandboxClient::new("mem-helper")
        .expect("valid id")
        .with_memory_audit();
    let event = AutomationEvent::pointer(Unknown, eidolon_core::PointerInput::click(10, 20));
    client.record_event(event).await.expect("ok");
    let entries = client
        .audit_engine()
        .expect("wired")
        .query(QueryFilter::default())
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action, "automation.pointer");
}

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn sandbox_client_unavailable_audit_fails_loud() {
    let client = SandboxClient::new("no-backend")
        .expect("valid id")
        .with_audit(Arc::new(UnavailableAuditStore), AuditConfig::default());

    let err = client
        .record_event(AutomationEvent::screenshot(Unknown, "/x.png"))
        .await
        .unwrap_err();
    assert_eq!(err.status_code(), 501);
    assert_eq!(
        err.unsupported_code().expect("code"),
        codes::SANDBOX_AUDIT_BACKEND
    );
}

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn auditing_sandbox_decorator_round_trip() {
    let store = Arc::new(MemoryAuditStore::new());
    let inner = SandboxClient::new("decorated").expect("valid id");
    let outer = AuditingSandbox::wrap(inner, "decorated", store.clone(), AuditConfig::default())
        .expect("valid id");

    let event = AutomationEvent::text(Unknown, eidolon_core::TextInput::keystroke("hello"));
    let event_id = event.id.clone();
    outer.record_event(event).await.expect("append");

    let entries = outer.engine().query(QueryFilter::default()).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, event_id);
    assert_eq!(entries[0].action, "automation.text");
    assert_eq!(store.len().await, 1);
}

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn default_client_record_event_ok_without_claiming_audit() {
    let client = SandboxClient::new("no-audit").expect("valid id");
    assert!(client.audit_engine().is_none());
    assert!(client
        .record_event(AutomationEvent::screenshot(Unknown, "/stub.png"))
        .await
        .is_ok());
}

#[test]
fn auditing_wrap_rejects_bad_id() {
    let err = AuditingSandbox::wrap_memory(SandboxClient::new("x").unwrap(), "");
    assert!(matches!(err, Err(PhenoError::BadRequest(_))));
}
