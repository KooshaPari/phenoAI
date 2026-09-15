//! OAuth 2.0 Authorization Code + PKCE helper — Phase F optional surface.
//!
//! wraps: `sha2` + `urlencoding` patterns for S256 PKCE (no network until
//! [`OAuthPkceFlow::authorization_url`] is opened by the operator).
//!
//! Enable with feature `desktop-security-oauth`. Does **not** embed a browser;
//! builds authorize URL + verifies state/code_verifier locally.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::Write as _;

/// Env: OAuth authorize endpoint (`EIDOLON_OAUTH_AUTHORIZE_URL`).
pub const OAUTH_AUTHORIZE_URL_ENV: &str = "EIDOLON_OAUTH_AUTHORIZE_URL";
/// Env: OAuth client id (`EIDOLON_OAUTH_CLIENT_ID`).
pub const OAUTH_CLIENT_ID_ENV: &str = "EIDOLON_OAUTH_CLIENT_ID";
/// Env: OAuth redirect URI (`EIDOLON_OAUTH_REDIRECT_URI`).
pub const OAUTH_REDIRECT_URI_ENV: &str = "EIDOLON_OAUTH_REDIRECT_URI";

fn oauth_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::DESKTOP_SECURITY_UNAVAILABLE,
        format!("OAuthManager::{method} unavailable — {detail}"),
    )
}

fn b64url_nopad(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
    }
    out
}

fn random_b64url(nbytes: usize) -> Result<String> {
    let mut buf = vec![0u8; nbytes];
    OsRng
        .try_fill_bytes(&mut buf)
        .map_err(|e| oauth_unavailable("rng", e))?;
    Ok(b64url_nopad(&buf))
}

fn pkce_s256_challenge(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    b64url_nopad(&h.finalize())
}

fn pct_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// PKCE authorization-code flow parameters (local; no token exchange yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthPkceFlow {
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub state: String,
    pub code_verifier: String,
    pub code_challenge: String,
    pub scope: String,
}

impl OAuthPkceFlow {
    /// Build a new PKCE flow from env + optional scope (default `openid`).
    pub fn from_env(scope: Option<&str>) -> Result<Self> {
        let authorize_url = std::env::var(OAUTH_AUTHORIZE_URL_ENV).map_err(|_| {
            oauth_unavailable(
                "from_env",
                format!("set {OAUTH_AUTHORIZE_URL_ENV} (authorization endpoint)"),
            )
        })?;
        let client_id = std::env::var(OAUTH_CLIENT_ID_ENV).map_err(|_| {
            oauth_unavailable("from_env", format!("set {OAUTH_CLIENT_ID_ENV}"))
        })?;
        let redirect_uri = std::env::var(OAUTH_REDIRECT_URI_ENV).map_err(|_| {
            oauth_unavailable("from_env", format!("set {OAUTH_REDIRECT_URI_ENV}"))
        })?;
        Self::new(&authorize_url, &client_id, &redirect_uri, scope)
    }

    /// Construct PKCE parameters (S256).
    pub fn new(
        authorize_url: &str,
        client_id: &str,
        redirect_uri: &str,
        scope: Option<&str>,
    ) -> Result<Self> {
        if authorize_url.is_empty() || client_id.is_empty() || redirect_uri.is_empty() {
            return Err(oauth_unavailable(
                "new",
                "authorize_url, client_id, and redirect_uri must be non-empty",
            ));
        }
        let state = random_b64url(16)?;
        let code_verifier = random_b64url(32)?;
        let code_challenge = pkce_s256_challenge(&code_verifier);
        Ok(Self {
            authorize_url: authorize_url.trim_end_matches('/').to_string(),
            client_id: client_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            state,
            code_verifier,
            code_challenge,
            scope: scope.unwrap_or("openid").to_string(),
        })
    }

    /// Full browser authorization URL (operator opens this).
    pub fn authorization_url(&self) -> String {
        format!(
            "{base}?response_type=code&client_id={cid}&redirect_uri={redir}\
             &scope={scope}&state={state}&code_challenge={chal}\
             &code_challenge_method=S256",
            base = self.authorize_url,
            cid = pct_encode(&self.client_id),
            redir = pct_encode(&self.redirect_uri),
            scope = pct_encode(&self.scope),
            state = pct_encode(&self.state),
            chal = pct_encode(&self.code_challenge),
        )
    }

    /// Validate `state` from the redirect query against this flow.
    pub fn validate_state(&self, returned_state: &str) -> Result<()> {
        if returned_state != self.state {
            return Err(oauth_unavailable(
                "validate_state",
                "OAuth state mismatch (CSRF / wrong flow)",
            ));
        }
        Ok(())
    }

    /// Parse `code` + `state` from a redirect URL or raw query string.
    pub fn parse_redirect_params(input: &str) -> Result<(String, String)> {
        let q = input
            .split_once('?')
            .map(|(_, q)| q)
            .unwrap_or(input);
        let mut map = HashMap::new();
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            }
        }
        let code = map
            .get("code")
            .cloned()
            .ok_or_else(|| oauth_unavailable("parse_redirect", "missing code"))?;
        let state = map
            .get("state")
            .cloned()
            .ok_or_else(|| oauth_unavailable("parse_redirect", "missing state"))?;
        Ok((code, state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-001
    #[test]
    fn pkce_flow_builds_authorize_url() {
        let flow = OAuthPkceFlow::new(
            "https://auth.example/authorize",
            "client-1",
            "http://127.0.0.1:9876/cb",
            Some("openid profile"),
        )
        .unwrap();
        let url = flow.authorization_url();
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("client_id=client-1"));
        assert_eq!(flow.code_challenge, pkce_s256_challenge(&flow.code_verifier));
        flow.validate_state(&flow.state).unwrap();
        assert!(flow.validate_state("nope").is_err());
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn parse_redirect_extracts_code_state() {
        let (c, s) = OAuthPkceFlow::parse_redirect_params(
            "http://127.0.0.1:9876/cb?code=abc&state=xyz&extra=1",
        )
        .unwrap();
        assert_eq!(c, "abc");
        assert_eq!(s, "xyz");
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn from_env_fails_loud_when_unset() {
        std::env::remove_var(OAUTH_AUTHORIZE_URL_ENV);
        std::env::remove_var(OAUTH_CLIENT_ID_ENV);
        std::env::remove_var(OAUTH_REDIRECT_URI_ENV);
        let err = OAuthPkceFlow::from_env(None).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_SECURITY_UNAVAILABLE)
        );
    }
}
