//! Optional live XCUITest / xcodebuild checks.
//!
//! Hermetic suite never requires Xcode. Enable with:
//! `EIDOLON_XCUI_XCODE_INTEGRATION=1 cargo test -p eidolon-mobile -- --ignored`

use eidolon_mobile::{
    discover_xcode_helper, xcode_project_dir, xcui_xcode_integration_enabled, XCODE_HELPER_BIN_NAME,
};
use std::process::Command;

fn gated() -> bool {
    xcui_xcode_integration_enabled()
}

#[test]
#[ignore = "requires EIDOLON_XCUI_XCODE_INTEGRATION=1 + built XCUI project + Booted Simulator"]
fn xcode_runner_viewport_smoke() {
    if !gated() {
        eprintln!(
            "skip: set EIDOLON_XCUI_XCODE_INTEGRATION=1 after \
             `swift native/ios/EidolonXcuiHelper/Scripts/build-for-testing.swift`"
        );
        return;
    }
    let runner = discover_xcode_helper().unwrap_or_else(|| {
        panic!(
            "expected built {XCODE_HELPER_BIN_NAME} under {:?}",
            xcode_project_dir()
        )
    });
    let udid = std::env::var("EIDOLON_MOBILE_DEVICE").unwrap_or_else(|_| {
        panic!("EIDOLON_MOBILE_DEVICE (booted Simulator UDID) required for integration")
    });
    let out = Command::new(&runner)
        .args(["viewport", "--udid", &udid])
        .output()
        .expect("spawn xcode runner");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.split_whitespace().count() >= 2,
        "expected WIDTH HEIGHT [SCALE], got {stdout:?}"
    );
}
