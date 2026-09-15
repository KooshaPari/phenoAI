//! XCUI-style action bridge for iOS Simulator / device input.
//!
//! This module:
//! - defines [`XcuiBridge`] (tap / swipe / text / viewport)
//! - runs a helper when [`crate::cli::IOS_XCUI_BUNDLE_ENV`] is set (**wins**)
//! - otherwise prefers the in-tree XCUITest runner (`eidolon-xcui-xctest`) when
//!   built under `native/ios/EidolonXcuiHelper/build/`
//! - else discovers the Rust [`crate::ios::xcui_helper`] fallback binary
//!   (`CARGO_BIN_EXE_*` / next to current exe / `PATH`)
//! - optionally uses AppleScript (`osascript`) when
//!   [`crate::cli::IOS_ALLOW_APPLESCRIPT_ENV`]`=1` (screen points, best-effort)
//! - otherwise fails loud with [`codes::MOBILE_IOS_XCUI_UNAVAILABLE`]
//!
//! Never pretends success when tools / helper / project are missing.
//! Do not unarchive kmobile.

use crate::cli::{
    env_ios_xcui_bundle, ios_applescript_allowed, which_bin, IOS_ALLOW_APPLESCRIPT_ENV,
    IOS_XCUI_BUNDLE_ENV,
};
use crate::codes;
use crate::ios::xcui_helper::{
    discover_preferred_helper, HELPER_BIN_NAME, XCODE_HELPER_BIN_NAME,
};
use eidolon_core::error::PhenoError;
use eidolon_core::{Result, Viewport};
use std::path::{Path, PathBuf};
use std::process::Command;

/// XCUI-shaped input / viewport bridge (in-tree XCUITest host preferred when built).
pub trait XcuiBridge: Send + Sync {
    /// Whether this bridge can honestly perform actions right now.
    fn ready(&self) -> bool;

    /// Backend label for logs / errors (`bundle`, `applescript`, `missing`).
    fn backend_name(&self) -> &'static str;

    fn tap(&self, udid: &str, x: i32, y: i32) -> Result<()>;

    fn swipe(&self, udid: &str, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()>;

    fn input_text(&self, udid: &str, text: &str) -> Result<()>;

    fn viewport(&self, udid: &str) -> Result<Viewport>;
}

fn xcui_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::MOBILE_IOS_XCUI_UNAVAILABLE,
        format!(
            "XcuiBridge::{method} unavailable — {detail}; set {IOS_XCUI_BUNDLE_ENV} \
             to a helper executable (argv: tap|swipe|text|viewport --udid …), build \
             the in-tree XCUITest runner `{XCODE_HELPER_BIN_NAME}` \
             (native/ios/EidolonXcuiHelper), or the Rust fallback `{HELPER_BIN_NAME}` \
             (`mobile-xcui-helper`), or set {IOS_ALLOW_APPLESCRIPT_ENV}=1 for \
             best-effort Simulator AppleScript (screen points); see \
             docs/guides/ios-xcui-helper.md"
        ),
    )
}

/// Fail-loud bridge when no helper / AppleScript path is configured.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissingXcuiBridge;

impl XcuiBridge for MissingXcuiBridge {
    fn ready(&self) -> bool {
        false
    }

    fn backend_name(&self) -> &'static str {
        "missing"
    }

    fn tap(&self, _udid: &str, _x: i32, _y: i32) -> Result<()> {
        Err(xcui_unavailable(
            "tap",
            "no XCUI helper and AppleScript not allowed",
        ))
    }

    fn swipe(&self, _udid: &str, _x1: i32, _y1: i32, _x2: i32, _y2: i32) -> Result<()> {
        Err(xcui_unavailable(
            "swipe",
            "no XCUI helper and AppleScript not allowed",
        ))
    }

    fn input_text(&self, _udid: &str, _text: &str) -> Result<()> {
        Err(xcui_unavailable(
            "input_text",
            "no XCUI helper and AppleScript not allowed",
        ))
    }

    fn viewport(&self, _udid: &str) -> Result<Viewport> {
        Err(xcui_unavailable(
            "viewport",
            "no XCUI helper (AppleScript cannot report device viewport)",
        ))
    }
}

/// Helper executable pointed at by [`IOS_XCUI_BUNDLE_ENV`].
///
/// Contract:
/// - `tap --udid <id> --x <n> --y <n>`
/// - `swipe --udid <id> --x1 <n> --y1 <n> --x2 <n> --y2 <n>`
/// - `text --udid <id> --value <str>`
/// - `viewport --udid <id>` → stdout `WIDTHxHEIGHT` or `WIDTH HEIGHT [SCALE]`
pub struct BundleXcuiBridge {
    helper: PathBuf,
}

impl BundleXcuiBridge {
    /// Construct from an existing helper path (caller validates existence).
    pub fn new(helper: PathBuf) -> Self {
        Self { helper }
    }

    /// Resolve from env; `None` when unset or not a file.
    pub fn from_env() -> Option<Self> {
        env_ios_xcui_bundle().map(Self::new)
    }

    /// Build argv for hermetic tests (no process spawn).
    pub fn build_argv(op: &str, udid: &str, extras: &[(&str, String)]) -> Vec<String> {
        let mut argv = vec![op.to_string(), "--udid".into(), udid.to_string()];
        for (k, v) in extras {
            argv.push((*k).to_string());
            argv.push(v.clone());
        }
        argv
    }

    fn run(&self, argv: &[String]) -> Result<String> {
        let output = Command::new(&self.helper)
            .args(argv)
            .output()
            .map_err(|e| {
                xcui_unavailable(
                    "run",
                    format!("spawn {} failed: {e}", self.helper.display()),
                )
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Err(PhenoError::Internal(format!(
                "{}: XCUI helper failed ({}): {}",
                codes::MOBILE_IOS_XCUI_UNAVAILABLE,
                self.helper.display(),
                stderr.trim().lines().last().unwrap_or("unknown")
            )));
        }
        Ok(stdout)
    }
}

impl XcuiBridge for BundleXcuiBridge {
    fn ready(&self) -> bool {
        self.helper.is_file()
    }

    fn backend_name(&self) -> &'static str {
        "bundle"
    }

    fn tap(&self, udid: &str, x: i32, y: i32) -> Result<()> {
        let argv = Self::build_argv(
            "tap",
            udid,
            &[("--x", x.to_string()), ("--y", y.to_string())],
        );
        self.run(&argv).map(|_| ())
    }

    fn swipe(&self, udid: &str, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()> {
        let argv = Self::build_argv(
            "swipe",
            udid,
            &[
                ("--x1", x1.to_string()),
                ("--y1", y1.to_string()),
                ("--x2", x2.to_string()),
                ("--y2", y2.to_string()),
            ],
        );
        self.run(&argv).map(|_| ())
    }

    fn input_text(&self, udid: &str, text: &str) -> Result<()> {
        let argv = Self::build_argv("text", udid, &[("--value", text.to_string())]);
        self.run(&argv).map(|_| ())
    }

    fn viewport(&self, udid: &str) -> Result<Viewport> {
        let argv = Self::build_argv("viewport", udid, &[]);
        let out = self.run(&argv)?;
        parse_viewport_stdout(&out).ok_or_else(|| {
            xcui_unavailable(
                "viewport",
                format!("helper stdout not WIDTHxHEIGHT: {}", out.trim()),
            )
        })
    }
}

/// Best-effort Simulator input via `osascript` (macOS screen points).
///
/// Not device-pixel accurate. Viewport always fails loud.
pub struct AppleScriptXcuiBridge {
    osascript: PathBuf,
}

impl AppleScriptXcuiBridge {
    pub fn try_new() -> Option<Self> {
        if !ios_applescript_allowed() {
            return None;
        }
        which_bin("osascript").map(|osascript| Self { osascript })
    }

    /// Hermetic AppleScript body for tap (no spawn).
    pub fn script_tap(x: i32, y: i32) -> String {
        format!(
            "tell application \"Simulator\" to activate\n\
             delay 0.15\n\
             tell application \"System Events\" to click at {{{x}, {y}}}"
        )
    }

    /// Hermetic AppleScript body for swipe (drag).
    pub fn script_swipe(x1: i32, y1: i32, x2: i32, y2: i32) -> String {
        format!(
            "tell application \"Simulator\" to activate\n\
             delay 0.15\n\
             tell application \"System Events\"\n\
             set startPt to {{{x1}, {y1}}}\n\
             set endPt to {{{x2}, {y2}}}\n\
             click at startPt\n\
             -- best-effort drag: mouse down / move / up via click endpoints\n\
             click at endPt\n\
             end tell"
        )
    }

    /// Hermetic AppleScript for keystrokes (escapes quotes/backslashes).
    pub fn script_text(text: &str) -> String {
        let escaped = escape_applescript_string(text);
        format!(
            "tell application \"Simulator\" to activate\n\
             delay 0.15\n\
             tell application \"System Events\" to keystroke \"{escaped}\""
        )
    }

    fn run_script(&self, script: &str) -> Result<()> {
        let output = Command::new(&self.osascript)
            .args(["-e", script])
            .output()
            .map_err(|e| xcui_unavailable("applescript", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PhenoError::Internal(format!(
                "{}: osascript failed: {}",
                codes::MOBILE_IOS_XCUI_UNAVAILABLE,
                stderr.trim().lines().last().unwrap_or("unknown")
            )));
        }
        Ok(())
    }
}

impl XcuiBridge for AppleScriptXcuiBridge {
    fn ready(&self) -> bool {
        self.osascript.is_file() && ios_applescript_allowed()
    }

    fn backend_name(&self) -> &'static str {
        "applescript"
    }

    fn tap(&self, _udid: &str, x: i32, y: i32) -> Result<()> {
        // udid unused: AppleScript targets the frontmost Simulator window.
        self.run_script(&Self::script_tap(x, y))
    }

    fn swipe(&self, _udid: &str, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()> {
        self.run_script(&Self::script_swipe(x1, y1, x2, y2))
    }

    fn input_text(&self, _udid: &str, text: &str) -> Result<()> {
        if text.is_empty() {
            return Err(PhenoError::BadRequest(
                "input_text requires non-empty text".into(),
            ));
        }
        self.run_script(&Self::script_text(text))
    }

    fn viewport(&self, _udid: &str) -> Result<Viewport> {
        Err(xcui_unavailable(
            "viewport",
            "AppleScript backend cannot report device viewport — use \
             EIDOLON_IOS_XCUI_BUNDLE helper",
        ))
    }
}

/// Escape a string for embedding in an AppleScript double-quoted literal.
pub fn escape_applescript_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Parse helper viewport stdout: `1080x1920`, `1080 1920`, or `1080 1920 2.0`.
pub fn parse_viewport_stdout(raw: &str) -> Option<Viewport> {
    let line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())?;
    if let Some((w, h)) = line.split_once('x') {
        let width: u32 = w.trim().parse().ok()?;
        let height: u32 = h.trim().parse().ok()?;
        return Some(Viewport::new(width, height, 1.0));
    }
    let mut parts = line.split_whitespace();
    let width: u32 = parts.next()?.parse().ok()?;
    let height: u32 = parts.next()?.parse().ok()?;
    let scale: f64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    Some(Viewport::new(width, height, scale))
}

/// Resolve the preferred XCUI bridge.
///
/// Order (honesty: first ready backend wins):
/// 1. [`IOS_XCUI_BUNDLE_ENV`] override (explicit path always wins when a file)
/// 2. In-tree XCUITest runner [`XCODE_HELPER_BIN_NAME`] when built
/// 3. Discovered Rust [`HELPER_BIN_NAME`] (`CARGO_BIN_EXE_*` / exe dir / PATH)
/// 4. AppleScript when [`IOS_ALLOW_APPLESCRIPT_ENV`]`=1`
/// 5. [`MissingXcuiBridge`]
pub fn resolve_xcui_bridge() -> Box<dyn XcuiBridge> {
    if let Some(bundle) = BundleXcuiBridge::from_env() {
        if bundle.ready() {
            return Box::new(bundle);
        }
    }
    if let Some(path) = discover_preferred_helper() {
        let bundle = BundleXcuiBridge::new(path);
        if bundle.ready() {
            return Box::new(bundle);
        }
    }
    if let Some(ascript) = AppleScriptXcuiBridge::try_new() {
        return Box::new(ascript);
    }
    Box::new(MissingXcuiBridge)
}

/// Whether env claims an XCUI helper path (may still be missing on disk).
pub fn xcui_bundle_env_set() -> bool {
    std::env::var(IOS_XCUI_BUNDLE_ENV)
        .ok()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Path used by the active bundle bridge, if any.
pub fn resolved_bundle_path() -> Option<PathBuf> {
    env_ios_xcui_bundle()
}

/// Document helper path for errors when env is set but file missing.
pub fn bundle_path_status() -> String {
    match std::env::var(IOS_XCUI_BUNDLE_ENV) {
        Ok(p) if !p.trim().is_empty() => {
            let path = Path::new(p.trim());
            if path.is_file() {
                format!("helper ok ({})", path.display())
            } else {
                format!("helper path not a file ({})", path.display())
            }
        }
        _ => format!("{IOS_XCUI_BUNDLE_ENV} unset"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_bridge_fail_loud_code() {
        let b = MissingXcuiBridge;
        assert!(!b.ready());
        let err = b.tap("UDID", 1, 2).unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_IOS_XCUI_UNAVAILABLE)
        );
        assert_eq!(err.status_code(), 501);
        assert!(err.to_string().contains(IOS_XCUI_BUNDLE_ENV));
    }

    #[test]
    fn build_tap_argv_is_stable() {
        let argv = BundleXcuiBridge::build_argv(
            "tap",
            "AAAA",
            &[("--x", "10".into()), ("--y", "20".into())],
        );
        assert_eq!(
            argv,
            vec!["tap", "--udid", "AAAA", "--x", "10", "--y", "20"]
        );
    }

    #[test]
    fn parse_viewport_forms() {
        let a = parse_viewport_stdout("1080x1920\n").unwrap();
        assert_eq!((a.width, a.height), (1080, 1920));
        let b = parse_viewport_stdout("  750 1334 2.0 \n").unwrap();
        assert_eq!((b.width, b.height), (750, 1334));
        assert!((b.dpr - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn applescript_escape_quotes() {
        assert_eq!(escape_applescript_string(r#"a"b\c"#), r#"a\"b\\c"#);
        let script = AppleScriptXcuiBridge::script_text(r#"hi "there""#);
        assert!(script.contains(r#"keystroke "hi \"there\"""#));
    }

    #[test]
    fn script_tap_mentions_simulator() {
        let s = AppleScriptXcuiBridge::script_tap(100, 200);
        assert!(s.contains("Simulator"));
        assert!(s.contains("{100, 200}"));
    }

    #[test]
    fn applescript_viewport_always_unavailable() {
        // Construct without env gate by forging path — ready() still needs env.
        let bridge = AppleScriptXcuiBridge {
            osascript: PathBuf::from("/usr/bin/osascript"),
        };
        let err = bridge.viewport("x").unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::MOBILE_IOS_XCUI_UNAVAILABLE)
        );
    }
}
