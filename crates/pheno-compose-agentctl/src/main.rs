//! `phenocompose-agentctl`: a single-binary JSON-in / JSON-out CLI that exposes
//! PhenoCompose port-trait operations. Wire-compatible with the Omniroute
//! dispatcher schema.
//!
//! Usage:
//!   echo '{"method":"manifest.parse","params":{"name":"hello"}}' | phenocompose-agentctl

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Deserialize, Debug)]
struct Request {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize, Debug)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn dispatch(req: &Request) -> Response {
    match req.method.as_str() {
        "manifest.parse" => {
            // Stub: real implementation would call phenocompose_port_composer::Composer::Compose
            let name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                return Response {
                    ok: false,
                    result: None,
                    error: Some("manifest.parse: missing 'name'".into()),
                };
            }
            Response {
                ok: true,
                result: Some(serde_json::json!({"id": format!("mf-{}", name), "name": name})),
                error: None,
            }
        }
        "secret.get" => {
            let id = req.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return Response {
                    ok: false,
                    result: None,
                    error: Some("secret.get: missing 'id'".into()),
                };
            }
            Response {
                ok: true,
                result: Some(serde_json::json!({"id": id, "version": 1})),
                error: None,
            }
        }
        "secret.list" => Response {
            ok: true,
            result: Some(serde_json::json!([])),
            error: None,
        },
        other => Response {
            ok: false,
            result: None,
            error: Some(format!("unknown method: {}", other)),
        },
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::to_string(&Response {
                        ok: false,
                        result: None,
                        error: Some(format!("invalid json: {}", e)),
                    })
                    .unwrap()
                );
                continue;
            }
        };
        let resp = dispatch(&req);
        let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
    }
}
