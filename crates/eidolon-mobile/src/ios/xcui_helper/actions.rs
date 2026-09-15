//! Action implementations for the XCUI helper.

use std::process::Command;

use super::HelperCommand;
use crate::cli::which_bin;
use crate::ios::xcui_bridge::AppleScriptXcuiBridge;

/// Result of a successful helper op (`viewport` may carry stdout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperOutput {
    pub stdout: String,
}

/// Execute a parsed command (best-effort macOS / fail-loud elsewhere).
pub fn execute(cmd: &HelperCommand) -> Result<HelperOutput, String> {
    #[cfg(target_os = "macos")]
    {
        execute_macos(cmd)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = cmd;
        Err(format!(
            "eidolon-xcui-helper: iOS Simulator automation is macOS-only; \
             full XCUI needs an Xcode test host (op={})",
            cmd.op_name()
        ))
    }
}

#[cfg(target_os = "macos")]
fn execute_macos(cmd: &HelperCommand) -> Result<HelperOutput, String> {
    ensure_simulator_context(cmd.udid())?;
    match cmd {
        HelperCommand::Tap { x, y, .. } => {
            run_osascript(&AppleScriptXcuiBridge::script_tap(*x, *y))?;
            Ok(HelperOutput {
                stdout: String::new(),
            })
        }
        HelperCommand::Swipe { x1, y1, x2, y2, .. } => {
            run_osascript(&AppleScriptXcuiBridge::script_swipe(*x1, *y1, *x2, *y2))?;
            Ok(HelperOutput {
                stdout: String::new(),
            })
        }
        HelperCommand::Text { value, .. } => {
            if value.is_empty() {
                return Err("text --value must be non-empty".into());
            }
            run_osascript(&AppleScriptXcuiBridge::script_text(value))?;
            Ok(HelperOutput {
                stdout: String::new(),
            })
        }
        HelperCommand::Viewport { udid } => {
            let line = resolve_viewport_line(udid)?;
            Ok(HelperOutput { stdout: line })
        }
    }
}

/// Fail loud unless the UDID looks like a known Simulator (Booted preferred).
#[cfg(target_os = "macos")]
fn ensure_simulator_context(udid: &str) -> Result<(), String> {
    let Some(xcrun) = which_bin("xcrun").or_else(|| {
        std::env::var("EIDOLON_XCRUN")
            .ok()
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file())
    }) else {
        return Err(format!(
            "eidolon-xcui-helper: xcrun not found — cannot verify Simulator \
             UDID `{udid}`; install Xcode CLIs, or use a real XCUITest host \
             for device-pixel accurate automation"
        ));
    };
    let output = Command::new(&xcrun)
        .args(["simctl", "list", "devices", "-j"])
        .output()
        .map_err(|e| format!("simctl list failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "eidolon-xcui-helper: simctl list failed — {}",
            stderr.trim().lines().last().unwrap_or("unknown")
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(udid) {
        return Err(format!(
            "eidolon-xcui-helper: UDID `{udid}` not found in `simctl list` — \
             boot a Simulator or set a real UDID; full XCUI still needs an \
             Xcode test host (this helper is best-effort AppleScript only)"
        ));
    }
    let idx = stdout.find(udid).expect("udid present");
    let start = idx.saturating_sub(200);
    let end = (idx + udid.len() + 200).min(stdout.len());
    let window = &stdout[start..end];
    if !window.contains("Booted") {
        return Err(format!(
            "eidolon-xcui-helper: Simulator `{udid}` is not Booted — \
             boot it via Simulator.app / `simctl boot`, then retry; \
             full XCUI needs an Xcode test host"
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<(), String> {
    let osascript = which_bin("osascript").ok_or_else(|| {
        "eidolon-xcui-helper: osascript not found — cannot send best-effort \
         Simulator clicks; full XCUI needs an Xcode test host"
            .to_string()
    })?;
    let output = Command::new(&osascript)
        .args(["-e", script])
        .output()
        .map_err(|e| format!("osascript spawn failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "eidolon-xcui-helper: osascript failed (Simulator may lack \
             Accessibility permission, or full XCUI needs an Xcode test host): {}",
            stderr.trim().lines().last().unwrap_or("unknown")
        ));
    }
    Ok(())
}

/// Best-effort viewport: parse `simctl list -j` deviceTypeIdentifier → known sizes.
#[cfg(target_os = "macos")]
fn resolve_viewport_line(udid: &str) -> Result<String, String> {
    let xcrun = which_bin("xcrun")
        .ok_or_else(|| "eidolon-xcui-helper: xcrun required for viewport".to_string())?;
    let output = Command::new(&xcrun)
        .args(["simctl", "list", "devices", "-j"])
        .output()
        .map_err(|e| format!("simctl list failed: {e}"))?;
    if !output.status.success() {
        return Err("simctl list failed for viewport".into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let device_type = extract_device_type_near_udid(&stdout, udid).ok_or_else(|| {
        format!(
            "eidolon-xcui-helper: could not resolve deviceType for `{udid}` — \
             full XCUI / XCTest host can report accurate viewport; refusing to invent"
        )
    })?;
    let (w, h, scale) = viewport_for_device_type(&device_type).ok_or_else(|| {
        format!(
            "eidolon-xcui-helper: unknown deviceType `{device_type}` for `{udid}` — \
             full XCUI needs an Xcode test host; refusing to invent pixels"
        )
    })?;
    Ok(format!("{w} {h} {scale}"))
}

/// Pull `deviceTypeIdentifier` from a small JSON window around the UDID.
#[cfg(target_os = "macos")]
fn extract_device_type_near_udid(json: &str, udid: &str) -> Option<String> {
    let idx = json.find(udid)?;
    let start = idx.saturating_sub(400);
    let end = (idx + udid.len() + 400).min(json.len());
    let window = &json[start..end];
    let key = "\"deviceTypeIdentifier\"";
    let k = window.find(key)?;
    let after = &window[k + key.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end_q = rest.find('"')?;
    Some(rest[..end_q].to_string())
}

/// Known Simulator logical points (not exhaustive — unknown → None / fail-loud).
#[cfg(target_os = "macos")]
fn viewport_for_device_type(device_type: &str) -> Option<(u32, u32, f64)> {
    let name = device_type.rsplit('.').next().unwrap_or(device_type);
    match name {
        "iPhone-16-Pro" | "iPhone-15-Pro" | "iPhone-14-Pro" => Some((393, 852, 3.0)),
        "iPhone-16" | "iPhone-15" | "iPhone-14" => Some((390, 844, 3.0)),
        "iPhone-16-Pro-Max" | "iPhone-15-Pro-Max" | "iPhone-14-Pro-Max" => Some((430, 932, 3.0)),
        "iPhone-SE-3rd-generation" | "iPhone-SE--3rd-generation-" => Some((375, 667, 2.0)),
        "iPad-Pro-13-inch-M4-Wi-Fi" | "iPad-Pro-12-9-inch-6th-generation-8GB" => {
            Some((1024, 1366, 2.0))
        }
        "iPad-Air-11-inch-M2-Wi-Fi" | "iPad-Air-5th-generation" => Some((820, 1180, 2.0)),
        _ => None,
    }
}
