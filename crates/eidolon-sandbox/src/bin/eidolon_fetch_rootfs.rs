//! Fetch / resolve the pinned published Ext4 rootfs release asset.
//!
//! ```text
//! cargo run -p eidolon-sandbox --features sandbox-rootfs-pack \
//!   --bin eidolon-fetch-rootfs --locked
//! ```
//!
//! Resolution: `EIDOLON_ROOTFS_IMG` → `EIDOLON_ROOTFS_ASSET_URL`+`SHA256` →
//! durable cache → checkout pin fetch (when published). Until the first
//! one-shot GH publish **or** an env pin, this fails loud. See
//! `docs/guides/gh-ext4-release.md`.

use eidolon_sandbox::unikernel_pack::{
    effective_release_pin, ensure_rootfs_release, resolve_rootfs_release, rootfs_release_cache_dir,
    ROOTFS_ASSET_SHA256_ENV, ROOTFS_ASSET_URL_ENV, ROOTFS_IMG_ENV, ROOTFS_RELEASE_CACHE_ENV,
    ROOTFS_RELEASE_FILENAME, ROOTFS_RELEASE_PUBLISHED, ROOTFS_RELEASE_SOURCE,
    ROOTFS_RELEASE_VERSION,
};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eidolon-fetch-rootfs: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> eidolon_core::Result<()> {
    let mut fetch = true;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "--resolve-only" => fetch = false,
            other => {
                return Err(eidolon_core::error::PhenoError::BadRequest(format!(
                    "unknown argument {other:?} — try --help"
                )));
            }
        }
        i += 1;
    }

    eprintln!(
        "eidolon-fetch-rootfs: pin version={ROOTFS_RELEASE_VERSION} published={ROOTFS_RELEASE_PUBLISHED}\n  \
         source: {ROOTFS_RELEASE_SOURCE}\n  filename: {ROOTFS_RELEASE_FILENAME}"
    );
    match effective_release_pin() {
        Ok(Some(pin)) => {
            eprintln!("  active:  {} (env_pin={})", pin.filename, pin.from_env);
        }
        Ok(None) => {
            eprintln!(
                "  active:  <none — set {ROOTFS_ASSET_URL_ENV}+{ROOTFS_ASSET_SHA256_ENV} or publish>"
            );
        }
        Err(e) => return Err(e),
    }
    if let Ok(cache) = rootfs_release_cache_dir() {
        eprintln!("  cache:  {}", cache.display());
    }

    let path = if fetch {
        ensure_rootfs_release()?
    } else {
        resolve_rootfs_release()?
    };
    eprintln!("eidolon-fetch-rootfs: ok\n  image: {}", path.display());
    Ok(())
}

fn print_help() {
    eprintln!(
        "\
eidolon-fetch-rootfs — resolve / fetch pinned GH Ext4 rootfs release asset

USAGE:
  eidolon-fetch-rootfs [--resolve-only]

OPTIONS:
  --resolve-only   no network; env + cache only
  -h, --help       this help

ENV:
  EIDOLON_ROOTFS_IMG              BYO local Ext4 (wins; no SHA check)
  EIDOLON_ROOTFS_ASSET_URL        operator pin URL (with SHA256)
  EIDOLON_ROOTFS_ASSET_SHA256     expected hex digest (64 chars)
  EIDOLON_ROOTFS_RELEASE_CACHE    durable cache root (not /tmp)

Honesty: checkout pin defaults unpublished (no invented release asset).
After a real `gh release upload`, either fill release-manifest.json + Rust
constants, or export {ROOTFS_ASSET_URL_ENV}+{ROOTFS_ASSET_SHA256_ENV}.
Fail-loud: EIDOLON_SANDBOX_ROOTFS_RELEASE_UNAVAILABLE.
See docs/guides/gh-ext4-release.md.

({ROOTFS_IMG_ENV} / {ROOTFS_ASSET_URL_ENV} / {ROOTFS_RELEASE_CACHE_ENV})"
    );
}
