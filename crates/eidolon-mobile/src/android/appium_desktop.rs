//! Appium Desktop / Inspector GUI launcher + local session dashboard.
//!
//! wraps: Appium Inspector (OSS) when installed on PATH / Applications —
//! launches it against [`super::appium_probe`] / UIA2 endpoints.
//!
//! Also serves a minimal local HTML dashboard (no Electron bundle) that
//! proxies status via the existing HTTP session client surface.
//!
//! Feature: `mobile-appium-desktop` (implies `mobile-appium`).

use super::appium_probe::{AppiumProbe, APPIUM_URL_ENV, DEFAULT_APPIUM_URL};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

/// Env: bind address for local dashboard (`EIDOLON_APPIUM_DESKTOP_BIND`, default `127.0.0.1:17890`).
pub const APPIUM_DESKTOP_BIND_ENV: &str = "EIDOLON_APPIUM_DESKTOP_BIND";
/// Env: path to Appium Inspector binary / .app (`EIDOLON_APPIUM_INSPECTOR`).
pub const APPIUM_INSPECTOR_ENV: &str = "EIDOLON_APPIUM_INSPECTOR";

fn desktop_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::MOBILE_APPIUM_UNAVAILABLE,
        format!("AppiumDesktop::{method} unavailable — {detail}"),
    )
}

/// Resolve Inspector binary (env → PATH → macOS Applications).
pub fn resolve_inspector() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(APPIUM_INSPECTOR_ENV) {
        let path = PathBuf::from(p);
        if path.is_file() || path.is_dir() {
            return Some(path);
        }
    }
    for name in ["appium-inspector", "Appium Inspector", "appium_inspector"] {
        if let Ok(out) = Command::new("which").arg(name).output() {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let app = PathBuf::from("/Applications/Appium Inspector.app");
        if app.is_dir() {
            return Some(app);
        }
    }
    None
}

/// Launch Appium Inspector against `server_url` when installed.
pub fn launch_inspector(server_url: &str) -> Result<PathBuf> {
    let path = resolve_inspector().ok_or_else(|| {
        desktop_unavailable(
            "launch_inspector",
            format!(
                "Appium Inspector not found — install from \
                 https://github.com/appium/appium-inspector/releases \
                 or set {APPIUM_INSPECTOR_ENV}. Local dashboard still available \
                 via AppiumDesktop::serve_dashboard"
            ),
        )
    })?;
    let url = server_url.trim_end_matches('/');
    #[cfg(target_os = "macos")]
    {
        if path.extension().and_then(|e| e.to_str()) == Some("app")
            || path.to_string_lossy().ends_with(".app")
        {
            Command::new("open")
                .args(["-a", path.to_str().unwrap_or(""), "--args", "--remote-server-url", url])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| desktop_unavailable("launch_inspector", e))?;
            return Ok(path);
        }
    }
    Command::new(&path)
        .args(["--remote-server-url", url])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| desktop_unavailable("launch_inspector", e))?;
    Ok(path)
}

fn default_bind() -> String {
    std::env::var(APPIUM_DESKTOP_BIND_ENV).unwrap_or_else(|_| "127.0.0.1:17890".into())
}

fn dashboard_html(server_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en"><head>
<meta charset="utf-8"/>
<title>Eidolon Appium Desktop</title>
<style>
:root {{ --bg:#0f1419; --fg:#e7ecf3; --accent:#3d9cf0; --muted:#8b9bb4; }}
html,body {{ margin:0; background:var(--bg); color:var(--fg);
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif; }}
main {{ max-width: 42rem; margin: 12vh auto; padding: 0 1.5rem; }}
h1 {{ font-family: "IBM Plex Mono", ui-monospace, monospace; font-weight: 500;
  letter-spacing: -0.02em; font-size: 1.75rem; }}
p {{ color: var(--muted); line-height: 1.5; }}
code {{ color: var(--accent); }}
button {{ background: var(--accent); color: #041018; border:0; padding: 0.65rem 1.1rem;
  font: inherit; font-weight: 600; cursor: pointer; }}
button:hover {{ filter: brightness(1.08); }}
#status {{ margin-top: 1.25rem; white-space: pre-wrap; font-family: ui-monospace, monospace;
  font-size: 0.85rem; color: var(--fg); }}
</style></head><body><main>
<h1>Eidolon Appium Desktop</h1>
<p>Session target: <code id="url">{server}</code></p>
<p>Probes Appium/UIA2 <code>/status</code> through this local dashboard
(no Electron bundle). Prefer Appium Inspector when installed.</p>
<button id="probe" type="button">Probe /status</button>
<pre id="status">Ready.</pre>
<script>
const server = document.getElementById('url').textContent;
document.getElementById('probe').onclick = async () => {{
  const el = document.getElementById('status');
  el.textContent = 'Probing…';
  try {{
    const r = await fetch('/proxy/status');
    const t = await r.text();
    el.textContent = r.status + '\n' + t;
  }} catch (e) {{
    el.textContent = String(e);
  }}
}};
</script></main></body></html>
"#,
        server = html_escape(server_url)
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn proxy_status(server_url: &str) -> (u16, String) {
    let url = format!("{}/status", server_url.trim_end_matches('/'));
    match ureq::get(&url).timeout(Duration::from_secs(3)).call() {
        Ok(resp) => {
            let code = resp.status();
            let body = resp.into_string().unwrap_or_default();
            (code, body)
        }
        Err(e) => (502, format!("proxy error: {e}")),
    }
}

fn http_response(status: u16, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Serve the local Appium desktop dashboard (blocking). Returns bind address.
pub fn serve_dashboard(server_url: &str) -> Result<String> {
    let bind = default_bind();
    let listener = TcpListener::bind(&bind).map_err(|e| {
        desktop_unavailable("serve_dashboard", format!("bind {bind}: {e}"))
    })?;
    let addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or(bind);
    let server = server_url.trim_end_matches('/').to_string();
    log::info!("Appium desktop dashboard at http://{addr} (upstream {server})");

    // Single-threaded accept loop is enough for local operator UI.
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = handle_client(stream, &server);
        }
    });
    // Give the listener a tick to start.
    thread::sleep(Duration::from_millis(50));
    Ok(addr)
}

fn handle_client(mut stream: std::net::TcpStream, server: &str) -> std::io::Result<()> {
    use std::io::{Read, Write};
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let line = req.lines().next().unwrap_or("");
    let resp = if line.starts_with("GET /proxy/status") {
        let (code, body) = proxy_status(server);
        http_response(code, "application/json; charset=utf-8", &body)
    } else if line.starts_with("GET / ") || line.starts_with("GET /HTTP") || line.starts_with("GET /?") {
        http_response(200, "text/html; charset=utf-8", &dashboard_html(server))
    } else if line.starts_with("GET /") {
        http_response(200, "text/html; charset=utf-8", &dashboard_html(server))
    } else {
        http_response(405, "text/plain; charset=utf-8", "method not allowed")
    };
    stream.write_all(resp.as_bytes())?;
    Ok(())
}

/// Resolve session server URL from probe / env / defaults.
pub fn resolve_server_url(probe: &AppiumProbe) -> String {
    probe
        .effective_url()
        .or_else(|| std::env::var(APPIUM_URL_ENV).ok())
        .unwrap_or_else(|| DEFAULT_APPIUM_URL.to_string())
}

/// Run desktop entry: prefer Inspector; always start local dashboard.
/// Returns `(dashboard_bind, inspector_path_opt)`.
pub fn run() -> Result<(String, Option<PathBuf>)> {
    let probe = AppiumProbe::discover();
    let url = resolve_server_url(&probe);
    let bind = serve_dashboard(&url)?;
    let inspector = match launch_inspector(&url) {
        Ok(p) => Some(p),
        Err(e) => {
            log::warn!("{e}");
            None
        }
    };
    if inspector.is_none() && !probe.tools_present() {
        // Dashboard is up; still warn loudly that no Appium endpoint was discovered.
        log::warn!(
            "No Appium CLI/URL discovered — dashboard proxies {url} and will fail loud on probe"
        );
    }
    Ok((bind, inspector))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-001
    #[test]
    fn resolve_server_url_defaults() {
        let probe = AppiumProbe {
            cli: None,
            home: None,
            url: None,
        };
        assert_eq!(resolve_server_url(&probe), DEFAULT_APPIUM_URL);
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn dashboard_html_includes_server() {
        let html = dashboard_html("http://127.0.0.1:4723");
        assert!(html.contains("http://127.0.0.1:4723"));
        assert!(html.contains("Eidolon Appium Desktop"));
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn launch_inspector_fails_loud_when_missing() {
        std::env::remove_var(APPIUM_INSPECTOR_ENV);
        // May succeed if user has Inspector installed — that is fine.
        if resolve_inspector().is_none() {
            let err = launch_inspector("http://127.0.0.1:4723").unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::MOBILE_APPIUM_UNAVAILABLE)
            );
        }
    }
}
