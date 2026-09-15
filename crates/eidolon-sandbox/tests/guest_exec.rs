//! Guest exec / serial + vsock I/O integration tests (macOS-safe by default).
//!
//! Live destructive serial writes require `UNIKERNEL_EXEC_INTEGRATION=1`.
//! Live AF_VSOCK requires Linux + `sandbox-vsock` + `UNIKERNEL_VSOCK_INTEGRATION=1`.

use eidolon_core::error::PhenoError;
use eidolon_sandbox::codes;
use eidolon_sandbox::unikernel_exec::{
    exec_on_guest, require_live_guest, serial_stdin_frame, vsock_connect_spec, GuestExecRequest,
    GuestIoTransport, EXEC_INTEGRATION_ENV,
};
use eidolon_sandbox::unikernel_vsock::{
    parse_vsock_exec_response, vsock_exec_request_frame, vsock_integration_enabled,
    vsock_response_text, VSOCK_INTEGRATION_ENV,
};
use std::process::Child;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn no_guest_fail_loud() {
    let mut guest: Option<Child> = None;
    let err = require_live_guest("GuestExecIt", &mut guest).unwrap_err();
    assert_eq!(
        err.unsupported_code(),
        Some(codes::SANDBOX_GUEST_NOT_RUNNING)
    );
    assert_eq!(err.status_code(), 501);
}

#[test]
fn argv_transport_builders() {
    assert_eq!(serial_stdin_frame("ls -la"), b"ls -la\n");
    let req = GuestExecRequest::serial("cat /etc/hostname").expect("serial req");
    assert_eq!(req.transport, GuestIoTransport::SerialStdio);
    assert_eq!(req.transport.as_str(), "serial-stdio");
    assert_eq!(req.serial_stdin_frame().unwrap(), b"cat /etc/hostname\n");

    let vsock = GuestExecRequest::try_new(
        "uname",
        GuestIoTransport::Vsock {
            guest_cid: 3,
            port: 52,
        },
        500,
    )
    .expect("vsock req builds");
    let err = vsock.serial_stdin_frame().unwrap_err();
    assert_eq!(
        err.unsupported_code(),
        Some(codes::SANDBOX_GUEST_IO_UNAVAILABLE)
    );
    let frame = vsock.vsock_frame().expect("ndjson frame");
    assert!(frame.ends_with(b"\n"));
    assert!(std::str::from_utf8(&frame).unwrap().contains("\"op\":\"exec\""));

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
fn vsock_ndjson_roundtrip_parse() {
    let req = vsock_exec_request_frame("pwd").unwrap();
    assert!(req.ends_with(b"\n"));
    let resp = parse_vsock_exec_response(
        br#"{"v":1,"op":"exec_result","ok":true,"stdout":"/\n","stderr":"","exit_code":0}"#,
    )
    .unwrap();
    assert_eq!(vsock_response_text(&resp).unwrap(), "/\n");
}

#[test]
fn exec_rejects_shell_injection_before_transport() {
    let err = GuestExecRequest::serial("echo a && echo b").unwrap_err();
    assert!(matches!(err, PhenoError::Forbidden(_)));
}

#[test]
fn exec_without_child_prefers_not_running_over_io() {
    let mut guest = None;
    let req = GuestExecRequest::serial("pwd").unwrap();
    let err = exec_on_guest("GuestExecIt", &mut guest, &req).unwrap_err();
    assert_eq!(
        err.unsupported_code(),
        Some(codes::SANDBOX_GUEST_NOT_RUNNING)
    );
}

#[test]
fn exec_integration_env_documented() {
    let _g = ENV_LOCK.lock().unwrap();
    std::env::remove_var(EXEC_INTEGRATION_ENV);
    assert_eq!(EXEC_INTEGRATION_ENV, "UNIKERNEL_EXEC_INTEGRATION");
    assert!(!eidolon_sandbox::unikernel_exec::exec_integration_enabled());
    std::env::remove_var(VSOCK_INTEGRATION_ENV);
    assert_eq!(VSOCK_INTEGRATION_ENV, "UNIKERNEL_VSOCK_INTEGRATION");
    assert!(!vsock_integration_enabled());
}
