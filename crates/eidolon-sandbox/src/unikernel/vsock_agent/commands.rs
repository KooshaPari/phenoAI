//! In-guest vsock NDJSON command dispatch (exec / exec_result).

use std::process::Command;

use eidolon_core::error::PhenoError;
use eidolon_core::security::validate_exec_cmd;
use eidolon_core::Result;

use super::super::vsock::{
    parse_vsock_exec_request, vsock_response_frame, VsockExecRequest, VsockExecResponse,
    MAX_VSOCK_RESPONSE_BYTES, VSOCK_PROTOCOL_VERSION,
};

/// Split a validated command into argv tokens (no shell).
pub fn split_argv(cmd: &str) -> Result<Vec<String>> {
    validate_exec_cmd(cmd)?;
    let parts: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
    if parts.is_empty() {
        return Err(PhenoError::BadRequest(
            "exec command must contain at least one argv token".into(),
        ));
    }
    Ok(parts)
}

/// Marker appended when a capture is clipped (UTF-8, counted in the budget).
const TRUNCATION_MARKER: &str = "…[truncated]";

/// Bytes reserved for JSON keys / status / id so the framed line stays under
/// [`MAX_VSOCK_RESPONSE_BYTES`].
const EXEC_RESULT_FRAME_OVERHEAD: usize = 1024;

/// Truncate `s` to at most `max_bytes` on a UTF-8 char boundary (never panic).
pub fn truncate_to_public(s: String, max_bytes: usize) -> String {
    truncate_to(s, max_bytes)
}

fn truncate_to(mut s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }
    let keep = max_bytes.saturating_sub(TRUNCATION_MARKER.len());
    let mut end = keep.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str(TRUNCATION_MARKER);
    s
}

/// Cap stdout+stderr so the encoded `exec_result` stays under the host read budget.
pub(super) fn bound_exec_captures(stdout: String, stderr: String) -> (String, String) {
    let budget = MAX_VSOCK_RESPONSE_BYTES.saturating_sub(EXEC_RESULT_FRAME_OVERHEAD);
    let per = super::MAX_CAPTURE_BYTES.min(budget / 2);
    (truncate_to(stdout, per), truncate_to(stderr, per))
}

/// Run `cmd` after [`validate_exec_cmd`]. No shell unless `allow_shell`.
pub fn run_validated_cmd(cmd: &str, allow_shell: bool) -> VsockExecResponse {
    if let Err(e) = validate_exec_cmd(cmd) {
        return VsockExecResponse {
            v: VSOCK_PROTOCOL_VERSION,
            op: "exec_result".into(),
            ok: false,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 126,
            id: None,
            error: Some(format!("cmd validation failed: {e}")),
        };
    }

    let result = if allow_shell {
        Command::new("sh").arg("-c").arg(cmd).output()
    } else {
        match split_argv(cmd) {
            Ok(argv) => {
                let mut c = Command::new(&argv[0]);
                if argv.len() > 1 {
                    c.args(&argv[1..]);
                }
                c.output()
            }
            Err(e) => {
                return VsockExecResponse {
                    v: VSOCK_PROTOCOL_VERSION,
                    op: "exec_result".into(),
                    ok: false,
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 126,
                    id: None,
                    error: Some(format!("argv split failed: {e}")),
                };
            }
        }
    };

    match result {
        Ok(out) => {
            let exit_code = out.status.code().unwrap_or(1);
            let (stdout, stderr) = bound_exec_captures(
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            );
            let ok = out.status.success();
            VsockExecResponse {
                v: VSOCK_PROTOCOL_VERSION,
                op: "exec_result".into(),
                ok,
                stdout,
                stderr,
                exit_code,
                id: None,
                error: if ok {
                    None
                } else {
                    Some(format!("process exited with code {exit_code}"))
                },
            }
        }
        Err(e) => VsockExecResponse {
            v: VSOCK_PROTOCOL_VERSION,
            op: "exec_result".into(),
            ok: false,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 127,
            id: None,
            error: Some(format!("spawn failed: {e}")),
        },
    }
}

/// Handle one parsed exec request → `exec_result` (echoes `id` when present).
pub fn handle_exec_request(req: &VsockExecRequest, allow_shell: bool) -> VsockExecResponse {
    let mut resp = run_validated_cmd(&req.cmd, allow_shell);
    resp.id = req.id.clone();
    resp
}

/// Parse one NDJSON request line and produce an `exec_result` response frame.
///
/// On protocol/parse errors, still returns a serialized `exec_result` with
/// `ok: false` (fail-loud to the host without dropping the connection).
pub fn handle_request_line(line: &[u8], allow_shell: bool) -> Result<Vec<u8>> {
    match parse_vsock_exec_request(line) {
        Ok(req) => {
            let resp = handle_exec_request(&req, allow_shell);
            vsock_response_frame(&resp)
        }
        Err(e) => {
            let resp = VsockExecResponse {
                v: VSOCK_PROTOCOL_VERSION,
                op: "exec_result".into(),
                ok: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 2,
                id: None,
                error: Some(e.to_string()),
            };
            vsock_response_frame(&resp)
        }
    }
}
