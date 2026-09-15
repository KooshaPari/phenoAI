//! XcapCapture adapter — cross-platform fallback capture using the xcap crate.
//! Implements CapturePort for all platforms when a primary adapter is unavailable.

use crate::domain::capture::{CaptureError, Frame};
use crate::ports::CapturePort;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
use tracing::instrument;

pub struct XcapCapture;

impl XcapCapture {
    pub fn new() -> Self {
        Self
    }
}

impl Default for XcapCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CapturePort for XcapCapture {
    #[instrument(name = "xcap.capture_display", skip(self))]
    async fn capture_display(&self, monitor: u32) -> Result<Frame, CaptureError> {
        let monitor_idx = monitor as usize;
        tokio::task::spawn_blocking(move || -> Result<Frame, CaptureError> {
            let monitors =
                xcap::Monitor::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
            let mon = monitors.into_iter().nth(monitor_idx).ok_or_else(|| {
                CaptureError::WindowNotFound(format!("monitor index {}", monitor_idx))
            })?;
            let img = mon
                .capture_image()
                .map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
            encode_xcap_image(img.width(), img.height(), img.into_raw())
        })
        .await
        .map_err(|e| CaptureError::CaptureFailed(format!("spawn_blocking panic: {e}")))?
    }

    #[instrument(name = "xcap.capture_window", skip(self))]
    #[allow(clippy::unnecessary_map_or)]
    async fn capture_window(&self, title: Option<&str>) -> Result<Frame, CaptureError> {
        let title_owned = title.map(|t| t.to_string());
        tokio::task::spawn_blocking(move || -> Result<Frame, CaptureError> {
            let title = title_owned.ok_or_else(|| {
                CaptureError::WindowNotFound("no title provided for window capture".to_string())
            })?;
            let windows =
                xcap::Window::all().map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
            let window = windows
                .into_iter()
                .find(|w| {
                    w.title()
                        .ok()
                        .map_or(false, |t| t.to_lowercase().contains(&title.to_lowercase()))
                })
                .ok_or_else(|| CaptureError::WindowNotFound(title.clone()))?;
            let img = window
                .capture_image()
                .map_err(|e| CaptureError::CaptureFailed(e.to_string()))?;
            encode_xcap_image(img.width(), img.height(), img.into_raw())
        })
        .await
        .map_err(|e| CaptureError::CaptureFailed(format!("spawn_blocking panic: {e}")))?
    }
}

fn encode_xcap_image(width: u32, height: u32, raw: Vec<u8>) -> Result<Frame, CaptureError> {
    encode_rgba_png_frame(width, height, &raw)
}

pub(crate) fn encode_rgba_png_frame(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<Frame, CaptureError> {
    let mut buf = Vec::new();
    PngEncoder::new(&mut buf)
        .write_image(rgba, width, height, ColorType::Rgba8.into())
        .map_err(|e| CaptureError::EncodeFailed(e.to_string()))?;
    let data = STANDARD.encode(&buf);
    Ok(Frame {
        data,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::encode_rgba_png_frame;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    #[test]
    fn encodes_rgba_pixels_as_png() {
        let rgba = [0_u8, 255, 0, 255];
        let frame = encode_rgba_png_frame(1, 1, &rgba).expect("png encoding should succeed");
        let bytes = STANDARD
            .decode(frame.data)
            .expect("frame data should be valid base64");

        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

        let image = image::load_from_memory(&bytes).expect("png should decode");
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
    }
}
