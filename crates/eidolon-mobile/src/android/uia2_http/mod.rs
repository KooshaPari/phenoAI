//! Appium-compatible **HTTP session client** (UIA2 `:6790` or Appium `:4723`).
//!
//! Talks to a running Appium UIA2 instrumentation server after
//! [`super::Uia2Server`] install/forward/start, or to a full Appium server
//! discovered via feature `mobile-appium` (`android::appium_probe`).
//! Paths match W3C WebDriver + Appium
//! (`POST /session`, `/session/:id/element(s)`, `/actions`, `/screenshot`, …).
//!
//! Extended verbs (windows, contexts, findElements, pointer actions,
//! screenshot, timeouts) live in the `uia2_session_ext` companion module.
//!
//! # wraps
//!
//! `ureq` 2.x — blocking HTTP client for localhost→adb-forwarded UIA2
//! (<https://crates.io/crates/ureq>). No TLS needed for `127.0.0.1`.
//!
//! # Gates
//!
//! Destructive verbs (`create_session`, `delete_session`, `click`, `clear`,
//! `send_keys`, actions, context switch) require
//! [`crate::cli::ACTIONS_ALLOW_ENV`]`=1`. Read probes (`status`,
//! `find_element(s)`, `get_text`, `page_source`, screenshot, windows/contexts
//! GET, timeouts GET) are ungated.
//!
//! # Fail-loud
//!
//! Missing session, non-2xx HTTP, transport errors, and Appium `value.error`
//! bodies map to [`codes::MOBILE_UIA2_UNAVAILABLE`] — never silent success.
//! Explicitly unsupported ops use [`Uia2HttpClient::unsupported`].
//!
//! Feature: `mobile-uia2` (+ optional `mobile-appium` probe). Do not unarchive
//! kmobile. Electron Appium Desktop GUI remains out of scope.

mod commands;

use std::time::Duration;

pub use commands::{
    clear, click, create_session, delete_session, find_element, get_text, page_source, send_keys,
    status, uia2_http_err,
};
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde_json::{json, Value};

pub use super::session_wire::SessionWireMode;
use crate::codes;

/// Default host for adb-forwarded UIA2 (see [`super::DEFAULT_DEVICE_PORT`]).
pub const DEFAULT_HTTP_HOST: &str = "127.0.0.1";

/// Alias for an Appium **server** session client (`:4723` / `EIDOLON_APPIUM_URL`).
///
/// Prefer [`Uia2HttpClient::for_appium`] (or [`AppiumProbe::session_client`]) so
/// the W3C/`using`/`value` translator is enabled. Plain [`Uia2HttpClient::with_base_url`]
/// stays UIA2-server dialect (`strategy`/`selector`) for direct `:6790`.
pub type AppiumSessionClient = Uia2HttpClient;

/// Element id returned by UIA2 `findElement` (JSONWP `ELEMENT` and/or W3C key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uia2Element {
    pub id: String,
}

impl Uia2Element {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// Blocking HTTP client for Appium UiAutomator2 / Appium-compatible session commands.
///
/// wraps: ureq 2.x (json) — https://crates.io/crates/ureq
pub struct Uia2HttpClient {
    base_url: String,
    session_id: Option<String>,
    agent: ureq::Agent,
    timeout: Duration,
    wire: SessionWireMode,
}

impl Uia2HttpClient {
    /// `http://{host}:{port}` with default 30s timeout (UIA2-server dialect).
    pub fn new(host: &str, port: u16) -> Self {
        Self::with_base_url(format!("http://{host}:{port}"))
    }

    /// Construct against an arbitrary base URL (UIA2-server dialect by default).
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let timeout = Duration::from_secs(30);
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            session_id: None,
            agent,
            timeout,
            wire: SessionWireMode::Uia2Server,
        }
    }

    /// Construct against a full Appium server with W3C locator/capability dialect.
    pub fn for_appium(base_url: impl Into<String>) -> Self {
        Self::with_base_url(base_url).with_wire_mode(SessionWireMode::AppiumW3c)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.agent = ureq::AgentBuilder::new().timeout(timeout).build();
        self
    }

    /// Select UIA2-server vs Appium/W3C request body shapes.
    pub fn with_wire_mode(mut self, wire: SessionWireMode) -> Self {
        self.wire = wire;
        self
    }

    /// Attach an existing session id (skips `create_session`).
    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub(crate) fn set_session_id(&mut self, id: Option<String>) {
        self.session_id = id;
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn wire_mode(&self) -> SessionWireMode {
        self.wire
    }

    /// Fail-loud for an explicitly unsupported Appium/W3C verb.
    pub fn unsupported(method: &str) -> PhenoError {
        uia2_http_err(
            method,
            "explicitly unsupported on this Appium-compatible session client \
             (not Electron Appium Desktop; expand Uia2HttpClient or use a \
             live Appium server for vendor-specific extensions)",
        )
    }

    /// Build `findElement` JSON for the client's wire dialect.
    ///
    /// UIA2-server: `strategy` / `selector`. Appium/W3C: `using` / `value`.
    pub fn find_element_body(&self, strategy: &str, selector: &str) -> Value {
        self.wire.find_element_body(strategy, selector)
    }

    /// Build `findElements` JSON for the client's wire dialect.
    pub fn find_elements_body(&self, strategy: &str, selector: &str) -> Value {
        self.wire.find_elements_body(strategy, selector)
    }

    /// W3C Actions payload for a single pointer tap at `(x, y)`.
    pub fn pointer_tap_actions(x: i64, y: i64) -> Value {
        json!({
            "actions": [{
                "type": "pointer",
                "id": "finger1",
                "parameters": { "pointerType": "touch" },
                "actions": [
                    { "type": "pointerMove", "duration": 0, "x": x, "y": y },
                    { "type": "pointerDown", "button": 0 },
                    { "type": "pause", "duration": 50 },
                    { "type": "pointerUp", "button": 0 }
                ]
            }]
        })
    }

    /// Build `POST /session` capabilities for the client's wire dialect.
    pub fn create_session_body(&self) -> Value {
        self.wire.create_session_body()
    }

    /// Extract element id from Appium JSONWP/W3C `value` object.
    pub fn parse_element_id(value: &Value) -> Option<String> {
        if let Some(s) = value.as_str() {
            return Some(s.to_string());
        }
        let obj = value.as_object()?;
        if let Some(s) = obj.get("ELEMENT").and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
        // W3C element key
        for (k, v) in obj {
            if k.starts_with("element-6066-") {
                if let Some(s) = v.as_str() {
                    return Some(s.to_string());
                }
            }
        }
        None
    }

    /// Parse top-level Appium response; fail if `value.error` present.
    pub fn parse_appium_value(body: &Value) -> Result<Value> {
        if let Some(err_obj) = body.get("value").and_then(|v| v.as_object()) {
            if let Some(err) = err_obj.get("error").and_then(|e| e.as_str()) {
                let msg = err_obj
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(err);
                return Err(uia2_http_err(
                    "parse_appium_value",
                    format!("appium error={err}: {msg}"),
                ));
            }
        }
        // Legacy JSONWP status != 0
        if let Some(status) = body.get("status").and_then(|s| s.as_u64()) {
            if status != 0 {
                return Err(uia2_http_err(
                    "parse_appium_value",
                    format!("jsonwp status={status} body={body}"),
                ));
            }
        }
        Ok(body.get("value").cloned().unwrap_or(Value::Null))
    }

    /// Extract `sessionId` from create-session response body.
    pub fn parse_session_id(body: &Value) -> Option<String> {
        if let Some(s) = body.get("sessionId").and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
        body.get("value")
            .and_then(|v| v.get("sessionId"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    /// Fail-loud unless `GET /status` succeeds (server reachable).
    pub fn require_reachable(&self) -> Result<()> {
        let _ = status(self)?;
        Ok(())
    }

    /// `POST /session` — gated. Stores session id on success.
    pub fn create_session(&mut self) -> Result<String> {
        commands::create_session(self)
    }

    /// `DELETE /session/:id` — gated.
    pub fn delete_session(&mut self) -> Result<()> {
        commands::delete_session(self)
    }

    /// `POST /session/:id/element` — ungated find (needs active session).
    pub fn find_element(&self, strategy: &str, selector: &str) -> Result<Uia2Element> {
        commands::find_element(self, strategy, selector)
    }

    /// `POST /session/:id/element/:el/click` — gated.
    pub fn click(&self, element_id: &str) -> Result<()> {
        commands::click(self, element_id)
    }

    /// `POST /session/:id/element/:el/clear` — gated.
    pub fn clear(&self, element_id: &str) -> Result<()> {
        commands::clear(self, element_id)
    }

    /// `POST /session/:id/element/:el/value` — gated sendKeys.
    pub fn send_keys(&self, element_id: &str, text: &str) -> Result<()> {
        commands::send_keys(self, element_id, text)
    }

    /// `GET /session/:id/element/:el/text` — ungated.
    pub fn get_text(&self, element_id: &str) -> Result<String> {
        commands::get_text(self, element_id)
    }

    /// `GET /session/:id/source` — ungated page source / hierarchy.
    pub fn page_source(&self) -> Result<String> {
        commands::page_source(self)
    }

    pub(crate) fn require_session(&self) -> Result<&str> {
        self.session_id.as_deref().ok_or_else(|| {
            uia2_http_err(
                "require_session",
                "no active session — call create_session or with_session_id first",
            )
        })
    }

    pub(crate) fn get_json(&self, url: &str, method: &str) -> Result<Value> {
        match self.agent.get(url).call() {
            Ok(resp) => commands::read_json_response(resp, method),
            Err(ureq::Error::Status(code, resp)) => Err(commands::status_err(method, code, resp)),
            Err(e) => Err(uia2_http_err(method, format!("transport: {e}"))),
        }
    }

    pub(crate) fn post_json(&self, url: &str, payload: &Value, method: &str) -> Result<Value> {
        match self.agent.post(url).send_json(payload.clone()) {
            Ok(resp) => commands::read_json_response(resp, method),
            Err(ureq::Error::Status(code, resp)) => Err(commands::status_err(method, code, resp)),
            Err(e) => Err(uia2_http_err(method, format!("transport: {e}"))),
        }
    }

    pub(crate) fn delete_json(&self, url: &str, method: &str) -> Result<Value> {
        match self.agent.delete(url).call() {
            Ok(resp) => commands::read_json_response(resp, method),
            Err(ureq::Error::Status(code, resp)) => Err(commands::status_err(method, code, resp)),
            Err(e) => Err(uia2_http_err(method, format!("transport: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests;

