//! Integration checks for the bundled `eidolon-xcui-helper` binary.
//!
//! Requires `--features mobile-xcui-helper` so Cargo sets
//! `CARGO_BIN_EXE_eidolon-xcui-helper`.

#![cfg(feature = "mobile-xcui-helper")]

use eidolon_mobile::{discover_bundled_helper, HELPER_BIN_NAME};
use std::path::PathBuf;
use std::process::Command;

fn helper_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_eidolon-xcui-helper")
        .map(PathBuf::from)
        .or_else(discover_bundled_helper)
        .unwrap_or_else(|| {
            panic!(
                "expected CARGO_BIN_EXE_eidolon-xcui-helper or discoverable \
                 {HELPER_BIN_NAME} when feature mobile-xcui-helper is on"
            )
        })
}

#[test]
fn helper_bin_reports_version() {
    let bin = helper_bin();
    assert!(bin.is_file(), "missing {}", bin.display());
    let out = Command::new(&bin)
        .arg("--version")
        .output()
        .expect("spawn helper");
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(HELPER_BIN_NAME), "{stdout}");
}

#[test]
fn helper_bin_fail_loud_without_simulator() {
    let bin = helper_bin();
    let out = Command::new(&bin)
        .args([
            "tap",
            "--udid",
            "00000000-DEAD-BEEF-0000-000000000000",
            "--x",
            "1",
            "--y",
            "2",
        ])
        .output()
        .expect("spawn helper");
    assert!(!out.status.success(), "must not succeed without Simulator");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Simulator")
            || stderr.contains("simctl")
            || stderr.contains("xcrun")
            || stderr.contains("macOS-only")
            || stderr.contains("Xcode"),
        "unexpected stderr={stderr}"
    );
}

#[test]
fn helper_bin_bad_argv_exits_2() {
    let bin = helper_bin();
    let out = Command::new(&bin).args(["tap"]).output().expect("spawn");
    assert_eq!(out.status.code(), Some(2));
}
