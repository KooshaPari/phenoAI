//! Redis-backed [`SessionStore`](crate::session::SessionStore) adapter.
//!
//! # wraps: redis 1.4
//!
//! Async client via `redis` crate features `tokio-comp` + `connection-manager`
//! (<https://crates.io/crates/redis>). No fake connection-pool / SIEM theatre —
//! connect + `PING` must succeed or construction fails loud.
//!
//! Feature: `sandbox-session-redis`.
//!
//! Env (for [`RedisSessionStore::connect_from_env`]):
//! - `EIDOLON_REDIS_URL` (preferred), else `REDIS_URL`
//!
//! Live integration: `REDIS_SESSION_INTEGRATION=1` (see integration tests).

use crate::codes;
use crate::session::{SessionRecord, SessionStore};
use async_trait::async_trait;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{AsyncCommands, Client};
use std::time::Duration;
use tokio::sync::Mutex;

const DEFAULT_KEY_PREFIX: &str = "eidolon:session";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

fn redis_cfg(detail: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_SESSION_REDIS, detail.into())
}

fn redis_io(op: &str, err: impl std::fmt::Display) -> PhenoError {
    PhenoError::Internal(format!(
        "[{}] redis session {op} failed: {err}",
        codes::SANDBOX_SESSION_IO
    ))
}

/// Redis / Valkey session store (feature `sandbox-session-redis`).
///
/// Requires an **explicit** Redis URL — no silent localhost default on
/// [`Self::connect_from_env`]. Missing URL, bad URL, connect failure, or failed
/// `PING` → [`codes::SANDBOX_SESSION_REDIS`]. Subsequent command / JSON
/// failures → [`codes::SANDBOX_SESSION_IO`].
pub struct RedisSessionStore {
    conn: Mutex<ConnectionManager>,
    key_prefix: String,
}

impl std::fmt::Debug for RedisSessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisSessionStore")
            .field("key_prefix", &self.key_prefix)
            .finish_non_exhaustive()
    }
}

impl RedisSessionStore {
    /// Connect to Redis at `redis_url` and verify with `PING`.
    ///
    /// Empty / whitespace-only URLs fail loud with
    /// [`codes::SANDBOX_SESSION_REDIS`].
    pub async fn connect(redis_url: impl AsRef<str>) -> Result<Self> {
        Self::connect_with_prefix(redis_url, DEFAULT_KEY_PREFIX).await
    }

    /// Like [`Self::connect`], with a custom key namespace prefix
    /// (default `eidolon:session`).
    pub async fn connect_with_prefix(
        redis_url: impl AsRef<str>,
        key_prefix: impl Into<String>,
    ) -> Result<Self> {
        let url = redis_url.as_ref().trim();
        if url.is_empty() {
            return Err(redis_cfg(
                "eidolon-sandbox::RedisSessionStore — Redis URL is empty; pass an \
                 explicit redis://… URL or set EIDOLON_REDIS_URL / REDIS_URL \
                 (see docs/EXTRACTION_PLAN.md Phase D)",
            ));
        }

        let client = Client::open(url).map_err(|e| {
            redis_cfg(format!(
                "eidolon-sandbox::RedisSessionStore — invalid Redis URL ({e})"
            ))
        })?;

        let cfg = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(CONNECT_TIMEOUT))
            .set_response_timeout(Some(RESPONSE_TIMEOUT))
            .set_number_of_retries(0);

        let mut conn = ConnectionManager::new_with_config(client, cfg)
            .await
            .map_err(|e| {
                redis_cfg(format!(
                    "eidolon-sandbox::RedisSessionStore — Redis connect failed ({e})"
                ))
            })?;

        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| {
                redis_cfg(format!(
                    "eidolon-sandbox::RedisSessionStore — Redis PING failed ({e})"
                ))
            })?;
        if !pong.eq_ignore_ascii_case("PONG") {
            return Err(redis_cfg(format!(
                "eidolon-sandbox::RedisSessionStore — unexpected PING reply: {pong}"
            )));
        }

        Ok(Self {
            conn: Mutex::new(conn),
            key_prefix: key_prefix.into(),
        })
    }

    /// Resolve `EIDOLON_REDIS_URL` or `REDIS_URL` and [`Self::connect`].
    ///
    /// Fails loud when neither env var is set / non-empty.
    pub async fn connect_from_env() -> Result<Self> {
        let url = std::env::var("EIDOLON_REDIS_URL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                std::env::var("REDIS_URL")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
            });
        match url {
            Some(u) => Self::connect(u).await,
            None => Err(redis_cfg(
                "eidolon-sandbox::RedisSessionStore — neither EIDOLON_REDIS_URL nor \
                 REDIS_URL is set; refuse silent empty success \
                 (see docs/EXTRACTION_PLAN.md Phase D)",
            )),
        }
    }

    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    fn name_key(&self, name: &str) -> String {
        format!("{}:name:{}", self.key_prefix, name)
    }

    fn id_key(&self, id: &str) -> String {
        format!("{}:id:{}", self.key_prefix, id)
    }

    fn index_key(&self) -> String {
        format!("{}:index", self.key_prefix)
    }

    async fn load_by_name(
        conn: &mut ConnectionManager,
        name_key: &str,
    ) -> Result<Option<SessionRecord>> {
        let raw: Option<String> = conn.get(name_key).await.map_err(|e| redis_io("GET", e))?;
        match raw {
            None => Ok(None),
            Some(s) => {
                let rec: SessionRecord =
                    serde_json::from_str(&s).map_err(|e| redis_io("parse", e))?;
                Ok(Some(rec))
            }
        }
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn put(&self, record: SessionRecord) -> Result<()> {
        if record.name.is_empty() {
            return Err(PhenoError::BadRequest(
                "session name must be non-empty".into(),
            ));
        }
        if record.id.is_empty() {
            return Err(PhenoError::BadRequest("session id must be non-empty".into()));
        }

        let payload = serde_json::to_string(&record).map_err(|e| redis_io("serialize", e))?;
        let name_key = self.name_key(&record.name);
        let id_key = self.id_key(&record.id);
        let index = self.index_key();

        let mut conn = self.conn.lock().await;

        // If replacing an existing name with a different id, drop the old id key.
        if let Some(prev) = Self::load_by_name(&mut conn, &name_key).await? {
            if prev.id != record.id {
                let _: () = conn
                    .del(self.id_key(&prev.id))
                    .await
                    .map_err(|e| redis_io("DEL old id", e))?;
            }
        }

        let _: () = conn
            .set(&name_key, payload)
            .await
            .map_err(|e| redis_io("SET name", e))?;
        let _: () = conn
            .set(&id_key, &record.name)
            .await
            .map_err(|e| redis_io("SET id", e))?;
        let _: () = conn
            .sadd(&index, &record.name)
            .await
            .map_err(|e| redis_io("SADD index", e))?;
        Ok(())
    }

    async fn get(&self, name: &str) -> Result<Option<SessionRecord>> {
        let mut conn = self.conn.lock().await;
        Self::load_by_name(&mut conn, &self.name_key(name)).await
    }

    async fn get_by_id(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        let mut conn = self.conn.lock().await;
        let name: Option<String> = conn
            .get(self.id_key(session_id))
            .await
            .map_err(|e| redis_io("GET id", e))?;
        match name {
            None => Ok(None),
            Some(n) => Self::load_by_name(&mut conn, &self.name_key(&n)).await,
        }
    }

    async fn remove(&self, name: &str) -> Result<Option<SessionRecord>> {
        let mut conn = self.conn.lock().await;
        let name_key = self.name_key(name);
        let existing = Self::load_by_name(&mut conn, &name_key).await?;
        if let Some(ref rec) = existing {
            let _: () = conn
                .del(&name_key)
                .await
                .map_err(|e| redis_io("DEL name", e))?;
            let _: () = conn
                .del(self.id_key(&rec.id))
                .await
                .map_err(|e| redis_io("DEL id", e))?;
            let _: () = conn
                .srem(self.index_key(), name)
                .await
                .map_err(|e| redis_io("SREM index", e))?;
        }
        Ok(existing)
    }

    async fn list(&self) -> Result<Vec<SessionRecord>> {
        let mut conn = self.conn.lock().await;
        let names: Vec<String> = conn
            .smembers(self.index_key())
            .await
            .map_err(|e| redis_io("SMEMBERS", e))?;
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            if let Some(rec) = Self::load_by_name(&mut conn, &self.name_key(&name)).await? {
                out.push(rec);
            }
        }
        Ok(out)
    }

    async fn cleanup_expired(&self, now_unix: u64) -> Result<usize> {
        let stale: Vec<String> = {
            let sessions = self.list().await?;
            sessions
                .into_iter()
                .filter(|s| s.is_expired(now_unix))
                .map(|s| s.name)
                .collect()
        };
        let count = stale.len();
        for name in stale {
            let _ = self.remove(&name).await?;
        }
        Ok(count)
    }

    async fn flush(&self) -> Result<()> {
        // Writes are durable on each mutating command — nothing to buffer.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn empty_url_fails_loud_with_redis_code() {
        let err = RedisSessionStore::connect("").await.unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_SESSION_REDIS)
        );
        assert_eq!(err.status_code(), 501);
    }

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn whitespace_url_fails_loud() {
        let err = RedisSessionStore::connect("   ").await.unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_SESSION_REDIS)
        );
    }

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn invalid_url_scheme_fails_loud() {
        let err = RedisSessionStore::connect("not-a-redis-url")
            .await
            .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_SESSION_REDIS)
        );
    }

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn unreachable_redis_fails_loud() {
        // Port 1 is almost never a Redis listener; short connect timeout.
        let err = RedisSessionStore::connect("redis://127.0.0.1:1/")
            .await
            .unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_SESSION_REDIS),
            "err = {err:?}"
        );
    }

    // Traces to: FR-EIDOLON-005
    #[tokio::test]
    async fn connect_from_env_fails_when_unset() {
        // Ensure neither var is set for this process slice.
        std::env::remove_var("EIDOLON_REDIS_URL");
        std::env::remove_var("REDIS_URL");
        let err = RedisSessionStore::connect_from_env().await.unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_SESSION_REDIS)
        );
        let msg = err.to_string();
        assert!(
            msg.contains("EIDOLON_REDIS_URL") || msg.contains("REDIS_URL"),
            "msg = {msg}"
        );
    }
}
