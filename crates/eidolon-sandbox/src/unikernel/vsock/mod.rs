//! Guest vsock transport (AF_VSOCK) + minimal NDJSON exec framing.
//!
//! # Honesty
//!
//! - **Wire format** is always-on (macOS-safe builders / parsers) — see
//!   `docs/reference/unikernel-vsock-protocol.md`.
//! - **Live AF_VSOCK connect** requires feature `sandbox-vsock` **and** Linux.
//!   macOS and feature-off → fail-loud [`codes::SANDBOX_GUEST_IO_UNAVAILABLE`].
//! - Destructive live I/O also requires [`VSOCK_INTEGRATION_ENV`]=`1`.
//! - Do **not** unarchive KDesktopVirt for this path; Firecracker guest CID
//!   + port is the documented endpoint.
//!
//! # Endpoint env (prefer vsock when both set)
//!
//! - [`VSOCK_CID_ENV`] (`EIDOLON_VSOCK_CID`) — guest CID (typically ≥ 3)
//! - [`VSOCK_PORT_ENV`] (`EIDOLON_VSOCK_PORT`) — guest listen port

mod connect;

use std::io::Write;
use std::time::Duration;

pub use connect::{connect_guest_vsock, VsockStream};
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use serde::{Deserialize, Serialize};

use crate::codes;

pub(crate) fn guest_io_unavailable(detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_GUEST_IO_UNAVAILABLE,
        format!(
            "guest exec I/O unavailable — {detail} (vsock: feature `sandbox-vsock` \
             + Linux + {VSOCK_INTEGRATION_ENV}=1; else serial-stdio + \
             UNIKERNEL_EXEC_INTEGRATION=1; docs/reference/unikernel-vsock-protocol.md)"
        ),
    )
}

/// Env gate for live AF_VSOCK guest exec I/O.
pub const VSOCK_INTEGRATION_ENV: &str = "UNIKERNEL_VSOCK_INTEGRATION";

/// Guest CID override used by [`endpoint_from_env`].
pub const VSOCK_CID_ENV: &str = "EIDOLON_VSOCK_CID";

/// Guest vsock port override used by [`endpoint_from_env`].
pub const VSOCK_PORT_ENV: &str = "EIDOLON_VSOCK_PORT";

/// Protocol version embedded in NDJSON frames.
pub const VSOCK_PROTOCOL_VERSION: u32 = 1;

/// Default Firecracker-style guest CID when documenting examples (not auto-used).
pub const DEFAULT_GUEST_CID: u32 = 3;

/// Max response bytes accepted from a vsock agent (runaway guard).
pub const MAX_VSOCK_RESPONSE_BYTES: usize = 64 * 1024;

/// Host → guest NDJSON exec request (`op` = `"exec"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VsockExecRequest {
    pub v: u32,
    pub op: String,
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Guest → host NDJSON exec response (`op` = `"exec_result"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VsockExecResponse {
    pub v: u32,
    pub op: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl VsockExecRequest {
    /// Build a v1 exec request for `cmd` (caller must already validate cmd).
    pub fn exec(cmd: impl Into<String>) -> Self {
        Self {
            v: VSOCK_PROTOCOL_VERSION,
            op: "exec".into(),
            cmd: cmd.into(),
            id: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

/// Encode a single NDJSON request line (trailing `\n`).
pub fn vsock_exec_request_frame(cmd: &str) -> Result<Vec<u8>> {
    let req = VsockExecRequest::exec(cmd);
    vsock_request_frame(&req)
}

/// Encode an arbitrary [`VsockExecRequest`] as one NDJSON line (trailing `\n`).
pub fn vsock_request_frame(req: &VsockExecRequest) -> Result<Vec<u8>> {
    let mut line = serde_json::to_vec(req)
        .map_err(|e| PhenoError::BadRequest(format!("vsock request serialize failed: {e}")))?;
    line.push(b'\n');
    Ok(line)
}

/// Encode a [`VsockExecResponse`] as one NDJSON line (trailing `\n`).
pub fn vsock_response_frame(resp: &VsockExecResponse) -> Result<Vec<u8>> {
    let mut line = serde_json::to_vec(resp)
        .map_err(|e| PhenoError::BadRequest(format!("vsock response serialize failed: {e}")))?;
    line.push(b'\n');
    Ok(line)
}

/// Parse one NDJSON request line (optional trailing newline / whitespace).
///
/// Guest agents use this; the host path builds requests via
/// [`vsock_exec_request_frame`].
pub fn parse_vsock_exec_request(bytes: &[u8]) -> Result<VsockExecRequest> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| guest_io_unavailable(format!("vsock request is not UTF-8: {e}")))?;
    let line = text.trim();
    if line.is_empty() {
        return Err(guest_io_unavailable("vsock request empty"));
    }
    let req: VsockExecRequest = serde_json::from_str(line)
        .map_err(|e| guest_io_unavailable(format!("vsock request JSON parse failed: {e}")))?;
    if req.v != VSOCK_PROTOCOL_VERSION {
        return Err(guest_io_unavailable(format!(
            "vsock protocol version mismatch: got {}, expected {VSOCK_PROTOCOL_VERSION}",
            req.v
        )));
    }
    if req.op != "exec" {
        return Err(guest_io_unavailable(format!(
            "vsock unexpected op {:?}, expected exec",
            req.op
        )));
    }
    if req.cmd.is_empty() {
        return Err(guest_io_unavailable("vsock exec request missing cmd"));
    }
    Ok(req)
}

/// Parse one NDJSON response line (optional trailing newline / whitespace).
pub fn parse_vsock_exec_response(bytes: &[u8]) -> Result<VsockExecResponse> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| guest_io_unavailable(format!("vsock response is not UTF-8: {e}")))?;
    let line = text.trim();
    if line.is_empty() {
        return Err(guest_io_unavailable("vsock response empty"));
    }
    let resp: VsockExecResponse = serde_json::from_str(line)
        .map_err(|e| guest_io_unavailable(format!("vsock response JSON parse failed: {e}")))?;
    if resp.v != VSOCK_PROTOCOL_VERSION {
        return Err(guest_io_unavailable(format!(
            "vsock protocol version mismatch: got {}, expected {VSOCK_PROTOCOL_VERSION}",
            resp.v
        )));
    }
    if resp.op != "exec_result" {
        return Err(guest_io_unavailable(format!(
            "vsock unexpected op {:?}, expected exec_result",
            resp.op
        )));
    }
    Ok(resp)
}

/// Extract stdout (or stderr/error) from a parsed response for [`SandboxAutomator::exec`].
pub fn vsock_response_text(resp: &VsockExecResponse) -> Result<String> {
    if resp.ok {
        Ok(resp.stdout.clone())
    } else if let Some(err) = &resp.error {
        Err(guest_io_unavailable(format!(
            "vsock agent error (exit {}): {err}",
            resp.exit_code
        )))
    } else if !resp.stderr.is_empty() {
        Err(guest_io_unavailable(format!(
            "vsock agent failed (exit {}): {}",
            resp.exit_code, resp.stderr
        )))
    } else {
        Err(guest_io_unavailable(format!(
            "vsock agent failed with exit_code {}",
            resp.exit_code
        )))
    }
}

/// `true` when live AF_VSOCK I/O is env-gated on.
pub fn vsock_integration_enabled() -> bool {
    std::env::var(VSOCK_INTEGRATION_ENV).ok().as_deref() == Some("1")
}

/// Read `(cid, port)` from env when **both** [`VSOCK_CID_ENV`] and [`VSOCK_PORT_ENV`] are set.
pub fn endpoint_from_env() -> Option<(u32, u32)> {
    let cid = std::env::var(VSOCK_CID_ENV).ok()?.parse().ok()?;
    let port = std::env::var(VSOCK_PORT_ENV).ok()?.parse().ok()?;
    validate_endpoint(cid, port).ok()
}

/// Validate a guest vsock endpoint (pure; platform-agnostic).
///
/// Guest CIDs are typically ≥ 3 (0=hypervisor, 1=local/reserved, 2=host).
/// Port must be non-zero.
pub fn validate_endpoint(guest_cid: u32, port: u32) -> Result<(u32, u32)> {
    if guest_cid < 3 {
        return Err(PhenoError::BadRequest(format!(
            "vsock guest_cid must be >= 3 (got {guest_cid}; 0=hypervisor, 1=local, 2=host)"
        )));
    }
    if port == 0 {
        return Err(PhenoError::BadRequest("vsock port must be >= 1".into()));
    }
    Ok((guest_cid, port))
}

/// Documented connect tuple: validates endpoint, then asserts platform/feature readiness.
///
/// - Always validates CID/port (BadRequest on hygiene failure).
/// - On Linux + `sandbox-vsock`: returns `Ok((cid, port))` (connect is separate).
/// - Else: fail-loud [`codes::SANDBOX_GUEST_IO_UNAVAILABLE`].
pub fn vsock_connect_spec(guest_cid: u32, port: u32) -> Result<(u32, u32)> {
    let endpoint = validate_endpoint(guest_cid, port)?;
    assert_vsock_platform()?;
    Ok(endpoint)
}

/// Fail-loud unless Linux + `sandbox-vsock` (macOS-safe unit path).
pub fn assert_vsock_platform() -> Result<()> {
    #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
    {
        Ok(())
    }
    #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
    {
        Err(vsock_platform_unavailable())
    }
}

pub(crate) fn vsock_platform_unavailable() -> PhenoError {
    #[cfg(not(feature = "sandbox-vsock"))]
    {
        guest_io_unavailable(
            "vsock AF_VSOCK requires feature `sandbox-vsock` (Linux only; \
             macOS has no AF_VSOCK — use GuestIoTransport::SerialStdio; \
             docs/reference/unikernel-vsock-protocol.md; do not unarchive \
             KDesktopVirt routinely)",
        )
    }
    #[cfg(all(feature = "sandbox-vsock", not(target_os = "linux")))]
    {
        guest_io_unavailable(
            "vsock AF_VSOCK is Linux-only (macOS has no guest CID sockets) — \
             use GuestIoTransport::SerialStdio; enable sandbox-vsock on Linux \
             hosts; docs/reference/unikernel-vsock-protocol.md",
        )
    }
    #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
    {
        // Unreachable when assert_vsock_platform short-circuits to Ok.
        guest_io_unavailable("vsock platform check failed unexpectedly")
    }
}

/// Connect to guest vsock and run one framed exec round-trip.
///
/// Requires [`vsock_integration_enabled`] and Linux + `sandbox-vsock`.
pub fn exec_via_vsock(guest_cid: u32, port: u32, cmd: &str, timeout_ms: u64) -> Result<String> {
    if !vsock_integration_enabled() {
        return Err(guest_io_unavailable(format!(
            "live vsock exec gated off; set {VSOCK_INTEGRATION_ENV}=1 \
             (destructive guest agent I/O; cid={guest_cid}, port={port})"
        )));
    }
    let (cid, port) = vsock_connect_spec(guest_cid, port)?;
    let frame = vsock_exec_request_frame(cmd)?;
    let mut stream = connect_guest_vsock(cid, port)?;
    stream.write_all(&frame).map_err(|e| {
        guest_io_unavailable(format!("vsock write failed (cid={cid}, port={port}): {e}"))
    })?;
    stream.flush().map_err(|e| {
        guest_io_unavailable(format!("vsock flush failed (cid={cid}, port={port}): {e}"))
    })?;
    let bytes = connect::read_line_with_timeout(&mut stream, Duration::from_millis(timeout_ms))?;
    let resp = parse_vsock_exec_response(&bytes)?;
    vsock_response_text(&resp)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn request_frame_is_ndjson_newline() {
        let frame = vsock_exec_request_frame("uname -a").expect("frame");
        assert!(frame.ends_with(b"\n"));
        let req = parse_vsock_exec_request(&frame).expect("parse request");
        assert_eq!(req.op, "exec");
        assert_eq!(req.cmd, "uname -a");
        let resp = parse_vsock_exec_response(
            br#"{"v":1,"op":"exec_result","ok":true,"stdout":"Linux\n","stderr":"","exit_code":0}"#,
        )
        .expect("parse");
        assert!(resp.ok);
        assert_eq!(vsock_response_text(&resp).unwrap(), "Linux\n");
        let out = vsock_response_frame(&resp).expect("response frame");
        assert!(out.ends_with(b"\n"));
    }

    #[test]
    fn parse_request_rejects_bad_op_and_version() {
        let err = parse_vsock_exec_request(br#"{"v":2,"op":"exec","cmd":"pwd"}"#).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
        );
        let err = parse_vsock_exec_request(br#"{"v":1,"op":"ping","cmd":"pwd"}"#).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
        );
    }

    #[test]
    fn response_error_fail_loud() {
        let resp = parse_vsock_exec_response(
            br#"{"v":1,"op":"exec_result","ok":false,"exit_code":127,"error":"not found"}"#,
        )
        .unwrap();
        let err = vsock_response_text(&resp).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
        );
    }

    #[test]
    fn validate_endpoint_rejects_reserved_cid() {
        assert!(validate_endpoint(2, 52).is_err());
        assert!(validate_endpoint(3, 0).is_err());
        assert_eq!(validate_endpoint(3, 52).unwrap(), (3, 52));
    }

    #[test]
    fn connect_spec_platform_gate() {
        let result = vsock_connect_spec(3, 52);
        #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
        {
            assert_eq!(result.unwrap(), (3, 52));
        }
        #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
        {
            let err = result.unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
            );
        }
    }

    #[test]
    fn vsock_integration_env_gate() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(VSOCK_INTEGRATION_ENV);
        assert!(!vsock_integration_enabled());
        std::env::set_var(VSOCK_INTEGRATION_ENV, "1");
        assert!(vsock_integration_enabled());
        std::env::remove_var(VSOCK_INTEGRATION_ENV);
    }

    #[test]
    fn endpoint_from_env_requires_both() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(VSOCK_CID_ENV);
        std::env::remove_var(VSOCK_PORT_ENV);
        assert!(endpoint_from_env().is_none());
        std::env::set_var(VSOCK_CID_ENV, "3");
        assert!(endpoint_from_env().is_none());
        std::env::set_var(VSOCK_PORT_ENV, "5252");
        assert_eq!(endpoint_from_env(), Some((3, 5252)));
        std::env::remove_var(VSOCK_CID_ENV);
        std::env::remove_var(VSOCK_PORT_ENV);
    }

    #[test]
    fn exec_via_vsock_without_env_fail_loud() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(VSOCK_INTEGRATION_ENV);
        let err = exec_via_vsock(3, 52, "pwd", 100).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
        );
    }
}
