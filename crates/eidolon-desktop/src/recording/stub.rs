//! Fail-loud recording stub — default when `desktop-recording` is off or no FFmpeg.

use super::{DesktopRecorder, QualityProfile};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Fail-loud recording stub.
#[derive(Debug, Default, Clone)]
pub struct RecordingStub;

impl RecordingStub {
    pub fn new() -> Self {
        Self
    }

    fn unavailable(method: &str) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::DESKTOP_RECORDING_UNAVAILABLE,
            format!(
                "DesktopRecorder::{method} unavailable — enable feature \
                 `desktop-recording` and install system `ffmpeg` \
                 (docs/EXTRACTION_PLAN.md Phase E; do not unarchive KDesktopVirt)"
            ),
        )
    }
}

#[async_trait::async_trait]
impl DesktopRecorder for RecordingStub {
    async fn start(&self, _output_path: &str, _profile: QualityProfile) -> Result<()> {
        Err(Self::unavailable("start"))
    }

    async fn stop(&self) -> Result<()> {
        Err(Self::unavailable("stop"))
    }

    fn ffmpeg_ready(&self) -> bool {
        // Stub never claims readiness even if ffmpeg is on PATH — callers must
        // use FfmpegRecorder / RecordingPipeline behind `desktop-recording`.
        false
    }
}
