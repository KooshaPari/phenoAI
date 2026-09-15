//! In-repo XCUI helpers — speak the [`super::BundleXcuiBridge`] contract.
//!
//! # Honesty
//!
//! Prefer the **in-tree Xcode XCUITest** runner when built
//! (`native/ios/EidolonXcuiHelper/build/eidolon-xcui-xctest`). The Rust binary
//! `eidolon-xcui-helper` remains a best-effort AppleScript/simctl **fallback**:
//! - verifies the UDID via `simctl` when available
//! - attempts Simulator input via `osascript` (screen points)
//! - resolves viewport via `simctl` device-type heuristics when possible
//! - otherwise **fails loud**
//!
//! Non-macOS builds always fail loud. Do not unarchive kmobile.

mod actions;

use std::path::{Path, PathBuf};

pub use actions::{execute, HelperOutput};

use crate::cli::which_bin;

/// Binary / discovery name for the bundled Rust helper (AppleScript fallback).
pub const HELPER_BIN_NAME: &str = "eidolon-xcui-helper";

/// Discoverable name for the in-tree XCUITest argv runner (preferred when built).
pub const XCODE_HELPER_BIN_NAME: &str = "eidolon-xcui-xctest";

/// Cargo sets this when building the binary for integration tests.
pub const CARGO_BIN_EXE_ENV: &str = "CARGO_BIN_EXE_eidolon-xcui-helper";

/// Optional override for the Xcode project directory (`EIDOLON_IOS_XCUI_XCODE_DIR`).
pub const XCODE_PROJECT_DIR_ENV: &str = "EIDOLON_IOS_XCUI_XCODE_DIR";

/// Gate for live `xcodebuild` integration tests (`EIDOLON_XCUI_XCODE_INTEGRATION=1`).
pub const XCUI_XCODE_INTEGRATION_ENV: &str = "EIDOLON_XCUI_XCODE_INTEGRATION";

/// Relative path from the `eidolon-mobile` crate manifest to the Xcode project.
pub const XCODE_PROJECT_REL: &str = "../../native/ios/EidolonXcuiHelper";

/// Parsed helper command (matches [`super::BundleXcuiBridge::build_argv`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperCommand {
    Tap {
        udid: String,
        x: i32,
        y: i32,
    },
    Swipe {
        udid: String,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
    },
    Text {
        udid: String,
        value: String,
    },
    Viewport {
        udid: String,
    },
}

impl HelperCommand {
    /// UDID shared by all ops.
    pub fn udid(&self) -> &str {
        match self {
            Self::Tap { udid, .. }
            | Self::Swipe { udid, .. }
            | Self::Text { udid, .. }
            | Self::Viewport { udid } => udid,
        }
    }

    /// Op name as used on argv (`tap` / `swipe` / `text` / `viewport`).
    pub fn op_name(&self) -> &'static str {
        match self {
            Self::Tap { .. } => "tap",
            Self::Swipe { .. } => "swipe",
            Self::Text { .. } => "text",
            Self::Viewport { .. } => "viewport",
        }
    }
}

/// Parse helper argv (without program name).
///
/// Expected forms:
/// - `tap --udid <id> --x <n> --y <n>`
/// - `swipe --udid <id> --x1 <n> --y1 <n> --x2 <n> --y2 <n>`
/// - `text --udid <id> --value <str>`
/// - `viewport --udid <id>`
pub fn parse_argv(args: &[String]) -> Result<HelperCommand, String> {
    if args.is_empty() {
        return Err(usage_err("missing op (tap|swipe|text|viewport)"));
    }
    let op = args[0].as_str();
    let flags = parse_flags(&args[1..])?;
    let udid = require_flag(&flags, "--udid")?;
    match op {
        "tap" => {
            let x: i32 = require_flag(&flags, "--x")?
                .parse()
                .map_err(|_| "--x must be an integer".to_string())?;
            let y: i32 = require_flag(&flags, "--y")?
                .parse()
                .map_err(|_| "--y must be an integer".to_string())?;
            Ok(HelperCommand::Tap { udid, x, y })
        }
        "swipe" => {
            let x1: i32 = require_flag(&flags, "--x1")?
                .parse()
                .map_err(|_| "--x1 must be an integer".to_string())?;
            let y1: i32 = require_flag(&flags, "--y1")?
                .parse()
                .map_err(|_| "--y1 must be an integer".to_string())?;
            let x2: i32 = require_flag(&flags, "--x2")?
                .parse()
                .map_err(|_| "--x2 must be an integer".to_string())?;
            let y2: i32 = require_flag(&flags, "--y2")?
                .parse()
                .map_err(|_| "--y2 must be an integer".to_string())?;
            Ok(HelperCommand::Swipe {
                udid,
                x1,
                y1,
                x2,
                y2,
            })
        }
        "text" => {
            let value = require_flag(&flags, "--value")?;
            Ok(HelperCommand::Text { udid, value })
        }
        "viewport" => Ok(HelperCommand::Viewport { udid }),
        "help" | "--help" | "-h" => Err(usage_err("help")),
        other => Err(usage_err(format!("unknown op `{other}`"))),
    }
}

fn usage_err(detail: impl std::fmt::Display) -> String {
    format!(
        "{detail}\n\
         usage: {HELPER_BIN_NAME} tap|swipe|text|viewport --udid <id> …\n\
         contract matches BundleXcuiBridge; prefer in-tree XCUITest runner \
         `{XCODE_HELPER_BIN_NAME}` (native/ios/EidolonXcuiHelper) when built; \
         this Rust binary is best-effort Simulator AppleScript / simctl only"
    )
}

fn parse_flags(args: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let key = &args[i];
        if !key.starts_with("--") {
            return Err(format!("unexpected positional `{key}`"));
        }
        i += 1;
        let Some(val) = args.get(i) else {
            return Err(format!("flag `{key}` requires a value"));
        };
        out.push((key.clone(), val.clone()));
        i += 1;
    }
    Ok(out)
}

fn require_flag(flags: &[(String, String)], key: &str) -> Result<String, String> {
    flags
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| format!("missing required flag `{key}`"))
}

/// Resolve the in-tree Xcode project directory (does not require a build).
///
/// Order:
/// 1. [`XCODE_PROJECT_DIR_ENV`] when set and exists
/// 2. `CARGO_MANIFEST_DIR`/`../../native/ios/EidolonXcuiHelper`
pub fn xcode_project_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(XCODE_PROJECT_DIR_ENV) {
        let path = PathBuf::from(p.trim());
        if path.is_dir() {
            return Some(path);
        }
    }
    let from_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(XCODE_PROJECT_REL);
    from_manifest.is_dir().then_some(from_manifest)
}

/// `true` when the Xcode project has been built for testing (stamp present).
pub fn xcode_helper_built(project_dir: &Path) -> bool {
    project_dir
        .join("build")
        .join(".eidolon-xcui-built")
        .is_file()
}

/// Discover the preferred in-tree XCUITest argv runner when built.
///
/// Looks for `build/eidolon-xcui-xctest` under [`xcode_project_dir`] only when
/// `build/.eidolon-xcui-built` exists (or the runner file is present next to a
/// built `.app`). Never invents a path into `/tmp`.
pub fn discover_xcode_helper() -> Option<PathBuf> {
    let root = xcode_project_dir()?;
    let runner = root.join("build").join(XCODE_HELPER_BIN_NAME);
    if !runner.is_file() {
        return None;
    }
    if xcode_helper_built(&root) {
        return Some(runner);
    }
    // Stamp missing: still accept runner if host .app was produced under build/.
    let products = root.join("build").join("Build").join("Products");
    if products.is_dir() {
        return Some(runner);
    }
    None
}

/// Discover bundled **Rust** helper path (env override is handled separately and wins).
///
/// Order:
/// 1. `CARGO_BIN_EXE_eidolon-xcui-helper` (cargo integration tests)
/// 2. Sibling of `current_exe()` named [`HELPER_BIN_NAME`]
/// 3. `PATH` via `which`
pub fn discover_bundled_helper() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(CARGO_BIN_EXE_ENV) {
        let path = PathBuf::from(p.trim());
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(found) = helper_in_dir(dir) {
                return Some(found);
            }
        }
    }
    which_bin(HELPER_BIN_NAME)
}

/// Prefer Xcode XCUITest runner when built, else Rust AppleScript helper.
pub fn discover_preferred_helper() -> Option<PathBuf> {
    discover_xcode_helper().or_else(discover_bundled_helper)
}

/// `true` when live xcodebuild integration tests are explicitly enabled.
pub fn xcui_xcode_integration_enabled() -> bool {
    std::env::var(XCUI_XCODE_INTEGRATION_ENV).ok().as_deref() == Some("1")
}

fn helper_in_dir(dir: &Path) -> Option<PathBuf> {
    let unix = dir.join(HELPER_BIN_NAME);
    if unix.is_file() {
        return Some(unix);
    }
    let windows = dir.join(format!("{HELPER_BIN_NAME}.exe"));
    windows.is_file().then_some(windows)
}

/// CLI entry used by the binary (and tests): parse + execute + print.
pub fn run_main(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("--version") {
        println!("{HELPER_BIN_NAME} {}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let cmd = match parse_argv(args) {
        Ok(c) => c,
        Err(e) => {
            // help request → exit 0 with usage
            if args.first().map(|s| s.as_str()) == Some("help")
                || args.first().map(|s| s.as_str()) == Some("--help")
                || args.first().map(|s| s.as_str()) == Some("-h")
            {
                eprintln!("{e}");
                return 0;
            }
            eprintln!("error: {e}");
            return 2;
        }
    };
    match execute(&cmd) {
        Ok(out) => {
            if !out.stdout.is_empty() {
                println!("{}", out.stdout.trim_end());
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ios::xcui_bridge::BundleXcuiBridge;

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn parse_matches_bundle_tap_argv() {
        let argv = BundleXcuiBridge::build_argv(
            "tap",
            "AAAA-BBBB",
            &[("--x", "10".into()), ("--y", "20".into())],
        );
        let cmd = parse_argv(&argv).expect("parse");
        assert_eq!(
            cmd,
            HelperCommand::Tap {
                udid: "AAAA-BBBB".into(),
                x: 10,
                y: 20,
            }
        );
    }

    #[test]
    fn parse_matches_bundle_swipe_argv() {
        let argv = BundleXcuiBridge::build_argv(
            "swipe",
            "UDID",
            &[
                ("--x1", "1".into()),
                ("--y1", "2".into()),
                ("--x2", "3".into()),
                ("--y2", "4".into()),
            ],
        );
        let cmd = parse_argv(&argv).unwrap();
        assert_eq!(cmd.op_name(), "swipe");
        assert_eq!(cmd.udid(), "UDID");
    }

    #[test]
    fn parse_text_and_viewport() {
        let text = parse_argv(&s(&["text", "--udid", "U", "--value", "hello"])).unwrap();
        assert_eq!(
            text,
            HelperCommand::Text {
                udid: "U".into(),
                value: "hello".into(),
            }
        );
        let vp = parse_argv(&s(&["viewport", "--udid", "U"])).unwrap();
        assert_eq!(vp, HelperCommand::Viewport { udid: "U".into() });
    }

    #[test]
    fn parse_missing_flag_fails() {
        let err = parse_argv(&s(&["tap", "--udid", "U", "--x", "1"])).unwrap_err();
        assert!(err.contains("--y"), "{err}");
    }

    #[test]
    fn parse_unknown_op_fails() {
        let err = parse_argv(&s(&["pinch", "--udid", "U"])).unwrap_err();
        assert!(err.contains("unknown op"), "{err}");
    }

    #[test]
    fn execute_fake_udid_fails_loud() {
        let cmd = HelperCommand::Tap {
            udid: "00000000-DEAD-BEEF-0000-000000000000".into(),
            x: 1,
            y: 2,
        };
        let err = execute(&cmd).unwrap_err();
        assert!(
            err.contains("Simulator")
                || err.contains("simctl")
                || err.contains("xcrun")
                || err.contains("macOS-only")
                || err.contains("Xcode"),
            "unexpected err={err}"
        );
    }

    #[test]
    fn run_main_bad_argv_exit_2() {
        assert_eq!(run_main(&s(&["tap"])), 2);
    }

    #[test]
    fn discover_respects_cargo_bin_exe_when_file() {
        let prev = std::env::var_os(CARGO_BIN_EXE_ENV);
        std::env::set_var(CARGO_BIN_EXE_ENV, "/nonexistent/eidolon-xcui-helper-xyz");
        let _ = discover_bundled_helper();
        match prev {
            Some(v) => std::env::set_var(CARGO_BIN_EXE_ENV, v),
            None => std::env::remove_var(CARGO_BIN_EXE_ENV),
        }
    }

    #[test]
    fn helper_bin_name_stable() {
        assert_eq!(HELPER_BIN_NAME, "eidolon-xcui-helper");
        assert_eq!(XCODE_HELPER_BIN_NAME, "eidolon-xcui-xctest");
    }

    #[test]
    fn xcode_project_dir_resolves_in_tree() {
        let _guard = super::XCODE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os(XCODE_PROJECT_DIR_ENV);
        std::env::remove_var(XCODE_PROJECT_DIR_ENV);
        let dir = xcode_project_dir().expect("native/ios/EidolonXcuiHelper present");
        assert!(
            dir.join("EidolonXcuiHelper.xcodeproj").is_dir() || dir.join("project.yml").is_file(),
            "expected xcodeproj or project.yml under {}",
            dir.display()
        );
        match prev {
            Some(v) => std::env::set_var(XCODE_PROJECT_DIR_ENV, v),
            None => std::env::remove_var(XCODE_PROJECT_DIR_ENV),
        }
    }

    #[test]
    fn discover_xcode_helper_respects_stamp_via_env_dir() {
        let _guard = super::XCODE_DIR_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp =
            std::env::temp_dir().join(format!("eidolon-xcui-discovery-{}", std::process::id()));
        let build = tmp.join("build");
        std::fs::create_dir_all(&build).expect("mkdir");
        let runner = build.join(XCODE_HELPER_BIN_NAME);
        let stamp = build.join(".eidolon-xcui-built");
        std::fs::write(&runner, b"#!/bin/sh\nexit 0\n").expect("runner");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&runner).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&runner, perms).unwrap();
        }

        let prev = std::env::var_os(XCODE_PROJECT_DIR_ENV);
        std::env::set_var(XCODE_PROJECT_DIR_ENV, &tmp);

        assert!(discover_xcode_helper().is_none());

        std::fs::write(&stamp, b"test\n").expect("stamp");
        let found = discover_xcode_helper();
        assert_eq!(found.as_deref(), Some(runner.as_path()));

        match prev {
            Some(v) => std::env::set_var(XCODE_PROJECT_DIR_ENV, v),
            None => std::env::remove_var(XCODE_PROJECT_DIR_ENV),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn xcui_xcode_integration_env_default_off() {
        let _ = XCUI_XCODE_INTEGRATION_ENV;
        let _ = xcui_xcode_integration_enabled();
    }
}

#[cfg(test)]
static XCODE_DIR_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
