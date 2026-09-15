//! Phase D file-backed session + audit stores.
//!
//! Requires `--features sandbox-session,sandbox-audit`.
//!
//! Traces to: FR-EIDOLON-004, FR-EIDOLON-005

#![cfg(all(feature = "sandbox-session", feature = "sandbox-audit"))]

use eidolon_sandbox::codes;
use eidolon_sandbox::{
    AuditConfig, AuditEngine, AuditEntry, AuditEventType, AuditQueryIndexes, AuditStore,
    FileAuditStore, FileSessionStore, OutcomeResult, QueryFilter, RetentionPolicy, SessionRecord,
    SessionStore, INDEX_VERSION,
};
use std::time::{SystemTime, UNIX_EPOCH};

fn tmp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("eidolon-phase-d-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

// Traces to: FR-EIDOLON-005
#[tokio::test]
async fn file_session_store_persists_across_open() {
    let dir = tmp_dir("session");
    let path = dir.join("sessions.json");

    {
        let store = FileSessionStore::open(&path).await.unwrap();
        let mut rec = SessionRecord::new("persist-me", "docker");
        rec.container_id = Some("c-1".into());
        store.put(rec).await.unwrap();
        store.flush().await.unwrap();
    }

    let reopened = FileSessionStore::open(&path).await.unwrap();
    let got = reopened.get("persist-me").await.unwrap().expect("reloaded");
    assert_eq!(got.container_id.as_deref(), Some("c-1"));
    assert_eq!(reopened.list().await.unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// Traces to: FR-EIDOLON-005
#[tokio::test]
async fn file_session_io_code_on_bad_json() {
    let dir = tmp_dir("session-bad");
    let path = dir.join("sessions.json");
    std::fs::write(&path, "{not-json").unwrap();

    let err = FileSessionStore::open(&path).await.unwrap_err();
    match err {
        eidolon_core::PhenoError::Internal(msg) => {
            assert!(
                msg.contains(codes::SANDBOX_SESSION_IO),
                "expected SESSION_IO token, got {msg}"
            );
        }
        other => panic!("expected Internal, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn file_audit_store_round_trip_and_sha_integrity() {
    let dir = tmp_dir("audit");
    let path = dir.join("audit.jsonl");

    let store = FileAuditStore::open(&path).await.unwrap();
    let engine = AuditEngine::new(
        store,
        AuditConfig {
            integrity_verification: true,
            retention: RetentionPolicy::default(),
        },
    );

    engine
        .log_event(
            AuditEntry::builder("session.create")
                .event_type(AuditEventType::SessionManagement)
                .outcome(OutcomeResult::Success),
        )
        .await
        .unwrap();
    engine
        .log_event(
            AuditEntry::builder("sandbox.exec")
                .outcome(OutcomeResult::Failure),
        )
        .await
        .unwrap();

    let verified = engine.verify_integrity().await.unwrap();
    assert!(verified.valid);

    // Re-open and confirm persistence + report honesty.
    let store2 = FileAuditStore::open(&path).await.unwrap();
    let engine2 = AuditEngine::new(store2, AuditConfig::default());
    let entries = engine2.query(QueryFilter::default()).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].chain_hash.is_some());
    // SHA-256 hex is 64 chars when sandbox-audit is on.
    assert_eq!(entries[0].chain_hash.as_ref().unwrap().len(), 64);

    let report = engine2
        .generate_report("file-audit", 0, u64::MAX)
        .await
        .unwrap();
    assert_eq!(report.summary.total_events, 2);
    assert_eq!(report.summary.failure_events, 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn file_audit_index_sidecar_persists_and_reopens() {
    let dir = tmp_dir("audit-idx");
    let path = dir.join("audit.jsonl");

    {
        let store = FileAuditStore::open(&path).await.unwrap();
        let engine = AuditEngine::new(store, AuditConfig::default());
        engine
            .log_event(
                AuditEntry::builder("session.create")
                    .event_type(AuditEventType::SessionManagement)
                    .actor("agent-1")
                    .target("sandbox-9")
                    .correlation_id("corr-9"),
            )
            .await
            .unwrap();
        engine.flush().await.unwrap();
    }

    let idx_path = path.with_file_name("audit.jsonl.idx.json");
    assert!(idx_path.exists(), "index sidecar should be written on flush");

    let store2 = FileAuditStore::open(&path).await.unwrap();
    assert_eq!(store2.index_path(), idx_path.as_path());

    let hits = store2
        .query(&QueryFilter {
            target: Some("sandbox-9".into()),
            correlation_id: Some("corr-9".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].action, "session.create");

    let _ = std::fs::remove_dir_all(&dir);
}

// Traces to: FR-EIDOLON-004
#[tokio::test]
async fn file_audit_corrupt_index_fails_loud_on_open() {
    let dir = tmp_dir("audit-bad-idx");
    let path = dir.join("audit.jsonl");

    {
        let store = FileAuditStore::open(&path).await.unwrap();
        let engine = AuditEngine::new(store, AuditConfig::default());
        engine
            .log_event(AuditEntry::builder("x").actor("a"))
            .await
            .unwrap();
        engine.flush().await.unwrap();
    }

    let idx_path = path.with_file_name("audit.jsonl.idx.json");
    let mut corrupt = AuditQueryIndexes {
        version: INDEX_VERSION,
        ..Default::default()
    };
    corrupt.by_time.insert("00000000000000000000:orphan".into(), ());
    std::fs::write(&idx_path, serde_json::to_string_pretty(&corrupt).unwrap()).unwrap();

    let err = FileAuditStore::open(&path).await.unwrap_err();
    match err {
        eidolon_core::PhenoError::Internal(msg) => {
            assert!(
                msg.contains(codes::SANDBOX_AUDIT_INDEX),
                "expected AUDIT_INDEX token, got {msg}"
            );
        }
        other => panic!("expected Internal, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}
