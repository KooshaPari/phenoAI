//! CGCapture adapter — macOS screen capture via xcap (CoreGraphics-backed).
//! Implements CapturePort for macOS; delegates to XcapCapture.

use crate::adapters::xcap::XcapCapture;
use crate::domain::capture::{CaptureError, Frame};
use crate::ports::CapturePort;
use async_trait::async_trait;
use tracing::instrument;

pub struct CGCapture {
    inner: XcapCapture,
}

impl CGCapture {
    pub fn new() -> Self {
        Self {
            inner: XcapCapture::new(),
        }
    }
}

impl Default for CGCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CapturePort for CGCapture {
    #[instrument(name = "cg.capture_display", skip(self))]
    async fn capture_display(&self, monitor: u32) -> Result<Frame, CaptureError> {
        self.inner.capture_display(monitor).await
    }

    #[instrument(name = "cg.capture_window", skip(self))]
    async fn capture_window(&self, title: Option<&str>) -> Result<Frame, CaptureError> {
        self.inner.capture_window(title).await
    }
}
