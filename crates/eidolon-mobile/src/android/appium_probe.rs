//! Optional Appium **server** discovery / probe (not Electron Desktop GUI).
//!
//! Hermetic when Appium is absent: probes return `None` / `false` without
//! inventing readiness. Fail-loud only when callers use
//! [`require_appium_ready`] or [`AppiumProbe::require_http_status`].
//!
//! Resolution order for HTTP base URL:
//! 1. [`APPIUM_URL_ENV`] (`EIDOLON_APPIUM_URL`)
//! 2. Default `http://127.0.0.1:4723` when probing status (never invents CLI)
//!
//! CLI / install home:
//! - [`APPIUM_HOME_ENV`] (`APPIUM_HOME`) — directory containing `appium` bin
//! - `PATH` via `which appium`
//!
//! Feature: `mobile-appium` (implies `mobile-uia2`). Do not unarchive kmobile.
//! Electron Appium Desktop remains out of scope.

use crate::cli::{resolve_override, which_bin};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Env override for Appium HTTP base URL (`EIDOLON_APPIUM_URL`).
pub const APPIUM_URL_ENV: &str = "EIDOLON_APPIUM_URL";

/// Standard Appium install home (`APPIUM_HOME`) — directory with `appium` bin.
pub const APPIUM_HOME_ENV: &str = "APPIUM_HOME";

/// Path override for the `appium` CLI (`EIDOLON_APPIUM`).
pub const APPIUM_PATH_ENV: &str = "EIDOLON_APPIUM";

/// Default Appium server listen URL (W3C /status).
pub const DEFAULT_APPIUM_URL: &str = "http://127.0.0.1:4723";

/// Snapshot of Appium discovery (hermetic — absent fields stay `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppiumProbe {
    pub cli: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub url: Option<String>,
}

impl AppiumProbe {
    /// Discover CLI / home / URL without spawning HTTP (hermetic).
    pub fn discover() -> Self {
        Self {
            cli: resolve_appium_cli(),
            home: resolve_appium_home(),
            url: env_appium_url(),
        }
    }

    /// `true` when a CLI binary or explicit URL override is present.
    pub fn tools_present(&self) -> bool {
        self.cli.is_some() || self.url.is_some() || self.home.is_some()
    }

    /// Prefer explicit URL, else default Appium port when tools exist.
    pub fn effective_url(&self) -> Option<String> {
        if let Some(u) = &self.url {
            return Some(u.trim_end_matches('/').to_string());
        }
        if self.cli.is_some() || self.home.is_some() {
            return Some(DEFAULT_APPIUM_URL.to_string());
        }
        None
    }

    /// First line of `appium --version` when CLI resolves; `None` if absent.
    pub fn version_line(&self) -> Option<String> {
        let bin = self.cli.as_ref()?;
        let output = Command::new(bin).arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    /// `GET {url}/status` via ureq — fail-loud on transport / non-2xx.
    pub fn http_status(&self, timeout: Duration) -> Result<Value> {
        let url = self.effective_url().ok_or_else(|| {
            PhenoError::unsupported_platform(
                codes::MOBILE_APPIUM_UNAVAILABLE,
                format!(
                    "AppiumProbe::http_status unavailable — set {APPIUM_URL_ENV} \
                     or install `appium` (APPIUM_HOME / PATH); hermetic when \
                     absent (feature `mobile-appium`; Appium Desktop GUI out \
                     of scope; do not unarchive kmobile)"
                ),
            )
        })?;
        let status_url = format!("{}/status", url.trim_end_matches('/'));
        let agent = ureq::AgentBuilder::new().timeout(timeout).build();
        match agent.get(&status_url).call() {
            Ok(resp) => {
                let text = resp.into_string().map_err(|e| {
                    PhenoError::unsupported_platform(
                        codes::MOBILE_APPIUM_UNAVAILABLE,
                        format!("AppiumProbe::http_status read body: {e}"),
                    )
                })?;
                if text.trim().is_empty() {
                    return Ok(serde_json::json!({ "value": null }));
                }
                serde_json::from_str(&text).map_err(|e| {
                    PhenoError::unsupported_platform(
                        codes::MOBILE_APPIUM_UNAVAILABLE,
                        format!("AppiumProbe::http_status invalid json: {e}"),
                    )
                })
            }
            Err(ureq::Error::Status(code, resp)) => {
                let text = resp.into_string().unwrap_or_default();
                Err(PhenoError::unsupported_platform(
                    codes::MOBILE_APPIUM_UNAVAILABLE,
                    format!(
                        "AppiumProbe::http_status http {code}: {}",
                        text.chars().take(200).collect::<String>()
                    ),
                ))
            }
            Err(e) => Err(PhenoError::unsupported_platform(
                codes::MOBILE_APPIUM_UNAVAILABLE,
                format!(
                    "AppiumProbe::http_status transport: {e} (url={status_url}; \
                     start Appium or set {APPIUM_URL_ENV})"
                ),
            )),
        }
    }

    /// Fail-loud unless HTTP `/status` succeeds.
    pub fn require_http_status(&self, timeout: Duration) -> Result<Value> {
        self.http_status(timeout)
    }

    /// Build an [`super::AppiumSessionClient`] against [`Self::effective_url`].
    ///
    /// Enables [`super::SessionWireMode::AppiumW3c`] so `create_session` /
    /// `find_element(s)` speak Appium/W3C (`using`/`value` + automation caps)
    /// rather than the direct UIA2-server dialect.
    pub fn session_client(&self) -> Result<super::AppiumSessionClient> {
        let url = self.effective_url().ok_or_else(|| {
            PhenoError::unsupported_platform(
                codes::MOBILE_APPIUM_UNAVAILABLE,
                format!(
                    "AppiumProbe::session_client unavailable — no \
                     {APPIUM_URL_ENV} / appium CLI / {APPIUM_HOME_ENV} \
                     (feature `mobile-appium`)"
                ),
            )
        })?;
        Ok(super::AppiumSessionClient::for_appium(url))
    }
}

/// Resolve `EIDOLON_APPIUM_URL` when non-empty (no reachability check).
pub fn env_appium_url() -> Option<String> {
    std::env::var(APPIUM_URL_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Resolve Appium home directory from `APPIUM_HOME` when it exists.
pub fn resolve_appium_home() -> Option<PathBuf> {
    let home = std::env::var(APPIUM_HOME_ENV).ok()?;
    let p = PathBuf::from(home.trim());
    p.is_dir().then_some(p)
}

/// Resolve `appium` CLI: `EIDOLON_APPIUM` → `APPIUM_HOME/bin/appium` /
/// `APPIUM_HOME/appium` → `PATH`.
pub fn resolve_appium_cli() -> Option<PathBuf> {
    if let Some(p) = resolve_override(APPIUM_PATH_ENV) {
        return Some(p);
    }
    if let Some(home) = resolve_appium_home() {
        for candidate in [
            home.join("bin").join("appium"),
            home.join("appium"),
            home.join("node_modules").join(".bin").join("appium"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    which_bin("appium")
}

/// `true` when CLI path or URL env is resolvable (no HTTP).
pub fn appium_tools_ready() -> bool {
    AppiumProbe::discover().tools_present()
}

/// `true` when `appium --version` succeeds.
pub fn appium_cli_ready() -> bool {
    AppiumProbe::discover().version_line().is_some()
}

/// Fail-loud unless tools are present (hermetic callers should use
/// [`appium_tools_ready`] instead).
pub fn require_appium_ready() -> Result<AppiumProbe> {
    let probe = AppiumProbe::discover();
    if probe.tools_present() {
        return Ok(probe);
    }
    Err(PhenoError::unsupported_platform(
        codes::MOBILE_APPIUM_UNAVAILABLE,
        format!(
            "Appium server tools unavailable — install `appium` on PATH, set \
             {APPIUM_HOME_ENV}, or set {APPIUM_URL_ENV} (feature \
             `mobile-appium`; Electron Appium Desktop GUI out of scope; do \
             not unarchive kmobile)"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn discover_is_hermetic_without_tools() {
        // May or may not find system appium; must not panic.
        let p = AppiumProbe::discover();
        assert_eq!(p.tools_present(), p.cli.is_some() || p.url.is_some() || p.home.is_some());
        if !p.tools_present() {
            assert!(p.effective_url().is_none());
            let err = require_appium_ready().unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::MOBILE_APPIUM_UNAVAILABLE)
            );
        }
    }

    #[test]
    fn constants_stable() {
        assert_eq!(APPIUM_URL_ENV, "EIDOLON_APPIUM_URL");
        assert_eq!(APPIUM_HOME_ENV, "APPIUM_HOME");
        assert_eq!(DEFAULT_APPIUM_URL, "http://127.0.0.1:4723");
    }

    #[test]
    fn http_status_against_mock_via_url_override() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}");
        let join = thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = r#"{"value":{"ready":true,"message":"appium"}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
        });
        let probe = AppiumProbe {
            cli: None,
            home: None,
            url: Some(url.clone()),
        };
        let status = probe
            .http_status(Duration::from_secs(2))
            .expect("http status");
        assert_eq!(status["value"]["ready"], true);
        let client = probe.session_client().expect("client");
        assert_eq!(client.base_url(), url.trim_end_matches('/'));
        assert_eq!(client.wire_mode(), crate::SessionWireMode::AppiumW3c);
        let _ = join.join();
    }

    #[test]
    fn http_status_fails_loud_without_url() {
        let probe = AppiumProbe {
            cli: None,
            home: None,
            url: None,
        };
        let err = probe.http_status(Duration::from_millis(50)).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_APPIUM_UNAVAILABLE)
        );
    }
}
