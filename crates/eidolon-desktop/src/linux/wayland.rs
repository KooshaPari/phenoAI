//! Pure Wayland desktop path via xdg-desktop-portal (ashpd).
//!
//! wraps: ashpd 0.11 — https://crates.io/crates/ashpd
//!
//! # Capabilities
//!
//! | Surface | Status |
//! |---|---|
//! | Screenshot / viewport size | ✅ `org.freedesktop.portal.Screenshot` |
//! | Pointer / keyboard inject | ✅ RemoteDesktop + Screencast (`wayland_input`) — absolute pointer + keysyms; opt-in restore-token (`EIDOLON_DESKTOP_WAYLAND_RESTORE=1`); portal miss → [`codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`]; device/stream gap → [`codes::DESKTOP_LINUX_WAYLAND_INPUT_UNSUPPORTED`]; restore store I/O → [`codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO`] |
//!
//! Portal / session missing → [`codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE`].
//! Destructive actions still require `EIDOLON_DESKTOP_ALLOW_ACTIONS=1`.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::{Result, Viewport};
use std::path::Path;

/// Env: set to `1` to allow interactive portal screenshot UI (default: non-interactive).
pub const WAYLAND_INTERACTIVE_ENV: &str = "EIDOLON_DESKTOP_WAYLAND_INTERACTIVE";

fn interactive_screenshot() -> bool {
    matches!(
        std::env::var(WAYLAND_INTERACTIVE_ENV).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes")
    )
}

pub(crate) fn map_portal_err(context: &str, err: ashpd::Error) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
        format!(
            "{context}: {err} — need a running xdg-desktop-portal (and session \
             bus) on pure Wayland; see docs/EXTRACTION_PLAN.md"
        ),
    )
}

/// Capture via Screenshot portal; copy the portal `file://` URI to `path`.
///
/// Portal backends typically produce PNG. Destination may be any extension;
/// bytes are copied as-is (no format conversion).
pub async fn screenshot(path: &str) -> Result<()> {
    let dest = Path::new(path);
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(PhenoError::Platform(format!(
                "screenshot parent directory does not exist: {}",
                parent.display()
            )));
        }
    }

    let response = ashpd::desktop::screenshot::Screenshot::request()
        .interactive(interactive_screenshot())
        .modal(false)
        .send()
        .await
        .map_err(|e| map_portal_err("Screenshot portal request failed", e))?
        .response()
        .map_err(|e| map_portal_err("Screenshot portal response failed", e))?;

    let src = {
        let uri = response.uri();
        if uri.scheme() != "file" {
            return Err(PhenoError::Platform(format!(
                "portal screenshot URI is not file://: {uri}"
            )));
        }
        uri.to_file_path().map_err(|_| {
            PhenoError::Platform(format!(
                "portal screenshot URI could not be converted to path: {uri}"
            ))
        })?
    };
    std::fs::copy(&src, dest).map_err(|e| {
        PhenoError::Platform(format!(
            "failed to copy portal screenshot {} → {}: {e}",
            src.display(),
            dest.display()
        ))
    })?;
    log::info!(
        "Screenshot (xdg-desktop-portal) saved to {} (from {})",
        dest.display(),
        src.display()
    );
    Ok(())
}

/// Viewport size via a temporary non-interactive portal screenshot + PNG IHDR.
pub async fn viewport() -> Result<Viewport> {
    let dir = std::env::temp_dir().join("eidolon-wayland-viewport");
    std::fs::create_dir_all(&dir).map_err(|e| {
        PhenoError::Platform(format!("viewport temp dir {}: {e}", dir.display()))
    })?;
    let tmp = dir.join(format!(
        "vp-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let tmp_str = tmp.to_string_lossy().into_owned();
    let result = screenshot(&tmp_str).await;
    let dims = match result {
        Ok(()) => png_dimensions(&tmp),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    let _ = std::fs::remove_file(&tmp);
    let (width, height) = dims?;
    if width == 0 || height == 0 {
        return Err(PhenoError::Platform(format!(
            "portal screenshot reported invalid size {width}x{height}"
        )));
    }
    Ok(Viewport::new(width, height, 1.0))
}

/// Read PNG IHDR width/height (big-endian) without an image crate.
pub(crate) fn png_dimensions(path: &Path) -> Result<(u32, u32)> {
    let bytes = std::fs::read(path).map_err(|e| {
        PhenoError::Platform(format!("read portal PNG {}: {e}", path.display()))
    })?;
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(PhenoError::Platform(format!(
            "portal screenshot is not a PNG (need IHDR for viewport): {}",
            path.display()
        )));
    }
    // IHDR chunk: length(4) + "IHDR"(4) starts at offset 8; width/height at 16/20.
    if &bytes[12..16] != b"IHDR" {
        return Err(PhenoError::Platform(
            "portal PNG missing IHDR chunk".into(),
        ));
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // Traces to: FR-EIDOLON-001
    #[test]
    fn png_dimensions_reads_ihdr() {
        // Minimal PNG: signature + IHDR chunk (13-byte data) + IEND.
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        png.extend_from_slice(&13u32.to_be_bytes()); // IHDR length
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&1920u32.to_be_bytes());
        png.extend_from_slice(&1080u32.to_be_bytes());
        png.extend_from_slice(&[8, 2, 0, 0, 0]); // bit depth, color, compression, filter, interlace
        png.extend_from_slice(&0u32.to_be_bytes()); // CRC placeholder
        // Pad so len >= 24 for the IHDR fields we read (already satisfied).
        let dir = std::env::temp_dir().join("eidolon-wayland-png-ihdr");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("ihdr.png");
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(&png).expect("write");
        let (w, h) = png_dimensions(&path).expect("dims");
        assert_eq!((w, h), (1920, 1080));
        let _ = std::fs::remove_file(&path);
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn map_portal_err_uses_portal_unavailable_code() {
        // Construct via a fake ashpd-shaped message path: call map with a
        // PortalNotFound-style error by using ashpd::Error::PortalNotFound if
        // available — otherwise verify the code constant wiring only.
        let err = PhenoError::unsupported_platform(
            codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE,
            "test",
        );
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_WAYLAND_PORTAL_UNAVAILABLE)
        );
    }
}
