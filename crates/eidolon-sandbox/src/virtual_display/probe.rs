//! Host probes for virtual display tooling (no spawn required).
//!
//! Always-on and macOS-safe — boolean probes only. Live spawn remains behind
//! feature `sandbox-virtual-display` + Linux + [`super::XVFB_INTEGRATION_ENV`].

use std::path::{Path, PathBuf};
use std::process::Command;

/// Env override for the Xvfb binary path (`EIDOLON_XVFB`).
pub const XVFB_PATH_ENV: &str = "EIDOLON_XVFB";

/// Env override for x11vnc (`EIDOLON_X11VNC`).
pub const X11VNC_PATH_ENV: &str = "EIDOLON_X11VNC";

/// Env override for TigerVNC `Xvnc` (`EIDOLON_XVNC`).
pub const XVNC_PATH_ENV: &str = "EIDOLON_XVNC";

/// Env override for Weston compositor (`EIDOLON_WESTON`).
pub const WESTON_PATH_ENV: &str = "EIDOLON_WESTON";

/// Candidate Xvfb binary names on `PATH`.
pub const XVFB_CANDIDATES: &[&str] = &["Xvfb"];

/// Candidate x11vnc binary names on `PATH`.
pub const X11VNC_CANDIDATES: &[&str] = &["x11vnc"];

/// Candidate TigerVNC server names on `PATH`.
pub const XVNC_CANDIDATES: &[&str] = &["Xvnc"];

/// Candidate headless Wayland compositor names on `PATH`.
pub const WAYLAND_COMPOSITOR_CANDIDATES: &[&str] = &["weston", "mutter", "cage"];

/// Resolve `Xvfb` via [`XVFB_PATH_ENV`] or `PATH`.
pub fn resolve_xvfb_cli() -> Option<PathBuf> {
    resolve_with_env_override(XVFB_PATH_ENV, XVFB_CANDIDATES)
}

/// `true` when [`resolve_xvfb_cli`] finds a runnable binary.
pub fn xvfb_ready() -> bool {
    resolve_xvfb_cli().is_some()
}

/// First line of `Xvfb -help`, if available.
pub fn xvfb_help_line() -> Option<String> {
    let bin = resolve_xvfb_cli()?;
    command_first_line(&bin, &["-help"])
}

/// Resolve x11vnc via env override or `PATH`.
pub fn resolve_x11vnc_cli() -> Option<PathBuf> {
    resolve_with_env_override(X11VNC_PATH_ENV, X11VNC_CANDIDATES)
}

/// `true` when x11vnc appears on `PATH`.
pub fn x11vnc_ready() -> bool {
    resolve_x11vnc_cli().is_some()
}

/// Resolve TigerVNC `Xvnc` via env override or `PATH`.
pub fn resolve_xvnc_cli() -> Option<PathBuf> {
    resolve_with_env_override(XVNC_PATH_ENV, XVNC_CANDIDATES)
}

/// `true` when TigerVNC `Xvnc` appears on `PATH`.
pub fn xvnc_ready() -> bool {
    resolve_xvnc_cli().is_some()
}

/// `true` when any VNC server binary (x11vnc or Xvnc) is discoverable.
pub fn vnc_server_ready() -> bool {
    x11vnc_ready() || xvnc_ready()
}

/// Resolve Weston (or other compositor) via env override or `PATH`.
pub fn resolve_wayland_compositor_cli() -> Option<PathBuf> {
    resolve_with_env_override(WESTON_PATH_ENV, WAYLAND_COMPOSITOR_CANDIDATES)
}

/// `true` when a Wayland compositor binary is discoverable.
pub fn wayland_compositor_ready() -> bool {
    resolve_wayland_compositor_cli().is_some()
}

fn resolve_with_env_override(env_key: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var(env_key) {
        let p = PathBuf::from(override_path);
        if p.is_file() {
            return Some(p);
        }
    }
    for name in candidates {
        if let Some(p) = which_bin(name) {
            return Some(p);
        }
    }
    None
}

fn which_bin(name: &str) -> Option<PathBuf> {
    let output = Command::new("which").arg(name).output().ok()?;
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

fn command_first_line(bin: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(bin).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_functions_are_boolean() {
        let _ = xvfb_ready();
        let _ = xvfb_help_line();
        let _ = x11vnc_ready();
        let _ = xvnc_ready();
        let _ = vnc_server_ready();
        let _ = wayland_compositor_ready();
        let _ = resolve_xvfb_cli();
        let _ = resolve_x11vnc_cli();
        let _ = resolve_xvnc_cli();
        let _ = resolve_wayland_compositor_cli();
    }
}
