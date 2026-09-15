//! Redis session store integration tests.
//!
//! Unit / fail-loud paths live in `session_redis` (no Redis required).
//! Live CRUD requires a reachable Redis and:
//! `REDIS_SESSION_INTEGRATION=1` plus `EIDOLON_REDIS_URL` or `REDIS_URL`.
//!
//! Traces to: FR-EIDOLON-005

#![cfg(feature = "sandbox-session-redis")]

use eidolon_sandbox::codes;
use eidolon_sandbox::{RedisSessionStore, SessionRecord, SessionStore};

fn integration_enabled() -> bool {
    matches!(
        std::env::var("REDIS_SESSION_INTEGRATION").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

fn redis_url() -> Option<String> {
    std::env::var("EIDOLON_REDIS_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("REDIS_URL")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
}

// Traces to: FR-EIDOLON-005
#[tokio::test]
async fn redis_session_round_trip_when_integration() {
    if !integration_enabled() {
        // Hermetic default: skip without claiming success against Redis.
        return;
    }
    let url = redis_url().expect(
        "REDIS_SESSION_INTEGRATION=1 requires EIDOLON_REDIS_URL or REDIS_URL",
    );
    let prefix = format!(
        "eidolon:session:itest:{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let store = RedisSessionStore::connect_with_prefix(&url, &prefix)
        .await
        .unwrap_or_else(|e| panic!("Redis connect failed (fail-loud expected only when down): {e}"));

    let mut rec = SessionRecord::new("redis-itest", "docker");
    rec.container_id = Some("c-redis".into());
    store.put(rec.clone()).await.unwrap();

    let got = store.get("redis-itest").await.unwrap().expect("by name");
    assert_eq!(got.id, rec.id);
    assert_eq!(got.container_id.as_deref(), Some("c-redis"));

    let by_id = store.get_by_id(&rec.id).await.unwrap().expect("by id");
    assert_eq!(by_id.name, "redis-itest");

    assert_eq!(store.list().await.unwrap().len(), 1);

    let mut expired = SessionRecord::new("expired", "docker");
    expired.expires_at_unix = Some(1);
    store.put(expired).await.unwrap();
    let removed = store.cleanup_expired(crate_unix_now()).await.unwrap();
    assert_eq!(removed, 1);
    assert!(store.get("expired").await.unwrap().is_none());

    store.remove("redis-itest").await.unwrap();
    assert!(store.get("redis-itest").await.unwrap().is_none());
}

fn crate_unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Traces to: FR-EIDOLON-005
#[tokio::test]
async fn redis_missing_env_code_documented() {
    // Always-on smoke: empty URL path (no Redis needed).
    let err = RedisSessionStore::connect("").await.unwrap_err();
    assert_eq!(
        err.unsupported_code(),
        Some(codes::SANDBOX_SESSION_REDIS)
    );
}
