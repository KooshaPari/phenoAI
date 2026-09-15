//! X11Capture adapter — Linux screen capture using xcap (backed by x11rb).
//! Implements CapturePort for Linux; delegates to XcapCapture.

use crate::adapters::xcap::XcapCapture;
use crate::domain::capture::{CaptureError, Frame};
use crate::ports::CapturePort;
use async_trait::async_trait;
use tracing::instrument;

pub struct X11Capture {
    inner: XcapCapture,
}

impl X11Capture {
    pub fn new() -> Self {
        Self {
            inner: XcapCapture::new(),
        }
    }
}

impl Default for X11Capture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CapturePort for X11Capture {
    #[instrument(name = "x11.capture_display", skip(self))]
    async fn capture_display(&self, monitor: u32) -> Result<Frame, CaptureError> {
        self.inner.capture_display(monitor).await
    }

    #[instrument(name = "x11.capture_window", skip(self))]
    async fn capture_window(&self, title: Option<&str>) -> Result<Frame, CaptureError> {
        self.inner.capture_window(title).await
    }
}
