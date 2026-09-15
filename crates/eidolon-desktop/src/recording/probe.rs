//! Probe helpers for a system `ffmpeg` binary (no crates.io FFmpeg binding).

use std::path::PathBuf;
use std::process::Command;

/// Env override for the FFmpeg binary path (`EIDOLON_FFMPEG`).
pub const FFMPEG_PATH_ENV: &str = "EIDOLON_FFMPEG";

/// Resolve `ffmpeg` via `EIDOLON_FFMPEG` or `PATH`.
pub fn resolve_ffmpeg() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var(FFMPEG_PATH_ENV) {
        let p = PathBuf::from(override_path);
        if p.is_file() {
            return Some(p);
        }
    }
    which_ffmpeg()
}

/// `true` when [`resolve_ffmpeg`] finds a runnable binary.
pub fn ffmpeg_ready() -> bool {
    resolve_ffmpeg().is_some()
}

/// First line of `ffmpeg -version`, if available.
pub fn ffmpeg_version_line() -> Option<String> {
    let bin = resolve_ffmpeg()?;
    let output = Command::new(&bin).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
}

fn which_ffmpeg() -> Option<PathBuf> {
    let output = Command::new("which").arg("ffmpeg").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let p = PathBuf::from(path);
    p.is_file().then_some(p)
}
