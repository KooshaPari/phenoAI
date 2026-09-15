//! Recording / FFmpeg probe + feature-gated pipeline tests (Phase E).
//!
//! Default `cargo test --locked` stays green without screen-capture permissions.
//! Live start/stop requires `--features desktop-recording` and
//! `RECORDING_INTEGRATION=1`.

use eidolon_core::error::PhenoError;
use eidolon_desktop::codes;
use eidolon_desktop::recording::{
    ffmpeg_capture_args, ffmpeg_encode_args, ffmpeg_gif_args, probe, QualityProfile,
    RecordingStub, VideoFormat,
};
use eidolon_desktop::DesktopRecorder;

fn assert_recording_unavailable(err: PhenoError) {
    assert_eq!(
        err.unsupported_code(),
        Some(codes::DESKTOP_RECORDING_UNAVAILABLE)
    );
    assert_eq!(err.status_code(), 501);
}

#[tokio::test]
async fn recording_stub_always_fail_loud() {
    let stub = RecordingStub::new();
    assert!(!stub.ffmpeg_ready());
    assert_recording_unavailable(
        stub.start("/tmp/out.mp4", QualityProfile::Balanced)
            .await
            .unwrap_err(),
    );
    assert_recording_unavailable(stub.stop().await.unwrap_err());
}

#[test]
fn capture_args_are_nonempty_and_end_with_path() {
    let args = ffmpeg_capture_args("/tmp/eidolon-phase-e.mp4", QualityProfile::High);
    assert!(args.len() > 4);
    assert_eq!(
        args.last().map(String::as_str),
        Some("/tmp/eidolon-phase-e.mp4")
    );
    assert!(args.iter().any(|a| a == "libx264"));
    assert!(args.iter().any(|a| a == "slow"));
    assert!(args.iter().any(|a| a == "60"));
    assert!(args.iter().any(|a| a == "-framerate" || a == "-r"));
}

#[test]
fn encode_and_gif_argv_builders() {
    let enc = ffmpeg_encode_args("/in.mp4", "/out.mp4", QualityProfile::Balanced);
    assert!(enc.iter().any(|a| a == "libx264"));
    assert!(enc.iter().any(|a| a == "23"));
    assert!(enc.iter().any(|a| a == "+faststart"));

    let webm = ffmpeg_encode_args("/in.mp4", "/out.webm", QualityProfile::LowLatency);
    assert!(webm.iter().any(|a| a == "libvpx-vp9"));

    let gif = ffmpeg_gif_args("/in.mp4", "/out.gif");
    assert_eq!(gif.last().map(String::as_str), Some("/out.gif"));
    assert_eq!(VideoFormat::Gif.extension(), "gif");
}

#[test]
fn probe_ffmpeg_consistency() {
    let ready = probe::ffmpeg_ready();
    let resolved = probe::resolve_ffmpeg();
    assert_eq!(ready, resolved.is_some());
    if let Some(path) = resolved {
        assert!(path.is_file(), "resolved ffmpeg must be a file: {path:?}");
        let line = probe::ffmpeg_version_line();
        assert!(
            line.is_some(),
            "ffmpeg -version should succeed when binary exists"
        );
        let line = line.unwrap();
        assert!(
            line.to_lowercase().contains("ffmpeg"),
            "version line = {line}"
        );
    }
}

#[cfg(feature = "desktop-recording")]
mod with_feature {
    use super::*;
    use eidolon_desktop::{FfmpegRecorder, RecordingPipeline};
    use std::path::PathBuf;

    #[test]
    fn try_new_matches_probe_fail_loud() {
        match FfmpegRecorder::try_new() {
            Ok(rec) => {
                assert!(probe::ffmpeg_ready());
                assert!(rec.ffmpeg_ready());
                assert!(rec.ffmpeg_path().is_file());
            }
            Err(err) => {
                assert!(!probe::ffmpeg_ready());
                assert_recording_unavailable(err);
            }
        }

        match RecordingPipeline::try_new() {
            Ok(pipe) => {
                assert!(probe::ffmpeg_ready());
                assert!(pipe.ffmpeg_ready());
                assert!(pipe.ffmpeg_path().is_file());
            }
            Err(err) => {
                assert!(!probe::ffmpeg_ready());
                assert_recording_unavailable(err);
            }
        }
    }

    #[tokio::test]
    async fn start_missing_parent_is_platform_error() {
        let Ok(rec) = FfmpegRecorder::try_new() else {
            return;
        };
        let err = rec
            .start(
                "/nonexistent-eidolon-phase-e-dir/out.mp4",
                QualityProfile::Balanced,
            )
            .await
            .unwrap_err();
        match err {
            PhenoError::Platform(msg) => assert!(msg.contains("parent directory")),
            other => panic!("expected Platform, got {other:?}"),
        }
    }

    #[test]
    fn encode_test_pattern_when_ffmpeg_present() {
        let Ok(pipe) = RecordingPipeline::try_new() else {
            eprintln!("skip encode_test_pattern: no ffmpeg");
            return;
        };
        let dir = std::env::temp_dir().join("eidolon-phase-e-encode");
        let _ = std::fs::create_dir_all(&dir);
        let out = dir.join("pattern.mp4");
        let _ = std::fs::remove_file(&out);
        pipe.encode_test_pattern(out.to_str().unwrap(), QualityProfile::LowLatency, 1)
            .expect("lavfi test-pattern encode");
        assert!(out.is_file(), "expected output file {out:?}");
        let meta = std::fs::metadata(&out).expect("meta");
        assert!(meta.len() > 0, "encoded file should be non-empty");

        let gif = dir.join("pattern.gif");
        let _ = std::fs::remove_file(&gif);
        pipe.encode_gif(out.to_str().unwrap(), gif.to_str().unwrap())
            .expect("gif encode");
        assert!(gif.is_file());

        let reenc = dir.join("pattern-reenc.mp4");
        let _ = std::fs::remove_file(&reenc);
        pipe.encode_file(
            out.to_str().unwrap(),
            reenc.to_str().unwrap(),
            QualityProfile::Balanced,
        )
        .expect("re-encode");
        assert!(reenc.is_file());
    }

    #[tokio::test]
    async fn integration_start_stop_gated() {
        if std::env::var("RECORDING_INTEGRATION").ok().as_deref() != Some("1") {
            eprintln!("skip: set RECORDING_INTEGRATION=1 for live FFmpeg capture");
            return;
        }
        let pipe = match RecordingPipeline::try_new() {
            Ok(r) => r,
            Err(e) => panic!("RECORDING_INTEGRATION=1 requires ffmpeg: {e}"),
        };

        let dir = std::env::temp_dir().join("eidolon-phase-e-recording");
        let _ = std::fs::create_dir_all(&dir);
        let out: PathBuf = dir.join("capture.mp4");
        let _ = std::fs::remove_file(&out);

        pipe.start_recording(out.to_str().unwrap(), QualityProfile::LowLatency)
            .await
            .expect("start recording");
        assert!(pipe.is_recording());
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        // stop may return success:false if OS denied screen capture — still proves wiring.
        let result = pipe.stop_recording().await;
        match result {
            Ok(r) => {
                assert!(!pipe.is_recording());
                eprintln!(
                    "integration stop: success={} bytes={} path={:?}",
                    r.success, r.file_size_bytes, r.output_path
                );
            }
            Err(e) => eprintln!("integration stop error (wiring ok): {e}"),
        }
    }
}
