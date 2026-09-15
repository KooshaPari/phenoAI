//! Bundled XCUI helper binary — argv contract for [`eidolon_mobile::BundleXcuiBridge`].
//!
//! Build: `cargo build -p eidolon-mobile --features mobile-xcui-helper --bin eidolon-xcui-helper`
//!
//! See `docs/guides/ios-xcui-helper.md`. Do not unarchive kmobile.

use eidolon_mobile::ios::xcui_helper;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = xcui_helper::run_main(&args);
    std::process::exit(code);
}
