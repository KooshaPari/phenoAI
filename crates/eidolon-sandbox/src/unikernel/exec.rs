//! Guest exec / serial I/O after unikernel boot (compose; no second VirtualStage).
//!
//! # Honesty
//!
//! - Preferred transport: **host process serial/stdio pipes** (Firecracker /
//!   `ops` console when spawned with piped stdin/stdout), unless
//!   [`EIDOLON_VSOCK_CID`](vsock::VSOCK_CID_ENV) +
//!   [`EIDOLON_VSOCK_PORT`](vsock::VSOCK_PORT_ENV) are both set — then
//!   [`GuestIoTransport::preferred`] selects vsock.
//! - **Vsock** framed NDJSON protocol is always-on (builders/parsers). Live
//!   AF_VSOCK connect requires feature `sandbox-vsock` + Linux +
//!   [`UNIKERNEL_VSOCK_INTEGRATION`](vsock::VSOCK_INTEGRATION_ENV)=`1`.
//!   macOS / feature-off → fail-loud [`codes::SANDBOX_GUEST_IO_UNAVAILABLE`].
//! - Live destructive serial writes require [`EXEC_INTEGRATION_ENV`]=`1`
//!   (in addition to a live guest child from boot).
//! - Always-on builders + fail-loud helpers are macOS-safe (no KVM / guest).
//!
//! See `docs/reference/unikernel-vsock-protocol.md`.

use crate::codes;
use crate::unikernel::vsock::{self, endpoint_from_env};
use eidolon_core::error::PhenoError;
use eidolon_core::security::validate_exec_cmd;
use eidolon_core::Result;
use std::io::{Read, Write};
use std::process::{Child, ChildStdout};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Env gate for destructive guest serial/stdio exec I/O.
///
/// When unset (default), [`exec_on_guest`] fails loud with
/// [`codes::SANDBOX_GUEST_IO_UNAVAILABLE`] even if a child exists.
pub const EXEC_INTEGRATION_ENV: &str = "UNIKERNEL_EXEC_INTEGRATION";

/// Default read budget for serial exec responses (milliseconds).
pub const DEFAULT_EXEC_TIMEOUT_MS: u64 = 5_000;

/// How guest command I/O is carried after boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestIoTransport {
    /// Host↔guest via the hypervisor process stdio pipes (serial console).
    SerialStdio,
    /// Firecracker vsock (CID + port) — live path behind `sandbox-vsock` + Linux.
    Vsock { guest_cid: u32, port: u32 },
}

impl GuestIoTransport {
    /// Stable label for diagnostics / metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SerialStdio => "serial-stdio",
            Self::Vsock { .. } => "vsock",
        }
    }

    /// Prefer vsock when `EIDOLON_VSOCK_CID` + `EIDOLON_VSOCK_PORT` are set; else serial.
    pub fn preferred() -> Self {
        match endpoint_from_env() {
            Some((guest_cid, port)) => Self::Vsock { guest_cid, port },
            None => Self::SerialStdio,
        }
    }
}

/// Validated guest exec request (pure; no I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestExecRequest {
    pub cmd: String,
    pub transport: GuestIoTransport,
    pub timeout_ms: u64,
}

impl GuestExecRequest {
    /// Build a serial/stdio exec request after [`validate_exec_cmd`].
    pub fn serial(cmd: impl Into<String>) -> Result<Self> {
        Self::try_new(cmd, GuestIoTransport::SerialStdio, DEFAULT_EXEC_TIMEOUT_MS)
    }

    /// Build using [`GuestIoTransport::preferred`] (vsock when CID/port env set).
    pub fn preferred(cmd: impl Into<String>) -> Result<Self> {
        Self::try_new(cmd, GuestIoTransport::preferred(), DEFAULT_EXEC_TIMEOUT_MS)
    }

    /// Build a request with an explicit transport + timeout.
    pub fn try_new(
        cmd: impl Into<String>,
        transport: GuestIoTransport,
        timeout_ms: u64,
    ) -> Result<Self> {
        let cmd = cmd.into();
        validate_exec_cmd(&cmd)?;
        if timeout_ms == 0 {
            return Err(PhenoError::BadRequest(
                "guest exec timeout_ms must be >= 1".into(),
            ));
        }
        Ok(Self {
            cmd,
            transport,
            timeout_ms,
        })
    }

    /// Bytes to write to guest serial stdin (`cmd` + trailing newline).
    ///
    /// Vsock requests must use [`Self::vsock_frame`] — this fails loud for
    /// [`GuestIoTransport::Vsock`] so callers cannot confuse transports.
    pub fn serial_stdin_frame(&self) -> Result<Vec<u8>> {
        match self.transport {
            GuestIoTransport::SerialStdio => Ok(serial_stdin_frame(&self.cmd)),
            GuestIoTransport::Vsock { guest_cid, port } => Err(guest_io_unavailable(format!(
                "serial_stdin_frame is serial-only (cid={guest_cid}, port={port}); \
                 use GuestExecRequest::vsock_frame for NDJSON vsock wire bytes"
            ))),
        }
    }

    /// NDJSON vsock request frame (trailing newline). Serial transport fails loud.
    pub fn vsock_frame(&self) -> Result<Vec<u8>> {
        match self.transport {
            GuestIoTransport::Vsock { .. } => vsock::vsock_exec_request_frame(&self.cmd),
            GuestIoTransport::SerialStdio => Err(guest_io_unavailable(
                "vsock_frame is vsock-only; use serial_stdin_frame for serial-stdio",
            )),
        }
    }
}

/// Pure serial frame builder (cmd already validated by caller / request).
pub fn serial_stdin_frame(cmd: &str) -> Vec<u8> {
    let mut frame = Vec::with_capacity(cmd.len() + 1);
    frame.extend_from_slice(cmd.as_bytes());
    frame.push(b'\n');
    frame
}

/// Documented vsock connect tuple — see [`vsock::vsock_connect_spec`].
pub fn vsock_connect_spec(guest_cid: u32, port: u32) -> Result<(u32, u32)> {
    vsock::vsock_connect_spec(guest_cid, port)
}

/// `true` when live destructive guest serial exec I/O is env-gated on.
pub fn exec_integration_enabled() -> bool {
    std::env::var(EXEC_INTEGRATION_ENV).ok().as_deref() == Some("1")
}

/// Fail-loud when no live guest child is held (hermetic start / stopped).
pub fn guest_not_running(client: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_GUEST_NOT_RUNNING,
        format!(
            "{client}::exec requires a live guest child from boot \
             (hermetic start alone is not enough; set \
             {}=1 / FIRECRACKER_INTEGRATION=1 / NANOVM_INTEGRATION=1, \
             then start(); docs/EXTRACTION_PLAN.md; do not unarchive \
             KDesktopVirt routinely)",
            crate::unikernel::boot::BOOT_INTEGRATION_ENV
        ),
    )
}

/// Fail-loud when transport / env gate blocks I/O.
pub fn guest_io_unavailable(detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::SANDBOX_GUEST_IO_UNAVAILABLE,
        format!(
            "guest exec I/O unavailable — {detail} (serial: set {EXEC_INTEGRATION_ENV}=1; \
             vsock: feature `sandbox-vsock` + Linux + {}=1; \
             docs/reference/unikernel-vsock-protocol.md)",
            vsock::VSOCK_INTEGRATION_ENV
        ),
    )
}

/// Require a live guest child for exec.
pub fn require_live_guest<'a>(
    client: &str,
    guest: &'a mut Option<Child>,
) -> Result<&'a mut Child> {
    guest.as_mut().ok_or_else(|| guest_not_running(client))
}

/// Execute `cmd` on a live guest via the requested transport.
///
/// - **Serial:** writes a line to child stdin; reads stdout until timeout /
///   first newline / EOF. Requires [`exec_integration_enabled`].
/// - **Vsock:** NDJSON framed round-trip via AF_VSOCK. Requires
///   [`vsock::vsock_integration_enabled`] + Linux + `sandbox-vsock`. Still
///   requires a live guest child (hypervisor process held by the client).
pub fn exec_on_guest(
    client: &str,
    guest: &mut Option<Child>,
    req: &GuestExecRequest,
) -> Result<String> {
    let _child = require_live_guest(client, guest)?;
    match req.transport {
        GuestIoTransport::SerialStdio => {
            if !exec_integration_enabled() {
                return Err(guest_io_unavailable(format!(
                    "live serial exec gated off; set {EXEC_INTEGRATION_ENV}=1 \
                     (destructive guest console I/O)"
                )));
            }
            exec_via_serial_stdio(_child, &req.cmd, req.timeout_ms)
        }
        GuestIoTransport::Vsock { guest_cid, port } => {
            vsock::exec_via_vsock(guest_cid, port, &req.cmd, req.timeout_ms)
        }
    }
}

/// Convenience: validate + preferred transport + [`exec_on_guest`].
pub fn exec_cmd_on_guest(client: &str, guest: &mut Option<Child>, cmd: &str) -> Result<String> {
    let req = GuestExecRequest::preferred(cmd)?;
    exec_on_guest(client, guest, &req)
}

fn exec_via_serial_stdio(child: &mut Child, cmd: &str, timeout_ms: u64) -> Result<String> {
    let frame = serial_stdin_frame(cmd);
    {
        let stdin = child.stdin.as_mut().ok_or_else(|| {
            guest_io_unavailable(
                "guest child stdin not piped — spawn via unikernel::boot \
                 (serial-stdio transport)",
            )
        })?;
        stdin.write_all(&frame).map_err(|e| {
            guest_io_unavailable(format!("serial stdin write failed: {e}"))
        })?;
        stdin.flush().map_err(|e| {
            guest_io_unavailable(format!("serial stdin flush failed: {e}"))
        })?;
    }

    // Take stdout for a bounded reader thread so a hung guest cannot block
    // the caller forever. Subsequent execs re-take if the guest still lives.
    let stdout = child.stdout.take().ok_or_else(|| {
        guest_io_unavailable(
            "guest child stdout not piped (or already consumed) — spawn via \
             unikernel::boot (serial-stdio transport)",
        )
    })?;
    let output = read_stdout_with_timeout(stdout, Duration::from_millis(timeout_ms))?;
    // Restore handle for follow-up execs when the reader thread returns it.
    child.stdout = output.stdout;
    Ok(output.text)
}

struct StdoutRead {
    text: String,
    stdout: Option<ChildStdout>,
}

fn read_stdout_with_timeout(mut stdout: ChildStdout, timeout: Duration) -> Result<StdoutRead> {
    let (tx, rx) = mpsc::channel::<(Vec<u8>, ChildStdout)>();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut out = Vec::new();
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    if out.contains(&b'\n') {
                        break;
                    }
                    // Cap runaway console spam.
                    if out.len() >= 64 * 1024 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send((out, stdout));
    });

    match rx.recv_timeout(timeout) {
        Ok((bytes, stdout)) => Ok(StdoutRead {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            stdout: Some(stdout),
        }),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Reader thread still owns stdout; do not claim success.
            Err(guest_io_unavailable(format!(
                "serial stdout read timed out after {}ms",
                timeout.as_millis()
            )))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(guest_io_unavailable(
            "serial stdout reader thread disconnected",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn serial_stdin_frame_appends_newline() {
        assert_eq!(serial_stdin_frame("uname -a"), b"uname -a\n");
    }

    #[test]
    fn guest_exec_request_serial_shape() {
        let req = GuestExecRequest::serial("echo hi").expect("req");
        assert_eq!(req.transport, GuestIoTransport::SerialStdio);
        assert_eq!(req.serial_stdin_frame().unwrap(), b"echo hi\n");
        assert_eq!(req.timeout_ms, DEFAULT_EXEC_TIMEOUT_MS);
    }

    #[test]
    fn guest_exec_request_rejects_injection() {
        let err = GuestExecRequest::serial("echo hi; rm -rf /").unwrap_err();
        assert!(matches!(err, PhenoError::Forbidden(_)));
    }

    #[test]
    fn vsock_connect_spec_fail_loud_off_platform() {
        let err_or_ok = vsock_connect_spec(3, 52);
        #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
        {
            assert_eq!(err_or_ok.unwrap(), (3, 52));
        }
        #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
        {
            let err = err_or_ok.unwrap_err();
            assert_eq!(
                err.unsupported_code(),
                Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
            );
        }
    }

    #[test]
    fn vsock_request_builds_ndjson_frame() {
        let req = GuestExecRequest::try_new(
            "uname",
            GuestIoTransport::Vsock {
                guest_cid: 3,
                port: 52,
            },
            1000,
        )
        .unwrap();
        let err = req.serial_stdin_frame().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
        );
        let frame = req.vsock_frame().unwrap();
        assert!(frame.ends_with(b"\n"));
        assert!(frame.starts_with(b"{"));
    }

    #[test]
    fn require_live_guest_none_fail_loud() {
        let mut guest: Option<Child> = None;
        let err = require_live_guest("TestClient", &mut guest).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_GUEST_NOT_RUNNING)
        );
    }

    #[test]
    fn exec_on_guest_without_child_fail_loud() {
        let mut guest = None;
        let req = GuestExecRequest::serial("pwd").unwrap();
        let err = exec_on_guest("TestClient", &mut guest, &req).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_GUEST_NOT_RUNNING)
        );
    }

    #[test]
    fn exec_integration_env_gate() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(EXEC_INTEGRATION_ENV);
        assert!(!exec_integration_enabled());
        std::env::set_var(EXEC_INTEGRATION_ENV, "1");
        assert!(exec_integration_enabled());
        std::env::remove_var(EXEC_INTEGRATION_ENV);
    }

    #[test]
    fn preferred_transport_serial_by_default() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(vsock::VSOCK_CID_ENV);
        std::env::remove_var(vsock::VSOCK_PORT_ENV);
        assert_eq!(
            GuestIoTransport::preferred(),
            GuestIoTransport::SerialStdio
        );
        assert_eq!(GuestIoTransport::SerialStdio.as_str(), "serial-stdio");
        std::env::set_var(vsock::VSOCK_CID_ENV, "3");
        std::env::set_var(vsock::VSOCK_PORT_ENV, "5252");
        assert_eq!(
            GuestIoTransport::preferred(),
            GuestIoTransport::Vsock {
                guest_cid: 3,
                port: 5252
            }
        );
        std::env::remove_var(vsock::VSOCK_CID_ENV);
        std::env::remove_var(vsock::VSOCK_PORT_ENV);
    }
}
