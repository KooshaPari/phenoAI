use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::{SessionWireMode, Uia2HttpClient};
use crate::codes;

/// Tiny hermetic HTTP mock (no wiremock dep).
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
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let join = thread::spawn(move || {
            let _ = ready_tx.send(());
            for _ in 0..16 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut raw = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            raw.extend_from_slice(&buf[..n]);
                            if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                                let text = String::from_utf8_lossy(&raw);
                                if let Some(cl) = text
                                    .lines()
                                    .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                                    .and_then(|l| l.split(':').nth(1))
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                                {
                                    if let Some(pos) = text.find("\r\n\r\n") {
                                        let body_len = raw.len().saturating_sub(pos + 4);
                                        if body_len >= cl {
                                            break;
                                        }
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                        Err(_) => break,
                    }
                    if raw.len() > 1_048_576 {
                        break;
                    }
                }
                let text = String::from_utf8_lossy(&raw);
                let first = text.lines().next().unwrap_or("");
                let mut parts = first.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                log_c.lock().unwrap().push(format!("{method} {path}"));
                let (code, resp_body) = handler(&method, &path, &body);
                let reason = if (200..300).contains(&code) {
                    "OK"
                } else {
                    "ERR"
                };
                let resp = format!(
                    "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
                    resp_body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        });
        let _ = ready_rx.recv();
        thread::sleep(Duration::from_millis(5));
        Self {
            addr,
            _join: Some(join),
            log,
        }
    }
}

#[test]
fn parse_element_id_jsonwp_and_w3c() {
    let v = json!({"ELEMENT": "el-1", "element-6066-11e4-a52e-4f735466cecf": "el-1"});
    assert_eq!(
        Uia2HttpClient::parse_element_id(&v).as_deref(),
        Some("el-1")
    );
    assert_eq!(
        Uia2HttpClient::parse_element_id(&json!("bare")).as_deref(),
        Some("bare")
    );
    assert!(Uia2HttpClient::parse_element_id(&json!({})).is_none());
}

#[test]
fn parse_appium_value_errors_loud() {
    let body = json!({
        "sessionId": "s",
        "value": { "error": "no such element", "message": "missing" }
    });
    let err = Uia2HttpClient::parse_appium_value(&body).unwrap_err();
    assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
}

#[test]
fn find_element_body_is_uia2_shaped_by_default() {
    let client = Uia2HttpClient::with_base_url("http://127.0.0.1:6790");
    let b = client.find_element_body("xpath", "//*[@text='Hi']");
    assert_eq!(b["strategy"], "xpath");
    assert_eq!(b["selector"], "//*[@text='Hi']");
    assert_eq!(b["multiple"], false);
    assert_eq!(client.wire_mode(), SessionWireMode::Uia2Server);
}

#[test]
fn for_appium_translates_find_and_session_bodies() {
    let client = Uia2HttpClient::for_appium("http://127.0.0.1:4723");
    assert_eq!(client.wire_mode(), SessionWireMode::AppiumW3c);
    let find = client.find_element_body("id", "com.ex:id/a");
    assert_eq!(find["using"], "id");
    assert_eq!(find["value"], "com.ex:id/a");
    assert!(find.get("strategy").is_none());
    let caps = client.create_session_body();
    assert_eq!(
        caps["capabilities"]["alwaysMatch"]["appium:automationName"],
        "UiAutomator2"
    );
}

#[test]
fn appium_find_element_posts_using_value_against_mock() {
    let server = MockServer::spawn(|method, path, body| match (method, path) {
        ("POST", p) if p.ends_with("/element") => {
            let v: Value = serde_json::from_str(body).unwrap_or(json!({}));
            assert_eq!(v["using"], "accessibility id");
            assert_eq!(v["value"], "Submit");
            assert!(v.get("strategy").is_none());
            (
                200,
                json!({
                    "sessionId": "sess-1",
                    "value": { "ELEMENT": "el-w3c" }
                })
                .to_string(),
            )
        }
        _ => (
            404,
            json!({"value":{"error":"unknown","message":path}}).to_string(),
        ),
    });
    let client = Uia2HttpClient::for_appium(&server.addr)
        .with_timeout(Duration::from_secs(2))
        .with_session_id("sess-1");
    let el = client
        .find_element("accessibility id", "Submit")
        .expect("find");
    assert_eq!(el.id, "el-w3c");
}

#[test]
fn status_and_find_element_against_mock() {
    let server = MockServer::spawn(|method, path, _body| match (method, path) {
        ("GET", "/status") => (
            200,
            json!({"value":{"ready":true,"message":"ok"}}).to_string(),
        ),
        ("POST", p) if p.ends_with("/element") => (
            200,
            json!({
                "sessionId": "sess-1",
                "value": { "ELEMENT": "el-99" }
            })
            .to_string(),
        ),
        _ => (
            404,
            json!({"value":{"error":"unknown","message":path}}).to_string(),
        ),
    });
    let client = Uia2HttpClient::with_base_url(&server.addr)
        .with_timeout(Duration::from_secs(2))
        .with_session_id("sess-1");
    let status = client.status().expect("status");
    assert_eq!(status["value"]["ready"], true);
    let el = client
        .find_element("id", "com.example:id/btn")
        .expect("find");
    assert_eq!(el.id, "el-99");
    let log = server.log.lock().unwrap();
    assert!(log.iter().any(|l| l.starts_with("GET /status")));
    assert!(log.iter().any(|l| l.contains("/element")));
}

#[test]
fn click_gated_without_actions_env() {
    if crate::cli::actions_allowed() {
        return;
    }
    let client = Uia2HttpClient::with_base_url("http://127.0.0.1:9").with_session_id("s");
    let err = client.click("el").unwrap_err();
    assert_eq!(err.unsupported_code(), Some(codes::MOBILE_ACTIONS_GATED));
}

#[test]
fn require_session_fails_loud() {
    let client = Uia2HttpClient::with_base_url("http://127.0.0.1:9");
    let err = client.find_element("id", "x").unwrap_err();
    assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
}

#[test]
fn http_error_fails_loud() {
    let server = MockServer::spawn(|_m, _p, _b| {
        (
            500,
            json!({"value":{"error":"unknown error","message":"boom"}}).to_string(),
        )
    });
    let client = Uia2HttpClient::with_base_url(&server.addr).with_timeout(Duration::from_secs(2));
    let err = client.status().unwrap_err();
    assert_eq!(err.unsupported_code(), Some(codes::MOBILE_UIA2_UNAVAILABLE));
}

#[test]
fn click_against_mock_when_actions_allowed() {
    if !crate::cli::actions_allowed() {
        return;
    }
    let server = MockServer::spawn(|method, path, _body| {
        if method == "POST" && path.ends_with("/click") {
            (200, json!({"sessionId":"s","value":null}).to_string())
        } else {
            (404, "{}".into())
        }
    });
    let client = Uia2HttpClient::with_base_url(&server.addr)
        .with_timeout(Duration::from_secs(2))
        .with_session_id("s");
    client.click("el-1").expect("click");
}
