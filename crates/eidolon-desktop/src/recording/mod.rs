//! Desktop recording / FFmpeg pipeline (KDesktopVirt Phase E — complete).
//!
//! # Status
//!
//! - Always-on: [`DesktopRecorder`] trait, [`QualityProfile`], [`VideoFormat`],
//!   argv builders, fail-loud [`RecordingStub`], and [`probe`] helpers.
//! - Feature `desktop-recording`: [`FfmpegRecorder`] + [`RecordingPipeline`] —
//!   system-`ffmpeg` screen capture start/stop and file encode / GIF paths.
//!   Requires a system `ffmpeg` binary (no bundled blobs). Live capture
//!   integration is gated on `RECORDING_INTEGRATION=1`.
//!
//! Do **not** unarchive KDesktopVirt for routine work; copy patterns only.
//! Do **not** port `ffmpeg_pipeline_broken.rs`.
//!
//! Intentionally **not** ported (enterprise / out-of-library): RTMP/WebRTC
//! streaming, audio DSP, bollard container recording (belongs in sandbox if
//! ever needed).
//!
//! See `docs/EXTRACTION_PLAN.md` Phase E and
//! `docs/consolidation/KDesktopVirt-to-Eidolon.md` Phase E.

mod profile;
pub mod probe;
mod stub;

#[cfg(feature = "desktop-recording")]
mod ffmpeg;
#[cfg(feature = "desktop-recording")]
mod pipeline;

pub use profile::{
    ffmpeg_capture_args, ffmpeg_encode_args, ffmpeg_gif_args, QualityProfile, VideoFormat,
};
pub use stub::RecordingStub;

#[cfg(feature = "desktop-recording")]
pub use ffmpeg::FfmpegRecorder;
#[cfg(feature = "desktop-recording")]
pub use pipeline::{RecordingPipeline, RecordingResult};

use eidolon_core::Result;
#[cfg(any(test, feature = "desktop-recording"))]
use eidolon_core::error::PhenoError;
#[cfg(any(test, feature = "desktop-recording"))]
use std::path::Path;

/// Trait hooks for screen / session recording.
///
/// Prefer [`FfmpegRecorder`] / [`RecordingPipeline`] when the
/// `desktop-recording` feature is enabled and [`probe::ffmpeg_ready`] is true.
/// Otherwise use [`RecordingStub`] (fail-loud).
#[async_trait::async_trait]
pub trait DesktopRecorder: Send + Sync {
    /// Start recording to `output_path`.
    async fn start(&self, output_path: &str, profile: QualityProfile) -> Result<()>;

    /// Stop an in-flight recording.
    async fn stop(&self) -> Result<()>;

    /// Whether an FFmpeg (or equivalent) backend is available on this host.
    fn ffmpeg_ready(&self) -> bool {
        false
    }
}

/// Validate that the parent directory of `output_path` exists (when present).
#[cfg(any(test, feature = "desktop-recording"))]
pub(crate) fn validate_output_parent(output_path: &str) -> Result<()> {
    let path_buf = Path::new(output_path);
    if let Some(parent) = path_buf.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(PhenoError::Platform(format!(
                "recording parent directory does not exist: {}",
                parent.display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn quality_profile_presets() {
        assert_eq!(QualityProfile::High.x264_preset(), "slow");
        assert_eq!(QualityProfile::Balanced.x264_preset(), "veryfast");
        assert_eq!(QualityProfile::LowLatency.x264_preset(), "ultrafast");
        assert_eq!(QualityProfile::Balanced.video_bitrate(), "4M");
        assert_eq!(QualityProfile::High.crf(), 18);
        assert_eq!(QualityProfile::Balanced.frame_rate(), 30);
    }

    #[test]
    fn capture_args_include_output_and_codec() {
        let args = ffmpeg_capture_args("/tmp/out.mp4", QualityProfile::LowLatency);
        assert!(args.contains(&"-y".into()));
        assert!(args.contains(&"libx264".into()));
        assert!(args.contains(&"ultrafast".into()));
        assert_eq!(args.last().map(String::as_str), Some("/tmp/out.mp4"));
    }

    #[test]
    fn encode_args_map_profile() {
        let args = ffmpeg_encode_args("/in.mp4", "/out.mp4", QualityProfile::High);
        assert!(args.iter().any(|a| a == "/in.mp4"));
        assert!(args.iter().any(|a| a == "slow"));
        assert!(args.iter().any(|a| a == "18"));
        assert_eq!(args.last().map(String::as_str), Some("/out.mp4"));
    }

    #[test]
    fn gif_args_end_with_gif_path() {
        let args = ffmpeg_gif_args("/in.mp4", "/out.gif");
        assert!(args.iter().any(|a| a == "fps=10,scale=480:-1:flags=lanczos"));
        assert_eq!(args.last().map(String::as_str), Some("/out.gif"));
    }

    #[test]
    fn test_pattern_args_include_lavfi() {
        let args = profile::ffmpeg_test_pattern_args("/tmp/p.mp4", QualityProfile::High, 2);
        assert!(args.iter().any(|a| a == "lavfi"));
        assert!(args.iter().any(|a| a.contains("color=c=blue")));
        assert!(args.iter().any(|a| a == "2"));
        assert_eq!(args.last().map(String::as_str), Some("/tmp/p.mp4"));
    }

    #[test]
    fn video_format_extensions() {
        assert_eq!(VideoFormat::Mp4.extension(), "mp4");
        assert_eq!(VideoFormat::Webm.extension(), "webm");
        assert_eq!(VideoFormat::Gif.extension(), "gif");
    }

    #[test]
    fn validate_missing_parent_fails() {
        let err = validate_output_parent("/nonexistent-dir-xyz/out.mp4").unwrap_err();
        match err {
            PhenoError::Platform(msg) => assert!(msg.contains("parent directory")),
            other => panic!("expected Platform, got {other:?}"),
        }
    }

    #[test]
    fn probe_ready_matches_resolve() {
        assert_eq!(probe::ffmpeg_ready(), probe::resolve_ffmpeg().is_some());
    }
}
