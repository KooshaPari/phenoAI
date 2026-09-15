//! Command implementations for the UIA2 HTTP client.
//!
//! Contains the HTTP transport helpers and all WebDriver/Appium command methods.

use std::time::Duration;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde_json::{json, Value};

use super::{Uia2Element, Uia2HttpClient};
use crate::cli::require_actions_allowed;
use crate::codes;

pub(crate) fn uia2_http_err(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::MOBILE_UIA2_UNAVAILABLE,
        format!(
            "Uia2HttpClient::{method} unavailable — {detail}; ensure UIA2 \
             (:6790) or Appium (:4723 / EIDOLON_APPIUM_URL) is reachable \
             (features `mobile-uia2` / `mobile-appium`), set \
             EIDOLON_MOBILE_ALLOW_ACTIONS=1 for destructive session/click \
             actions; see docs/EXTRACTION_PLAN.md (do not unarchive kmobile; \
             Appium Desktop GUI out of scope)"
        ),
    )
}

/// `GET {base}/status` — ungated readiness probe (fail-loud on HTTP error).
pub fn status(client: &Uia2HttpClient) -> Result<Value> {
    let url = format!("{}/status", client.base_url());
    let body = client.get_json(&url, "status")?;
    Ok(body)
}

/// `POST /session` — gated. Stores session id on success.
pub fn create_session(client: &mut Uia2HttpClient) -> Result<String> {
    require_actions_allowed("uia2_http_create_session")?;
    let url = format!("{}/session", client.base_url());
    let body = client.post_json(&url, &client.create_session_body(), "create_session")?;
    let value = Uia2HttpClient::parse_appium_value(&body)?;
    let id = Uia2HttpClient::parse_session_id(&body)
        .or_else(|| Uia2HttpClient::parse_session_id(&json!({ "value": value })))
        .ok_or_else(|| {
            uia2_http_err(
                "create_session",
                format!("no sessionId in response: {body}"),
            )
        })?;
    client.set_session_id(Some(id.clone()));
    Ok(id)
}

/// `DELETE /session/:id` — gated.
pub fn delete_session(client: &mut Uia2HttpClient) -> Result<()> {
    require_actions_allowed("uia2_http_delete_session")?;
    let id = client.require_session()?.to_string();
    let url = format!("{}/session/{id}", client.base_url());
    let _ = client.delete_json(&url, "delete_session")?;
    client.set_session_id(None);
    Ok(())
}

/// `POST /session/:id/element` — ungated find (needs active session).
pub fn find_element(
    client: &Uia2HttpClient,
    strategy: &str,
    selector: &str,
) -> Result<Uia2Element> {
    let id = client.require_session()?;
    let url = format!("{}/session/{id}/element", client.base_url());
    let body = client.post_json(
        &url,
        &client.find_element_body(strategy, selector),
        "find_element",
    )?;
    let value = Uia2HttpClient::parse_appium_value(&body)?;
    let el = Uia2HttpClient::parse_element_id(&value)
        .ok_or_else(|| uia2_http_err("find_element", format!("no ELEMENT id in value={value}")))?;
    Ok(Uia2Element::new(el))
}

/// `POST /session/:id/element/:el/click` — gated.
pub fn click(client: &Uia2HttpClient, element_id: &str) -> Result<()> {
    require_actions_allowed("uia2_http_click")?;
    let sid = client.require_session()?;
    let url = format!(
        "{}/session/{sid}/element/{element_id}/click",
        client.base_url()
    );
    let body = client.post_json(&url, &json!({}), "click")?;
    let _ = Uia2HttpClient::parse_appium_value(&body)?;
    Ok(())
}

/// `POST /session/:id/element/:el/clear` — gated.
pub fn clear(client: &Uia2HttpClient, element_id: &str) -> Result<()> {
    require_actions_allowed("uia2_http_clear")?;
    let sid = client.require_session()?;
    let url = format!(
        "{}/session/{sid}/element/{element_id}/clear",
        client.base_url()
    );
    let body = client.post_json(&url, &json!({}), "clear")?;
    let _ = Uia2HttpClient::parse_appium_value(&body)?;
    Ok(())
}

/// `POST /session/:id/element/:el/value` — gated sendKeys.
pub fn send_keys(client: &Uia2HttpClient, element_id: &str, text: &str) -> Result<()> {
    require_actions_allowed("uia2_http_send_keys")?;
    let sid = client.require_session()?;
    let url = format!(
        "{}/session/{sid}/element/{element_id}/value",
        client.base_url()
    );
    let payload = json!({
        "text": text,
        "value": text.chars().map(|c| c.to_string()).collect::<Vec<_>>(),
    });
    let body = client.post_json(&url, &payload, "send_keys")?;
    let _ = Uia2HttpClient::parse_appium_value(&body)?;
    Ok(())
}

/// `GET /session/:id/element/:el/text` — ungated.
pub fn get_text(client: &Uia2HttpClient, element_id: &str) -> Result<String> {
    let sid = client.require_session()?;
    let url = format!(
        "{}/session/{sid}/element/{element_id}/text",
        client.base_url()
    );
    let body = client.get_json(&url, "get_text")?;
    let value = Uia2HttpClient::parse_appium_value(&body)?;
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| uia2_http_err("get_text", format!("expected string value, got {value}")))
}

/// `GET /session/:id/source` — ungated page source / hierarchy.
pub fn page_source(client: &Uia2HttpClient) -> Result<String> {
    let sid = client.require_session()?;
    let url = format!("{}/session/{sid}/source", client.base_url());
    let body = client.get_json(&url, "page_source")?;
    let value = Uia2HttpClient::parse_appium_value(&body)?;
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| uia2_http_err("page_source", format!("expected string value, got {value}")))
}

pub(crate) fn read_json_response(resp: ureq::Response, method: &str) -> Result<Value> {
    let status = resp.status();
    let text = resp
        .into_string()
        .map_err(|e| uia2_http_err(method, format!("read body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(uia2_http_err(
            method,
            format!(
                "http {status}: {}",
                text.chars().take(200).collect::<String>()
            ),
        ));
    }
    if text.trim().is_empty() {
        return Ok(json!({ "value": null }));
    }
    serde_json::from_str(&text).map_err(|e| {
        uia2_http_err(
            method,
            format!(
                "invalid json: {e}; body={}",
                text.chars().take(200).collect::<String>()
            ),
        )
    })
}

pub(crate) fn status_err(method: &str, code: u16, resp: ureq::Response) -> PhenoError {
    let text = resp.into_string().unwrap_or_default();
    uia2_http_err(
        method,
        format!(
            "http {code}: {}",
            text.chars().take(200).collect::<String>()
        ),
    )
}
