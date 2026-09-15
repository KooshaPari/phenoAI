//! AF_VSOCK stream connection + timeout read helpers.
//!
//! Split from `vsock.rs` to keep the connection I/O layer focused.
//! Live AF_VSOCK requires feature `sandbox-vsock` **and** Linux.

use std::io::{Read, Write};
use std::time::Duration;

use eidolon_core::Result;

use super::{guest_io_unavailable, MAX_VSOCK_RESPONSE_BYTES};

/// Opaque vsock stream handle (`Read` + `Write`).
pub struct VsockStream {
    #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
    pub(super) inner: std::net::TcpStream,
    #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
    _priv: (),
}

impl Read for VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
        {
            self.inner.read(buf)
        }
        #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
        {
            let _ = buf;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "vsock Read unavailable on this platform/feature set",
            ))
        }
    }
}

impl Write for VsockStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
        {
            self.inner.write(buf)
        }
        #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
        {
            let _ = buf;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "vsock Write unavailable on this platform/feature set",
            ))
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
        {
            self.inner.flush()
        }
        #[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "vsock flush unavailable on this platform/feature set",
            ))
        }
    }
}

/// Open an AF_VSOCK stream to `(guest_cid, port)` (Linux + feature only).
pub fn connect_guest_vsock(guest_cid: u32, port: u32) -> Result<VsockStream> {
    let (cid, port) = super::vsock_connect_spec(guest_cid, port)?;
    connect_guest_vsock_inner(cid, port)
}

#[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
fn connect_guest_vsock_inner(guest_cid: u32, port: u32) -> Result<VsockStream> {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::io::AsRawFd;

    // wraps: libc AF_VSOCK / sockaddr_vm (Linux vm_sockets)
    let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(guest_io_unavailable(format!(
            "vsock socket(AF_VSOCK) failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };

    let mut addr: libc::sockaddr_vm = unsafe { std::mem::zeroed() };
    addr.svm_family = libc::AF_VSOCK as libc::sa_family_t;
    addr.svm_cid = guest_cid;
    addr.svm_port = port;

    let rc = unsafe {
        libc::connect(
            owned.as_raw_fd(),
            &addr as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_vm>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(guest_io_unavailable(format!(
            "vsock connect(cid={guest_cid}, port={port}) failed: {} \
             (is the guest agent listening? Firecracker vsock configured? \
              set {VSOCK_INTEGRATION_ENV}=1 only with a live guest)",
            std::io::Error::last_os_error()
        )));
    }

    let stream = std::net::TcpStream::from(owned);
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| guest_io_unavailable(format!("vsock set_read_timeout: {e}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| guest_io_unavailable(format!("vsock set_write_timeout: {e}")))?;
    Ok(VsockStream { inner: stream })
}

#[cfg(not(all(feature = "sandbox-vsock", target_os = "linux")))]
fn connect_guest_vsock_inner(_guest_cid: u32, _port: u32) -> Result<VsockStream> {
    Err(super::vsock_platform_unavailable())
}

pub(super) fn read_line_with_timeout(
    stream: &mut VsockStream,
    timeout: Duration,
) -> Result<Vec<u8>> {
    // Apply deadline on the stream when possible, then read until newline.
    #[cfg(all(feature = "sandbox-vsock", target_os = "linux"))]
    {
        stream
            .inner
            .set_read_timeout(Some(timeout))
            .map_err(|e| guest_io_unavailable(format!("vsock set_read_timeout: {e}")))?;
    }
    let _ = timeout;

    let mut buf = [0u8; 4096];
    let mut out = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&buf[..n]);
                if out.contains(&b'\n') {
                    break;
                }
                if out.len() >= MAX_VSOCK_RESPONSE_BYTES {
                    break;
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Err(guest_io_unavailable(format!(
                    "vsock read timed out after {}ms",
                    timeout.as_millis()
                )));
            }
            Err(e) => {
                return Err(guest_io_unavailable(format!("vsock read failed: {e}")));
            }
        }
    }
    if out.is_empty() {
        return Err(guest_io_unavailable("vsock read returned EOF with no data"));
    }
    Ok(out)
}
