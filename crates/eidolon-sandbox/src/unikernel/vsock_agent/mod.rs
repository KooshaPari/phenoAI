//! In-guest vsock NDJSON agent (AF_VSOCK listener + `exec` / `exec_result`).
//!
//! Framing + handler always-on; live AF_VSOCK listen requires Linux.
//! Feature `sandbox-vsock-agent` gates the binary only.
//! See `docs/guides/vsock-guest-agent.md` and
//! `docs/reference/unikernel-vsock-protocol.md`.

pub mod commands;

use std::io::{Read, Write};

pub use commands::{handle_exec_request, handle_request_line, run_validated_cmd, split_argv};
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use super::vsock::{VSOCK_CID_ENV, VSOCK_PORT_ENV};
use crate::codes;

pub const VMADDR_CID_ANY: u32 = 0xFFFF_FFFF;
pub const DEFAULT_AGENT_PORT: u32 = 5252;
pub const AGENT_ALLOW_SHELL_ENV: &str = "EIDOLON_VSOCK_AGENT_ALLOW_SHELL";
pub const MAX_CAPTURE_BYTES: usize = 32 * 1024;

fn guest_io_unavailable(detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_GUEST_IO_UNAVAILABLE,
        format!("vsock guest agent unavailable — {detail} (docs/guides/vsock-guest-agent.md)"),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub cid: u32,
    pub port: u32,
    pub allow_shell: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            cid: VMADDR_CID_ANY,
            port: DEFAULT_AGENT_PORT,
            allow_shell: false,
        }
    }
}

impl AgentConfig {
    pub fn from_env() -> Result<Self> {
        let mut cfg = Self::default();
        if let Ok(port) = std::env::var(VSOCK_PORT_ENV) {
            cfg.port = port.parse().map_err(|_| {
                PhenoError::BadRequest(format!("{VSOCK_PORT_ENV} must be u32, got {port:?}"))
            })?;
        }
        if let Ok(cid) = std::env::var(VSOCK_CID_ENV) {
            cfg.cid = cid.parse().map_err(|_| {
                PhenoError::BadRequest(format!("{VSOCK_CID_ENV} must be u32, got {cid:?}"))
            })?;
        }
        cfg.allow_shell = std::env::var(AGENT_ALLOW_SHELL_ENV).ok().as_deref() == Some("1");
        cfg.validate()?;
        Ok(cfg)
    }
    pub fn validate(&self) -> Result<()> {
        if self.port == 0 {
            return Err(PhenoError::BadRequest(
                "vsock agent port must be >= 1".into(),
            ));
        }
        Ok(())
    }
}

pub fn read_request_line(reader: &mut impl Read, max_bytes: usize) -> Result<Vec<u8>> {
    let mut buf = [0u8; 4096];
    let mut out = Vec::new();
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.contains(&b'\n') {
                    break;
                }
                if out.len() >= max_bytes {
                    return Err(guest_io_unavailable(format!(
                        "vsock agent request exceeded {max_bytes} bytes"
                    )));
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Err(guest_io_unavailable(format!(
                    "vsock agent read timed out: {e}"
                )));
            }
            Err(e) => {
                return Err(guest_io_unavailable(format!(
                    "vsock agent read failed: {e}"
                )));
            }
        }
    }
    if out.is_empty() {
        return Err(guest_io_unavailable("vsock agent read EOF with no data"));
    }
    Ok(out)
}

pub fn serve_connection(stream: &mut (impl Read + Write), allow_shell: bool) -> Result<()> {
    let line = read_request_line(stream, super::vsock::MAX_VSOCK_RESPONSE_BYTES)?;
    let frame = commands::handle_request_line(&line, allow_shell)?;
    stream
        .write_all(&frame)
        .map_err(|e| guest_io_unavailable(format!("vsock agent write failed: {e}")))?;
    stream
        .flush()
        .map_err(|e| guest_io_unavailable(format!("vsock agent flush failed: {e}")))?;
    Ok(())
}

pub fn serve(config: &AgentConfig) -> Result<()> {
    config.validate()?;
    serve_inner(config)
}

#[cfg(all(feature = "sandbox-vsock-agent", target_os = "linux"))]
fn serve_inner(config: &AgentConfig) -> Result<()> {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::io::AsRawFd;
    use std::time::Duration;
    let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(guest_io_unavailable(format!(
            "vsock agent socket(AF_VSOCK) failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut addr: libc::sockaddr_vm = unsafe { std::mem::zeroed() };
    addr.svm_family = libc::AF_VSOCK as libc::sa_family_t;
    addr.svm_cid = config.cid;
    addr.svm_port = config.port;
    let rc = unsafe {
        libc::bind(
            owned.as_raw_fd(),
            &addr as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(guest_io_unavailable(format!(
            "vsock agent bind(cid={}, port={}) failed: {}",
            config.cid,
            config.port,
            std::io::Error::last_os_error()
        )));
    }
    let rc = unsafe { libc::listen(owned.as_raw_fd(), 16) };
    if rc != 0 {
        return Err(guest_io_unavailable(format!(
            "vsock agent listen failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    eprintln!(
        "eidolon-vsock-agent listening on vsock cid={} port={} allow_shell={}",
        config.cid, config.port, config.allow_shell
    );
    loop {
        let mut peer: libc::sockaddr_vm = unsafe { std::mem::zeroed() };
        let mut peer_len = std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t;
        let conn = unsafe {
            libc::accept(
                owned.as_raw_fd(),
                &mut peer as *mut _ as *mut libc::sockaddr,
                &mut peer_len,
            )
        };
        if conn < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(guest_io_unavailable(format!(
                "vsock agent accept failed: {err}"
            )));
        }
        let conn_owned = unsafe { OwnedFd::from_raw_fd(conn) };
        let mut stream = std::net::TcpStream::from(conn_owned);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
        if let Err(e) = serve_connection(&mut stream, config.allow_shell) {
            eprintln!("eidolon-vsock-agent connection error: {e}");
        }
    }
}

#[cfg(not(all(feature = "sandbox-vsock-agent", target_os = "linux")))]
fn serve_inner(_config: &AgentConfig) -> Result<()> {
    #[cfg(not(feature = "sandbox-vsock-agent"))]
    {
        Err(guest_io_unavailable(
            "vsock agent listen requires feature `sandbox-vsock-agent`",
        ))
    }
    #[cfg(all(feature = "sandbox-vsock-agent", not(target_os = "linux")))]
    {
        Err(guest_io_unavailable("AF_VSOCK listen is Linux-only"))
    }
}

pub fn run_main(args: impl IntoIterator<Item = String>) -> i32 {
    match run_main_inner(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("eidolon-vsock-agent: {e}");
            1
        }
    }
}

fn run_main_inner(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut cfg = AgentConfig::from_env().unwrap_or_else(|_| AgentConfig::default());
    let raw: Vec<String> = args.into_iter().collect();
    let start = usize::from(
        raw.first()
            .is_some_and(|a| !a.starts_with('-') && a.contains("eidolon-vsock-agent")),
    );
    let args = &raw[start..];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            "--port" => {
                i += 1;
                let p = args
                    .get(i)
                    .ok_or_else(|| PhenoError::BadRequest("--port requires a value".into()))?;
                cfg.port = p.parse().map_err(|_| {
                    PhenoError::BadRequest(format!("--port must be u32, got {p:?}"))
                })?;
            }
            "--cid" => {
                i += 1;
                let c = args
                    .get(i)
                    .ok_or_else(|| PhenoError::BadRequest("--cid requires a value".into()))?;
                cfg.cid = c
                    .parse()
                    .map_err(|_| PhenoError::BadRequest(format!("--cid must be u32, got {c:?}")))?;
            }
            "--shell" => {
                cfg.allow_shell = true;
            }
            "--any-cid" => {
                cfg.cid = VMADDR_CID_ANY;
            }
            other => {
                return Err(PhenoError::BadRequest(format!(
                    "unknown argument {other:?} (try --help)"
                )));
            }
        }
        i += 1;
    }
    cfg.validate()?;
    serve(&cfg)
}

fn print_usage() {
    eprintln!("eidolon-vsock-agent — in-guest AF_VSOCK NDJSON exec agent (v1)\n\nUSAGE:\n  eidolon-vsock-agent [--port PORT] [--cid CID | --any-cid] [--shell]\n\nOPTIONS:\n  --port PORT   Listen port (default {DEFAULT_AGENT_PORT} or {VSOCK_PORT_ENV})\n  --cid CID     Bind CID (default {VMADDR_CID_ANY:#x} / {VSOCK_CID_ENV})\n  --any-cid     Bind VMADDR_CID_ANY (guest listen any)\n  --shell       Allow sh -c after validate_exec_cmd (default: argv only)\n  -h, --help    Show this help\n\nENV:\n  {VSOCK_PORT_ENV} / {VSOCK_CID_ENV}\n  {AGENT_ALLOW_SHELL_ENV}=1\n\nLinux guest only for live listen. macOS builds fail loud at runtime.\nSee docs/guides/vsock-guest-agent.md.");
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use commands::{bound_exec_captures, handle_exec_request, handle_request_line};

    use super::super::vsock::{
        vsock_response_frame, VsockExecRequest, VsockExecResponse, MAX_VSOCK_RESPONSE_BYTES,
        VSOCK_PROTOCOL_VERSION,
    };
    use super::*;

    #[test]
    fn truncate_capture_respects_utf8_char_boundary() {
        let mut s = String::new();
        while s.len() + 4 <= MAX_CAPTURE_BYTES + 8 {
            s.push('😀');
        }
        let mut over = "a".repeat(MAX_CAPTURE_BYTES - 1);
        over.push('😀');
        let clipped = commands::truncate_to_public(over, MAX_CAPTURE_BYTES);
        assert!(clipped.len() <= MAX_CAPTURE_BYTES + "…[truncated]".len());
        assert!(clipped.is_char_boundary(clipped.len()));
    }

    #[test]
    fn bound_exec_captures_fit_host_response_budget() {
        let big = "x".repeat(MAX_CAPTURE_BYTES + 4096);
        let (so, se) = bound_exec_captures(big.clone(), big);
        assert!(so.len() <= MAX_CAPTURE_BYTES);
        assert!(se.len() <= MAX_CAPTURE_BYTES);
        let overhead: usize = 1024;
        assert!(so.len() + se.len() + overhead <= MAX_VSOCK_RESPONSE_BYTES);
        let frame = vsock_response_frame(&VsockExecResponse {
            v: VSOCK_PROTOCOL_VERSION,
            op: "exec_result".into(),
            ok: true,
            stdout: so,
            stderr: se,
            exit_code: 0,
            id: Some("budget".into()),
            error: None,
        })
        .expect("frame");
        assert!(frame.len() <= MAX_VSOCK_RESPONSE_BYTES);
    }

    #[test]
    fn handle_request_round_trip_echo_id() {
        let req = VsockExecRequest::exec("true").with_id("req-1");
        let resp = handle_exec_request(&req, false);
        assert_eq!(resp.id.as_deref(), Some("req-1"));
        assert_eq!(resp.op, "exec_result");
        assert_eq!(resp.v, VSOCK_PROTOCOL_VERSION);
    }

    #[test]
    fn handle_rejects_shell_metachar_without_running() {
        let req = VsockExecRequest::exec("echo hi && rm -rf /");
        let resp = handle_exec_request(&req, false);
        assert!(!resp.ok);
        assert!(resp
            .error
            .as_deref()
            .unwrap_or("")
            .contains("validation failed"));
    }

    #[test]
    fn handle_request_line_bad_json_fail_loud_frame() {
        let frame = handle_request_line(br#"not-json"#, false).expect("frame");
        assert!(frame.ends_with(b"\n"));
        let text = std::str::from_utf8(&frame).unwrap();
        assert!(text.contains("\"ok\":false"));
        assert!(text.contains("exec_result"));
    }

    #[test]
    fn split_argv_rejects_empty_and_metachar() {
        assert!(split_argv("").is_err());
        assert!(split_argv("a|b").is_err());
        assert_eq!(split_argv("uname -a").unwrap(), vec!["uname", "-a"]);
    }

    #[test]
    fn serve_connection_over_memory_stream() {
        let req = br#"{"v":1,"op":"exec","cmd":"true","id":"m1"}"#;
        let mut input = Cursor::new({
            let mut v = req.to_vec();
            v.push(b'\n');
            v
        });
        let mut output = Vec::new();
        let line = read_request_line(&mut input, 64 * 1024).unwrap();
        let frame = commands::handle_request_line(&line, false).unwrap();
        output.extend_from_slice(&frame);
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("\"id\":\"m1\""));
        assert!(text.contains("exec_result"));
    }

    #[test]
    fn agent_config_rejects_zero_port() {
        let cfg = AgentConfig {
            port: 0,
            ..AgentConfig::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn serve_fail_loud_without_linux_feature() {
        #[cfg(not(all(feature = "sandbox-vsock-agent", target_os = "linux")))]
        {
            let err = serve(&AgentConfig::default()).unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
            );
        }
        #[cfg(all(feature = "sandbox-vsock-agent", target_os = "linux"))]
        {
            let _ = VMADDR_CID_ANY;
        }
    }

    #[test]
    fn run_main_help_exits_zero() {
        assert_eq!(
            run_main(vec!["eidolon-vsock-agent".into(), "--help".into()]),
            0
        );
    }

    #[test]
    fn run_main_unknown_flag_fail_loud() {
        assert_eq!(
            run_main(vec!["eidolon-vsock-agent".into(), "--nope".into()]),
            1
        );
    }
}
