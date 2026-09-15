//! FFmpeg subprocess recorder (feature `desktop-recording`).

use super::{ffmpeg_capture_args, probe, validate_output_parent, DesktopRecorder, QualityProfile};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// FFmpeg subprocess recorder (feature `desktop-recording`).
///
/// Spawns system `ffmpeg` for screen capture. Does **not** bundle binaries.
/// Live capture often needs OS screen-recording permission; integration tests
/// must set `RECORDING_INTEGRATION=1`.
pub struct FfmpegRecorder {
    ffmpeg: PathBuf,
    child: Mutex<Option<Child>>,
}

impl FfmpegRecorder {
    /// Construct if `ffmpeg` is resolvable; otherwise fail-loud.
    pub fn try_new() -> Result<Self> {
        let Some(ffmpeg) = probe::resolve_ffmpeg() else {
            return Err(PhenoError::unsupported_platform(
                codes::DESKTOP_RECORDING_UNAVAILABLE,
                "FfmpegRecorder::try_new: system `ffmpeg` not found on PATH \
                 (set EIDOLON_FFMPEG or install ffmpeg; see docs/EXTRACTION_PLAN.md Phase E)",
            ));
        };
        Ok(Self {
            ffmpeg,
            child: Mutex::new(None),
        })
    }

    /// Path to the resolved FFmpeg binary.
    pub fn ffmpeg_path(&self) -> &Path {
        &self.ffmpeg
    }

    /// Whether a capture child is currently tracked.
    pub fn is_recording(&self) -> bool {
        self.child
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }

    fn unavailable(method: &str, detail: impl Into<String>) -> PhenoError {
        PhenoError::unsupported_platform(
            codes::DESKTOP_RECORDING_UNAVAILABLE,
            format!("FfmpegRecorder::{method}: {}", detail.into()),
        )
    }
}

#[async_trait::async_trait]
impl DesktopRecorder for FfmpegRecorder {
    async fn start(&self, output_path: &str, profile: QualityProfile) -> Result<()> {
        validate_output_parent(output_path)?;

        let mut guard = self.child.lock().map_err(|_| {
            PhenoError::Internal("FfmpegRecorder mutex poisoned".into())
        })?;
        if guard.is_some() {
            return Err(PhenoError::Platform(
                "recording already in progress; call stop() first".into(),
            ));
        }

        let args = ffmpeg_capture_args(output_path, profile);
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            Self::unavailable("start", format!("failed to spawn ffmpeg: {e}"))
        })?;

        log::info!(
            "FfmpegRecorder started → {} (preset={})",
            output_path,
            profile.x264_preset()
        );
        *guard = Some(child);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        let mut guard = self.child.lock().map_err(|_| {
            PhenoError::Internal("FfmpegRecorder mutex poisoned".into())
        })?;
        let Some(mut child) = guard.take() else {
            return Err(PhenoError::Platform(
                "no recording in progress".into(),
            ));
        };

        // Graceful quit: FFmpeg listens for 'q' on stdin.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"q");
            let _ = stdin.flush();
        }

        match child.wait() {
            Ok(status) => {
                if status.success() || status.code() == Some(0) {
                    log::info!("FfmpegRecorder stopped cleanly");
                    Ok(())
                } else {
                    // Non-zero is common when capture ends early / permission
                    // denied; surface as Platform so callers can retry.
                    Err(PhenoError::Platform(format!(
                        "ffmpeg exited with status {status}"
                    )))
                }
            }
            Err(e) => Err(PhenoError::Platform(format!(
                "failed waiting for ffmpeg: {e}"
            ))),
        }
    }

    fn ffmpeg_ready(&self) -> bool {
        true
    }
}
