//! Fetch pinned Appium UiAutomator2 server + instrumentation APKs.
//!
//! Downloads official GitHub release artifacts into a durable cache
//! (`EIDOLON_UIA2_CACHE` or `~/.cache/eidolon/uia2/<version>/`), verifies
//! SHA-256, and refuses `/tmp`. See `crates/eidolon-mobile/assets/uia2/`.
//!
//! ```text
//! cargo run -p eidolon-mobile --features mobile-uia2 --bin eidolon-fetch-uia2 --locked
//! ```

use eidolon_mobile::uia2_assets::{
    durable_cache_dir, fetch_into, resolve, UIA2_PINNED_VERSION, UIA2_RELEASE_SOURCE,
};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eidolon-fetch-uia2: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> eidolon_core::Result<()> {
    let cache = durable_cache_dir()?;
    eprintln!(
        "eidolon-fetch-uia2: fetching Appium UiAutomator2 {UIA2_PINNED_VERSION}\n  source: {UIA2_RELEASE_SOURCE}\n  cache:  {}",
        cache.display()
    );
    fetch_into(&cache)?;
    let paths = resolve()?;
    eprintln!(
        "eidolon-fetch-uia2: ok\n  server: {}\n  test:   {}",
        paths.server.display(),
        paths.test.display()
    );
    Ok(())
}
