//! Real Linux desktop driver (X11 XTEST + GetImage; pure Wayland portal).
//!
//! Compiled only with `cfg(all(target_os = "linux", feature = "desktop-linux"))`.
//! wraps: x11rb 0.14 — https://crates.io/crates/x11rb
//! wraps: ashpd 0.11 — https://crates.io/crates/ashpd (pure Wayland Screenshot
//! + RemoteDesktop/Screencast inject)
//!
//! # Session support
//!
//! | Session | Path |
//! |---|---|
//! | `DISPLAY` set (X11 / XWayland) | X11 XTEST + GetImage BMP |
//! | Pure Wayland (`WAYLAND_DISPLAY` only) | portal Screenshot + RemoteDesktop/Screencast absolute pointer/keysyms |

mod keyboard;
mod mouse;

use std::fs::File;
use std::io::Write;
use std::path::Path;

use eidolon_core::error::PhenoError;
use eidolon_core::input::{PointerInput, TextInput};
use eidolon_core::traits::DesktopAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};
use keyboard::{chord_ctrl, inject_text, tap_key, XK_CONTROL_L};
use mouse::{button_detail, move_absolute};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::{
    ConnectionExt as XprotoExt, ImageFormat, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT,
};
use x11rb::protocol::xtest::{self, ConnectionExt as XtestExt};
use x11rb::rust_connection::RustConnection;

use crate::codes;
use crate::linux::{wayland, wayland_input};
use crate::windows::require_actions_allowed;

/// Native Linux desktop automation client (X11 + pure Wayland portal).
///
/// Destructive methods require `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.
#[derive(Debug, Default, Clone)]
pub struct LinuxClient;

/// Session backend selected from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    /// Native X11 or XWayland (`DISPLAY` set; preferred when both are set).
    X11,
    /// Pure Wayland (`WAYLAND_DISPLAY` set, `DISPLAY` unset).
    Wayland,
}

const XK_DELETE: u32 = 0xffff;
const XK_A: u32 = 0x0061;
const XK_V: u32 = 0x0076;

impl LinuxClient {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    /// Detect X11 vs pure Wayland from the environment.
    pub(crate) fn detect_session() -> Result<SessionKind> {
        if std::env::var_os("DISPLAY").is_some() {
            return Ok(SessionKind::X11);
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return Ok(SessionKind::Wayland);
        }
        Err(PhenoError::Platform(
            "no DISPLAY (X11/XWayland) or WAYLAND_DISPLAY in environment".into(),
        ))
    }

    /// Require an X11/`DISPLAY` session (used by X11 connection helpers).
    pub(crate) fn require_x11_session() -> Result<()> {
        match Self::detect_session()? {
            SessionKind::X11 => Ok(()),
            SessionKind::Wayland => Err(PhenoError::unsupported_platform(
                codes::DESKTOP_LINUX_WAYLAND_UNSUPPORTED,
                "internal: X11 helper called on pure Wayland session — use \
                 portal path (wayland module) instead",
            )),
        }
    }

    /// Open an X11 connection for read-only paths (viewport / GetImage).
    /// Does **not** require XTEST — capture works when input injection is unavailable.
    fn with_conn<F, T>(f: F) -> Result<T>
    where
        F: FnOnce(&RustConnection, usize) -> Result<T>,
    {
        Self::require_x11_session()?;
        let (conn, screen_num) = RustConnection::connect(None)
            .map_err(|e| PhenoError::Platform(format!("X11 connect failed: {e}")))?;
        f(&conn, screen_num)
    }

    /// Open an X11 connection and require XTEST for pointer/keyboard injection.
    fn with_xtest_conn<F, T>(f: F) -> Result<T>
    where
        F: FnOnce(&RustConnection, usize) -> Result<T>,
    {
        Self::with_conn(|conn, screen_num| {
            let _ = conn
                .extension_information(xtest::X11_EXTENSION_NAME)
                .map_err(|e| PhenoError::Platform(format!("X11 extension query failed: {e}")))?
                .ok_or_else(|| {
                    PhenoError::Platform(
                        "XTEST extension missing — cannot inject pointer/keyboard".into(),
                    )
                })?;
            conn.xtest_get_version(2, 1)
                .map_err(|e| PhenoError::Platform(format!("XTEST GetVersion: {e}")))?
                .reply()
                .map_err(|e| PhenoError::Platform(format!("XTEST GetVersion reply: {e}")))?;
            f(conn, screen_num)
        })
    }

    /// Root-window `GetImage` → Windows BMP (BGRA) at `path`.
    fn capture_bmp(conn: &RustConnection, screen_num: usize, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                return Err(PhenoError::Platform(format!(
                    "screenshot parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }

        let (width, height) = screen_size(conn, screen_num)?;
        let root = root_window(conn, screen_num)?;
        let reply = conn
            .get_image(ImageFormat::Z_PIXMAP, root, 0, 0, width, height, !0u32)
            .map_err(|e| PhenoError::Platform(format!("GetImage: {e}")))?
            .reply()
            .map_err(|e| PhenoError::Platform(format!("GetImage reply: {e}")))?;

        let depth = reply.depth;
        let data = reply.data;
        let bgra = zpixmap_to_bgra(width as u32, height as u32, depth, &data)?;
        write_bmp32(path, width as u32, height as u32, &bgra)?;
        log::info!("Screenshot (X11 GetImage BMP) saved to {}", path.display());
        Ok(())
    }
}

fn screen_size(conn: &RustConnection, screen_num: usize) -> Result<(u16, u16)> {
    let screen = conn
        .setup()
        .roots
        .get(screen_num)
        .ok_or_else(|| PhenoError::Platform("X11 screen index out of range".into()))?;
    let width = screen.width_in_pixels;
    let height = screen.height_in_pixels;
    if width == 0 || height == 0 {
        return Err(PhenoError::Platform(format!(
            "X11 screen size invalid: {width}x{height}"
        )));
    }
    Ok((width, height))
}

fn root_window(conn: &RustConnection, screen_num: usize) -> Result<u32> {
    let screen = conn
        .setup()
        .roots
        .get(screen_num)
        .ok_or_else(|| PhenoError::Platform("X11 screen index out of range".into()))?;
    Ok(screen.root)
}

fn fake_input(
    conn: &RustConnection,
    type_: u8,
    detail: u8,
    root: u32,
    root_x: i16,
    root_y: i16,
) -> Result<()> {
    conn.xtest_fake_input(type_, detail, 0, root, root_x, root_y, 0)
        .map_err(|e| PhenoError::Platform(format!("XTEST FakeInput: {e}")))?
        .check()
        .map_err(|e| PhenoError::Platform(format!("XTEST FakeInput check: {e}")))?;
    Ok(())
}

/// Convert ZPixmap bytes to packed BGRA for BMP.
fn zpixmap_to_bgra(width: u32, height: u32, depth: u8, data: &[u8]) -> Result<Vec<u8>> {
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| PhenoError::Platform("screenshot size overflow".into()))?;
    let mut out = vec![0u8; pixels.saturating_mul(4)];

    match depth {
        24 | 32 => {
            let stride = if data.len() >= pixels * 4 {
                4
            } else if data.len() >= pixels * 3 {
                3
            } else {
                return Err(PhenoError::Platform(format!(
                    "GetImage buffer too small for {width}x{height} depth={depth} ({} bytes)",
                    data.len()
                )));
            };
            for i in 0..pixels {
                let src = i * stride;
                let b = data[src];
                let g = data[src + 1];
                let r = data[src + 2];
                let dst = i * 4;
                out[dst] = b;
                out[dst + 1] = g;
                out[dst + 2] = r;
                out[dst + 3] = 0xff;
            }
        }
        16 => {
            if data.len() < pixels * 2 {
                return Err(PhenoError::Platform(
                    "GetImage buffer too small for 16-bit ZPixmap".into(),
                ));
            }
            for i in 0..pixels {
                let src = i * 2;
                let pix = u16::from_le_bytes([data[src], data[src + 1]]);
                let r = ((pix >> 11) & 0x1f) as u8;
                let g = ((pix >> 5) & 0x3f) as u8;
                let b = (pix & 0x1f) as u8;
                let dst = i * 4;
                out[dst] = (b << 3) | (b >> 2);
                out[dst + 1] = (g << 2) | (g >> 4);
                out[dst + 2] = (r << 3) | (r >> 2);
                out[dst + 3] = 0xff;
            }
        }
        other => {
            return Err(PhenoError::Platform(format!(
                "unsupported X11 root depth {other} for screenshot (need 16/24/32)"
            )));
        }
    }
    Ok(out)
}

fn write_bmp32(path: &Path, width: u32, height: u32, bgra: &[u8]) -> Result<()> {
    let row_stride = width.saturating_mul(4);
    let pixel_bytes = row_stride.saturating_mul(height);
    if bgra.len() < pixel_bytes as usize {
        return Err(PhenoError::Platform("BMP pixel buffer too small".into()));
    }

    let file_header_size = 14u32;
    let info_header_size = 40u32;
    let offset = file_header_size + info_header_size;
    let file_size = offset + pixel_bytes;

    let mut file = File::create(path).map_err(|e| {
        PhenoError::Platform(format!(
            "failed to create screenshot {}: {e}",
            path.display()
        ))
    })?;

    file.write_all(b"BM").map_err(io_platform)?;
    file.write_all(&file_size.to_le_bytes())
        .map_err(io_platform)?;
    file.write_all(&0u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&offset.to_le_bytes()).map_err(io_platform)?;

    file.write_all(&info_header_size.to_le_bytes())
        .map_err(io_platform)?;
    file.write_all(&(width as i32).to_le_bytes())
        .map_err(io_platform)?;
    file.write_all(&(-(height as i32)).to_le_bytes())
        .map_err(io_platform)?;
    file.write_all(&1u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&32u16.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u32.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&pixel_bytes.to_le_bytes())
        .map_err(io_platform)?;
    file.write_all(&0i32.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0i32.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u32.to_le_bytes()).map_err(io_platform)?;
    file.write_all(&0u32.to_le_bytes()).map_err(io_platform)?;

    file.write_all(&bgra[..pixel_bytes as usize])
        .map_err(io_platform)?;
    Ok(())
}

fn io_platform(e: std::io::Error) -> PhenoError {
    PhenoError::Platform(format!("screenshot I/O failed: {e}"))
}

#[async_trait::async_trait]
impl DesktopAutomator for LinuxClient {
    async fn get_viewport(&self) -> Result<Viewport> {
        match Self::detect_session()? {
            SessionKind::X11 => Self::with_conn(|conn, screen_num| {
                let (width, height) = screen_size(conn, screen_num)?;
                Ok(Viewport::new(u32::from(width), u32::from(height), 1.0))
            }),
            SessionKind::Wayland => wayland::viewport().await,
        }
    }

    async fn screenshot(&self, path: &str) -> Result<()> {
        require_actions_allowed("screenshot")?;
        match Self::detect_session()? {
            SessionKind::X11 => Self::with_conn(|conn, screen_num| {
                Self::capture_bmp(conn, screen_num, Path::new(path))
            }),
            SessionKind::Wayland => wayland::screenshot(path).await,
        }
    }

    async fn pointer(&self, event: &PointerInput) -> Result<()> {
        require_actions_allowed("pointer")?;
        if Self::detect_session()? == SessionKind::Wayland {
            return wayland_input::pointer(event).await;
        }
        Self::with_xtest_conn(|conn, screen_num| {
            let root = root_window(conn, screen_num)?;
            let detail = button_detail(&event.button);

            match event.action.as_str() {
                "move" => {
                    move_absolute(conn, screen_num, event.x, event.y)?;
                }
                "press" | "tap" => {
                    move_absolute(conn, screen_num, event.x, event.y)?;
                    fake_input(conn, BUTTON_PRESS_EVENT, detail, root, 0, 0)?;
                    if event.action == "tap" {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    fake_input(conn, BUTTON_RELEASE_EVENT, detail, root, 0, 0)?;
                }
                "release" => {
                    fake_input(conn, BUTTON_RELEASE_EVENT, detail, root, 0, 0)?;
                }
                "long_press" => {
                    move_absolute(conn, screen_num, event.x, event.y)?;
                    fake_input(conn, BUTTON_PRESS_EVENT, detail, root, 0, 0)?;
                    let duration = event.duration_ms.unwrap_or(500);
                    std::thread::sleep(std::time::Duration::from_millis(duration as u64));
                    fake_input(conn, BUTTON_RELEASE_EVENT, detail, root, 0, 0)?;
                }
                "drag" => {
                    fake_input(conn, BUTTON_PRESS_EVENT, detail, root, 0, 0)?;
                    move_absolute(conn, screen_num, event.x, event.y)?;
                    fake_input(conn, BUTTON_RELEASE_EVENT, detail, root, 0, 0)?;
                }
                other => {
                    log::warn!("Unknown pointer action: {other}");
                }
            }

            log::debug!(
                "Pointer event executed (X11): ({}, {}) action={}",
                event.x,
                event.y,
                event.action
            );
            Ok(())
        })
    }

    async fn text(&self, event: &TextInput) -> Result<()> {
        require_actions_allowed("text")?;
        if Self::detect_session()? == SessionKind::Wayland {
            return wayland_input::text(event).await;
        }
        let delay = event.delay_ms.unwrap_or(10);
        Self::with_xtest_conn(|conn, screen_num| {
            let root = root_window(conn, screen_num)?;
            let map = keyboard::build_keysym_map(conn)?;

            match event.input_type.as_str() {
                "keystroke" => {
                    inject_text(conn, screen_num, &event.text, delay)?;
                }
                "paste" => {
                    chord_ctrl(conn, root, &map, XK_V)?;
                }
                "clear" => {
                    chord_ctrl(conn, root, &map, XK_A)?;
                    tap_key(conn, root, &map, XK_DELETE)?;
                }
                other => {
                    log::warn!("Unknown text input type: {other}");
                }
            }

            log::debug!(
                "Text input executed (X11): type={} text={}",
                event.input_type,
                event.text
            );
            Ok(())
        })
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event (linux): {:?}", event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traces to: FR-EIDOLON-001
    #[test]
    fn detect_session_prefers_display_over_wayland() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        assert_eq!(
            LinuxClient::detect_session().expect("session"),
            SessionKind::X11
        );
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn detect_session_wayland_when_display_unset() {
        if std::env::var_os("DISPLAY").is_some() {
            return;
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return;
        }
        assert_eq!(
            LinuxClient::detect_session().expect("session"),
            SessionKind::Wayland
        );
        let err = LinuxClient::require_x11_session().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_WAYLAND_UNSUPPORTED)
        );
    }

    #[test]
    fn zpixmap_24_padded_to_bgra() {
        let data = vec![1u8, 2, 3, 0];
        let out = zpixmap_to_bgra(1, 1, 24, &data).expect("convert");
        assert_eq!(out, vec![1, 2, 3, 0xff]);
    }

    #[test]
    fn write_bmp32_uses_top_down_negative_height() {
        let dir = std::env::temp_dir().join("eidolon-linux-bmp-topdown");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("topdown.bmp");
        let pixels = vec![0u8; 2 * 2 * 4];
        write_bmp32(&path, 2, 2, &pixels).expect("write");
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(&bytes[0..2], b"BM");
        let bi_height = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
        assert_eq!(bi_height, -2, "top-down BMP requires negative biHeight");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn client_constructs() {
        let _ = LinuxClient::new().expect("construct");
    }
}
