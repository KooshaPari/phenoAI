//! Extended W3C / Appium session verbs on [`super::Uia2HttpClient`].
//!
//! Windows, contexts, `findElements`, pointer actions, screenshot, timeouts.
//! Fail-loud on HTTP / Appium errors. Feature: `mobile-uia2`.

use super::uia2_http::{uia2_http_err, Uia2Element, Uia2HttpClient};
use crate::cli::require_actions_allowed;
use eidolon_core::Result;
use serde_json::{json, Value};

impl Uia2HttpClient {
    // --- findElements -------------------------------------------------------

    /// `POST /session/:id/elements` — ungated multi-find (needs active session).
    pub fn find_elements(&self, strategy: &str, selector: &str) -> Result<Vec<Uia2Element>> {
        let id = self.require_session()?;
        let url = format!("{}/session/{id}/elements", self.base_url());
        let body = self.post_json(
            &url,
            &self.find_elements_body(strategy, selector),
            "find_elements",
        )?;
        let value = Self::parse_appium_value(&body)?;
        let arr = value.as_array().ok_or_else(|| {
            uia2_http_err(
                "find_elements",
                format!("expected array value, got {value}"),
            )
        })?;
        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            let el = Self::parse_element_id(item).ok_or_else(|| {
                uia2_http_err(
                    "find_elements",
                    format!("no ELEMENT id in item={item}"),
                )
            })?;
            out.push(Uia2Element::new(el));
        }
        Ok(out)
    }

    // --- windows ------------------------------------------------------------

    /// `GET /session/:id/window` — current window handle.
    pub fn get_window_handle(&self) -> Result<String> {
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/window", self.base_url());
        let body = self.get_json(&url, "get_window_handle")?;
        let value = Self::parse_appium_value(&body)?;
        value.as_str().map(str::to_string).ok_or_else(|| {
            uia2_http_err(
                "get_window_handle",
                format!("expected string handle, got {value}"),
            )
        })
    }

    /// `GET /session/:id/window/handles` — all window handles.
    pub fn get_window_handles(&self) -> Result<Vec<String>> {
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/window/handles", self.base_url());
        let body = self.get_json(&url, "get_window_handles")?;
        let value = Self::parse_appium_value(&body)?;
        let arr = value.as_array().ok_or_else(|| {
            uia2_http_err(
                "get_window_handles",
                format!("expected array, got {value}"),
            )
        })?;
        arr.iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    uia2_http_err(
                        "get_window_handles",
                        format!("non-string handle entry: {v}"),
                    )
                })
            })
            .collect()
    }

    /// `POST /session/:id/window` — switch to window handle (gated).
    pub fn switch_to_window(&self, handle: &str) -> Result<()> {
        require_actions_allowed("uia2_http_switch_to_window")?;
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/window", self.base_url());
        let body = self.post_json(
            &url,
            &json!({ "handle": handle, "name": handle }),
            "switch_to_window",
        )?;
        let _ = Self::parse_appium_value(&body)?;
        Ok(())
    }

    /// `DELETE /session/:id/window` — close current window (gated).
    pub fn close_window(&self) -> Result<()> {
        require_actions_allowed("uia2_http_close_window")?;
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/window", self.base_url());
        let body = self.delete_json(&url, "close_window")?;
        let _ = Self::parse_appium_value(&body)?;
        Ok(())
    }

    /// `GET /session/:id/window/rect` — window rectangle.
    pub fn get_window_rect(&self) -> Result<Value> {
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/window/rect", self.base_url());
        let body = self.get_json(&url, "get_window_rect")?;
        Self::parse_appium_value(&body)
    }

    /// `POST /session/:id/window/rect` — set window rect (gated).
    pub fn set_window_rect(&self, rect: &Value) -> Result<Value> {
        require_actions_allowed("uia2_http_set_window_rect")?;
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/window/rect", self.base_url());
        let body = self.post_json(&url, rect, "set_window_rect")?;
        Self::parse_appium_value(&body)
    }

    // --- contexts (Appium) --------------------------------------------------

    /// `GET /session/:id/contexts` — available contexts (NATIVE_APP / WEBVIEW_*).
    pub fn get_contexts(&self) -> Result<Vec<String>> {
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/contexts", self.base_url());
        let body = self.get_json(&url, "get_contexts")?;
        let value = Self::parse_appium_value(&body)?;
        let arr = value.as_array().ok_or_else(|| {
            uia2_http_err("get_contexts", format!("expected array, got {value}"))
        })?;
        arr.iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    uia2_http_err("get_contexts", format!("non-string context: {v}"))
                })
            })
            .collect()
    }

    /// `GET /session/:id/context` — current context.
    pub fn get_context(&self) -> Result<String> {
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/context", self.base_url());
        let body = self.get_json(&url, "get_context")?;
        let value = Self::parse_appium_value(&body)?;
        value.as_str().map(str::to_string).ok_or_else(|| {
            uia2_http_err("get_context", format!("expected string, got {value}"))
        })
    }

    /// `POST /session/:id/context` — switch context (gated).
    pub fn set_context(&self, name: &str) -> Result<()> {
        require_actions_allowed("uia2_http_set_context")?;
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/context", self.base_url());
        let body = self.post_json(&url, &json!({ "name": name }), "set_context")?;
        let _ = Self::parse_appium_value(&body)?;
        Ok(())
    }

    // --- W3C actions --------------------------------------------------------

    /// `POST /session/:id/actions` — perform W3C actions (gated).
    pub fn perform_actions(&self, actions: &Value) -> Result<()> {
        require_actions_allowed("uia2_http_perform_actions")?;
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/actions", self.base_url());
        let body = self.post_json(&url, actions, "perform_actions")?;
        let _ = Self::parse_appium_value(&body)?;
        Ok(())
    }

    /// Pointer tap convenience — builds [`Self::pointer_tap_actions`] then
    /// [`Self::perform_actions`] (gated).
    pub fn pointer_tap(&self, x: i64, y: i64) -> Result<()> {
        self.perform_actions(&Self::pointer_tap_actions(x, y))
    }

    /// `DELETE /session/:id/actions` — release pointer/key state (gated).
    pub fn release_actions(&self) -> Result<()> {
        require_actions_allowed("uia2_http_release_actions")?;
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/actions", self.base_url());
        let body = self.delete_json(&url, "release_actions")?;
        let _ = Self::parse_appium_value(&body)?;
        Ok(())
    }

    // --- screenshot ---------------------------------------------------------

    /// `GET /session/:id/screenshot` — PNG as base64 string (ungated).
    pub fn screenshot_base64(&self) -> Result<String> {
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/screenshot", self.base_url());
        let body = self.get_json(&url, "screenshot_base64")?;
        let value = Self::parse_appium_value(&body)?;
        value.as_str().map(str::to_string).ok_or_else(|| {
            uia2_http_err(
                "screenshot_base64",
                format!("expected base64 string, got {value}"),
            )
        })
    }

    // --- timeouts -----------------------------------------------------------

    /// `GET /session/:id/timeouts` — current timeouts object.
    pub fn get_timeouts(&self) -> Result<Value> {
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/timeouts", self.base_url());
        let body = self.get_json(&url, "get_timeouts")?;
        Self::parse_appium_value(&body)
    }

    /// `POST /session/:id/timeouts` — set timeouts (gated).
    pub fn set_timeouts(&self, timeouts: &Value) -> Result<()> {
        require_actions_allowed("uia2_http_set_timeouts")?;
        let sid = self.require_session()?;
        let url = format!("{}/session/{sid}/timeouts", self.base_url());
        let body = self.post_json(&url, timeouts, "set_timeouts")?;
        let _ = Self::parse_appium_value(&body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    struct MockServer {
        addr: String,
        _join: Option<thread::JoinHandle<()>>,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl MockServer {
        fn spawn(handler: impl Fn(&str, &str, &str) -> (u16, String) + Send + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let addr = format!("http://{}", listener.local_addr().unwrap());
            let log = Arc::new(Mutex::new(Vec::new()));
            let log_c = Arc::clone(&log);
            let join = thread::spawn(move || {
                for _ in 0..24 {
                    let Ok((mut stream, _)) = listener.accept() else {
                        break;
                    };
                    let mut buf = [0u8; 16384];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let raw = String::from_utf8_lossy(&buf[..n]);
                    let first = raw.lines().next().unwrap_or("");
                    let mut parts = first.split_whitespace();
                    let method = parts.next().unwrap_or("").to_string();
                    let path = parts.next().unwrap_or("").to_string();
                    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                    log_c
                        .lock()
                        .unwrap()
                        .push(format!("{method} {path}"));
                    let (code, resp_body) = handler(&method, &path, &body);
                    let resp = format!(
                        "HTTP/1.1 {code} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
                        resp_body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes());
                }
            });
            Self {
                addr,
                _join: Some(join),
                log,
            }
        }
    }

    fn client(addr: &str) -> Uia2HttpClient {
        Uia2HttpClient::with_base_url(addr)
            .with_timeout(Duration::from_secs(2))
            .with_session_id("sess-ext")
    }

    #[test]
    fn find_elements_and_screenshot_against_mock() {
        let server = MockServer::spawn(|method, path, _body| match (method, path) {
            ("POST", p) if p.ends_with("/elements") => (
                200,
                json!({
                    "value": [
                        { "ELEMENT": "el-a" },
                        { "ELEMENT": "el-b" }
                    ]
                })
                .to_string(),
            ),
            ("GET", p) if p.ends_with("/screenshot") => {
                (200, json!({"value": "aGVsbG8="}).to_string())
            }
            ("GET", p) if p.ends_with("/timeouts") => (
                200,
                json!({"value": {"implicit": 0, "pageLoad": 300000, "script": 30000}}).to_string(),
            ),
            ("GET", p) if p.ends_with("/contexts") => {
                (200, json!({"value": ["NATIVE_APP", "WEBVIEW_1"]}).to_string())
            }
            ("GET", p) if p.ends_with("/context") => {
                (200, json!({"value": "NATIVE_APP"}).to_string())
            }
            ("GET", p) if p.ends_with("/window/handles") => {
                (200, json!({"value": ["w1", "w2"]}).to_string())
            }
            ("GET", p) if p.ends_with("/window") => {
                (200, json!({"value": "w1"}).to_string())
            }
            ("GET", p) if p.ends_with("/window/rect") => (
                200,
                json!({"value": {"x": 0, "y": 0, "width": 1080, "height": 1920}}).to_string(),
            ),
            _ => (404, json!({"value": {"error": "unknown", "message": path}}).to_string()),
        });
        let c = client(&server.addr);
        let els = c.find_elements("xpath", "//*").expect("find_elements");
        assert_eq!(els.len(), 2);
        assert_eq!(els[0].id, "el-a");
        assert_eq!(c.screenshot_base64().unwrap(), "aGVsbG8=");
        let timeouts = c.get_timeouts().unwrap();
        assert_eq!(timeouts["implicit"], 0);
        assert_eq!(
            c.get_contexts().unwrap(),
            vec!["NATIVE_APP".to_string(), "WEBVIEW_1".to_string()]
        );
        assert_eq!(c.get_context().unwrap(), "NATIVE_APP");
        assert_eq!(c.get_window_handle().unwrap(), "w1");
        assert_eq!(c.get_window_handles().unwrap(), vec!["w1", "w2"]);
        assert_eq!(c.get_window_rect().unwrap()["width"], 1080);
        let log = server.log.lock().unwrap();
        assert!(log.iter().any(|l| l.contains("/elements")));
        assert!(log.iter().any(|l| l.contains("/screenshot")));
    }

    #[test]
    fn perform_actions_gated_without_env() {
        if crate::cli::actions_allowed() {
            return;
        }
        let c = Uia2HttpClient::with_base_url("http://127.0.0.1:9").with_session_id("s");
        let err = c
            .perform_actions(&Uia2HttpClient::pointer_tap_actions(10, 20))
            .unwrap_err();
        assert_eq!(err.unsupported_code(), Some(codes::MOBILE_ACTIONS_GATED));
    }

    #[test]
    fn pointer_tap_actions_shape() {
        let a = Uia2HttpClient::pointer_tap_actions(100, 200);
        assert_eq!(a["actions"][0]["type"], "pointer");
        assert_eq!(a["actions"][0]["actions"][0]["x"], 100);
        assert_eq!(a["actions"][0]["actions"][0]["y"], 200);
    }

    #[test]
    fn unsupported_fails_loud() {
        let err = Uia2HttpClient::unsupported("execute_driver_script");
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_UIA2_UNAVAILABLE)
        );
    }

    #[test]
    fn actions_against_mock_when_allowed() {
        if !crate::cli::actions_allowed() {
            return;
        }
        let server = MockServer::spawn(|method, path, _body| {
            if method == "POST" && path.ends_with("/actions") {
                (200, json!({"value": null}).to_string())
            } else if method == "DELETE" && path.ends_with("/actions") {
                (200, json!({"value": null}).to_string())
            } else {
                (404, "{}".into())
            }
        });
        let c = client(&server.addr);
        c.pointer_tap(1, 2).expect("pointer_tap");
        c.release_actions().expect("release");
    }
}
