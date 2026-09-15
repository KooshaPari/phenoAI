//! High-level recording pipeline: capture start/stop + file encode paths.
//!
//! Salvaged from archived KDesktopVirt `recording_pipeline` / `ffmpeg_pipeline`
//! patterns — without streaming, audio DSP, or bollard coupling.

use super::ffmpeg::FfmpegRecorder;
use super::profile::{
    ffmpeg_encode_args, ffmpeg_gif_args, ffmpeg_test_pattern_args, QualityProfile,
};
use super::{probe, validate_output_parent, DesktopRecorder};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Session metadata returned after a successful [`RecordingPipeline::stop`].
#[derive(Debug, Clone)]
pub struct RecordingResult {
    pub output_path: PathBuf,
    pub duration: Duration,
    pub file_size_bytes: u64,
    pub success: bool,
}

/// Orchestrates screen capture via [`FfmpegRecorder`] plus one-shot encode helpers.
///
/// Construction **fails loud** when system `ffmpeg` is missing
/// ([`codes::DESKTOP_RECORDING_UNAVAILABLE`]).
pub struct RecordingPipeline {
    recorder: FfmpegRecorder,
    ffmpeg: PathBuf,
    started_at: std::sync::Mutex<Option<(Instant, PathBuf)>>,
}

impl RecordingPipeline {
    /// Resolve system `ffmpeg` and build a pipeline; fail-loud if absent.
    pub fn try_new() -> Result<Self> {
        let Some(ffmpeg) = probe::resolve_ffmpeg() else {
            return Err(PhenoError::unsupported_platform(
                codes::DESKTOP_RECORDING_UNAVAILABLE,
                "RecordingPipeline::try_new: system `ffmpeg` not found on PATH \
                 (set EIDOLON_FFMPEG or install ffmpeg; docs/EXTRACTION_PLAN.md Phase E)",
            ));
        };
        let recorder = FfmpegRecorder::try_new()?;
        Ok(Self {
            recorder,
            ffmpeg,
            started_at: std::sync::Mutex::new(None),
        })
    }

    /// Path to the resolved FFmpeg binary.
    pub fn ffmpeg_path(&self) -> &Path {
        &self.ffmpeg
    }

    /// Whether a capture session is in progress.
    pub fn is_recording(&self) -> bool {
        self.recorder.is_recording()
    }

    /// Start screen capture to `output_path`.
    pub async fn start_recording(
        &self,
        output_path: &str,
        profile: QualityProfile,
    ) -> Result<()> {
        self.recorder.start(output_path, profile).await?;
        let mut guard = self.started_at.lock().map_err(|_| {
            PhenoError::Internal("RecordingPipeline mutex poisoned".into())
        })?;
        *guard = Some((Instant::now(), PathBuf::from(output_path)));
        Ok(())
    }

    /// Stop the active capture and return size / duration metadata when possible.
    pub async fn stop_recording(&self) -> Result<RecordingResult> {
        let meta = {
            let mut guard = self.started_at.lock().map_err(|_| {
                PhenoError::Internal("RecordingPipeline mutex poisoned".into())
            })?;
            guard.take()
        };

        let stop_result = self.recorder.stop().await;
        let (started, output_path) = meta.unwrap_or_else(|| (Instant::now(), PathBuf::new()));
        let duration = started.elapsed();
        let file_size_bytes = std::fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0);

        match stop_result {
            Ok(()) => Ok(RecordingResult {
                output_path,
                duration,
                file_size_bytes,
                success: true,
            }),
            Err(e) => {
                // Still return metadata so callers can inspect a partial file.
                if file_size_bytes > 0 {
                    log::warn!("stop_recording: ffmpeg reported error but file exists: {e}");
                    Ok(RecordingResult {
                        output_path,
                        duration,
                        file_size_bytes,
                        success: false,
                    })
                } else {
                    Err(e)
                }
            }
        }
    }

    /// One-shot file→file re-encode (blocks until ffmpeg exits).
    pub fn encode_file(
        &self,
        input_path: &str,
        output_path: &str,
        profile: QualityProfile,
    ) -> Result<()> {
        if !Path::new(input_path).is_file() {
            return Err(PhenoError::Platform(format!(
                "encode input does not exist: {input_path}"
            )));
        }
        validate_output_parent(output_path)?;
        let args = ffmpeg_encode_args(input_path, output_path, profile);
        self.run_ffmpeg(&args, "encode_file")
    }

    /// One-shot video→GIF encode.
    pub fn encode_gif(&self, input_path: &str, output_path: &str) -> Result<()> {
        if !Path::new(input_path).is_file() {
            return Err(PhenoError::Platform(format!(
                "gif input does not exist: {input_path}"
            )));
        }
        validate_output_parent(output_path)?;
        let args = ffmpeg_gif_args(input_path, output_path);
        self.run_ffmpeg(&args, "encode_gif")
    }

    /// Encode a short lavfi test pattern (useful for hermetic encode-path checks
    /// when `ffmpeg` is installed but screen capture is not permitted).
    pub fn encode_test_pattern(
        &self,
        output_path: &str,
        profile: QualityProfile,
        duration_secs: u32,
    ) -> Result<()> {
        validate_output_parent(output_path)?;
        let secs = duration_secs.max(1);
        let args = ffmpeg_test_pattern_args(output_path, profile, secs);
        self.run_ffmpeg(&args, "encode_test_pattern")
    }

    fn run_ffmpeg(&self, args: &[String], method: &str) -> Result<()> {
        let output = Command::new(&self.ffmpeg)
            .args(args)
            .output()
            .map_err(|e| {
                PhenoError::unsupported_platform(
                    codes::DESKTOP_RECORDING_UNAVAILABLE,
                    format!("RecordingPipeline::{method}: failed to spawn ffmpeg: {e}"),
                )
            })?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
            Err(PhenoError::Platform(format!(
                "RecordingPipeline::{method}: ffmpeg exited with {}: {detail}",
                output.status
            )))
        }
    }
}

#[async_trait::async_trait]
impl DesktopRecorder for RecordingPipeline {
    async fn start(&self, output_path: &str, profile: QualityProfile) -> Result<()> {
        self.start_recording(output_path, profile).await
    }

    async fn stop(&self) -> Result<()> {
        self.stop_recording().await.map(|_| ())
    }

    fn ffmpeg_ready(&self) -> bool {
        true
    }
}
