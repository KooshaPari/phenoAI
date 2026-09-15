//! Quality profiles, output formats, and FFmpeg argv builders.

/// Recording quality hint → FFmpeg preset / bitrate / CRF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualityProfile {
    #[default]
    Balanced,
    High,
    LowLatency,
}

impl QualityProfile {
    /// libx264 `-preset` for capture and encode paths.
    pub fn x264_preset(self) -> &'static str {
        match self {
            Self::High => "slow",
            Self::Balanced => "veryfast",
            Self::LowLatency => "ultrafast",
        }
    }

    /// Target video bitrate string for FFmpeg `-b:v` (capture path).
    pub fn video_bitrate(self) -> &'static str {
        match self {
            Self::High => "8M",
            Self::Balanced => "4M",
            Self::LowLatency => "2M",
        }
    }

    /// Constant rate factor for file encode (`-crf`); lower = higher quality.
    pub fn crf(self) -> u8 {
        match self {
            Self::High => 18,
            Self::Balanced => 23,
            Self::LowLatency => 28,
        }
    }

    /// Nominal frame rate for capture argv (`-r`).
    pub fn frame_rate(self) -> u32 {
        match self {
            Self::High => 60,
            Self::Balanced => 30,
            Self::LowLatency => 30,
        }
    }

    /// Suggested capture / encode resolution (width, height).
    pub fn resolution(self) -> (u32, u32) {
        match self {
            Self::High => (1920, 1080),
            Self::Balanced => (1920, 1080),
            Self::LowLatency => (1280, 720),
        }
    }
}

/// Output container / format hint for encode helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoFormat {
    #[default]
    Mp4,
    Webm,
    Gif,
}

impl VideoFormat {
    /// File extension without a leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Webm => "webm",
            Self::Gif => "gif",
        }
    }
}

/// Build platform-specific FFmpeg argv for screen capture (no bollard / no blobs).
pub fn ffmpeg_capture_args(output_path: &str, profile: QualityProfile) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let fps = profile.frame_rate().to_string();
    let (w, h) = profile.resolution();

    #[cfg(target_os = "macos")]
    {
        let _ = (w, h); // resolution applied post-capture via encode path when needed
        // avfoundation: screen index 1 is typically the main display; audio omitted.
        args.extend([
            "-y".into(),
            "-f".into(),
            "avfoundation".into(),
            "-framerate".into(),
            fps,
            "-capture_cursor".into(),
            "1".into(),
            "-i".into(),
            "1:none".into(),
        ]);
    }

    #[cfg(target_os = "linux")]
    {
        let size = format!("{w}x{h}");
        args.extend([
            "-y".into(),
            "-f".into(),
            "x11grab".into(),
            "-framerate".into(),
            fps,
            "-video_size".into(),
            size,
            "-i".into(),
            ":0.0".into(),
        ]);
    }

    #[cfg(target_os = "windows")]
    {
        let _ = (w, h);
        args.extend([
            "-y".into(),
            "-f".into(),
            "gdigrab".into(),
            "-framerate".into(),
            fps,
            "-i".into(),
            "desktop".into(),
        ]);
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        args.extend([
            "-y".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("color=c=black:s={w}x{h}:r={fps}:d=1"),
        ]);
    }

    args.extend([
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        profile.x264_preset().into(),
        "-b:v".into(),
        profile.video_bitrate().into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        output_path.into(),
    ]);
    args
}

/// Build FFmpeg argv for file→file re-encode (software libx264 / libvpx).
pub fn ffmpeg_encode_args(input_path: &str, output_path: &str, profile: QualityProfile) -> Vec<String> {
    let mut args = vec![
        "-y".into(),
        "-i".into(),
        input_path.into(),
    ];

    if output_path.ends_with(".webm") {
        args.extend([
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-b:v".into(),
            profile.video_bitrate().into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ]);
    } else {
        args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            profile.x264_preset().into(),
            "-crf".into(),
            profile.crf().to_string(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-movflags".into(),
            "+faststart".into(),
        ]);
    }

    args.push(output_path.into());
    args
}

/// Build FFmpeg argv for video→GIF (single-pass, scaled).
pub fn ffmpeg_gif_args(input_path: &str, output_path: &str) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        input_path.into(),
        "-vf".into(),
        "fps=10,scale=480:-1:flags=lanczos".into(),
        "-loop".into(),
        "0".into(),
        output_path.into(),
    ]
}

/// Build FFmpeg argv for a short lavfi test-pattern encode (hermetic when ffmpeg exists).
#[cfg(any(test, feature = "desktop-recording"))]
pub fn ffmpeg_test_pattern_args(
    output_path: &str,
    profile: QualityProfile,
    duration_secs: u32,
) -> Vec<String> {
    let (w, h) = profile.resolution();
    let fps = profile.frame_rate();
    let src = format!("color=c=blue:s={w}x{h}:r={fps}:d={duration_secs}");
    let mut args = vec![
        "-y".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        src,
    ];
    args.extend([
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        profile.x264_preset().into(),
        "-crf".into(),
        profile.crf().to_string(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-t".into(),
        duration_secs.to_string(),
        output_path.into(),
    ]);
    args
}
