// SPDX-License-Identifier: MIT OR Apache-2.0
// Build script for nvms-ffi
// Links to the nanovms C staticlib when available, falls back to the Rust shim.

use std::path::Path;

fn main() {
    // Register custom cfg so rustc doesn't warn.
    println!("cargo::rustc-check-cfg=cfg(nvms_real_ffi)");

    // Determine the path to the nanovms build output directory.
    // The nanovms repo lives as a sibling of PhenoCompose under the
    // same parent (e.g. ../../repos/nanovms/build/).
    // CARGO_MANIFEST_DIR is <repo-root>/bindings/rust-ffi.
    let manifest_dir = Path::new(std::env!("CARGO_MANIFEST_DIR"));
    let nanovms_build = manifest_dir
        .join("..") // bindings/
        .join("..") // repo root (PhenoCompose)
        .join("..") // repos/
        .join("nanovms")
        .join("build");
    let default_path = nanovms_build.to_str().unwrap_or("").to_string();

    // Also check a canonical absolute path via NANOVMS_BUILD env var.
    let env_path: Option<String> = std::env::var("NANOVMS_BUILD").ok();
    let search_dirs: Vec<String> = env_path
        .into_iter()
        .chain(std::iter::once(default_path))
        .filter(|p| !p.is_empty())
        .collect();

    // T09: also check the new cgo-shim header location
    let go_lib_path = manifest_dir.join("../../nanovms/cgo-shim/nvms_export.h");
    if go_lib_path.exists() {
        println!("cargo:rerun-if-changed={}", go_lib_path.display());
    }

    // Try to locate libnvms_core.a in each candidate directory.
    let found_staticlib = search_dirs.iter().any(|dir| {
        let candidate = Path::new(dir).join("libnvms_core.a");
        if candidate.exists() {
            println!("cargo:rustc-link-search=native={}", Path::new(dir).display());
            println!("cargo:rustc-link-lib=static=nvms_core");
            println!("cargo:rerun-if-changed={}", candidate.display());
            true
        } else {
            false
        }
    });

    if found_staticlib {
        // Signal to lib.rs that the real C symbols will be resolved at link
        // time, so the Rust fallback shim must be omitted.
        println!("cargo:rustc-cfg=nvms_real_ffi");
        println!("cargo:info=staticlib found, linking to libnvms_core.a");
    } else {
        // Fall back to the pure-Rust shim (see `mod shim` in lib.rs).
        // This lets the crate compile + test offline without the Go toolchain.
        println!("cargo:info=no staticlib found, using Rust FFI shim");
    }

    // Check that the Go C header exists (informational / re-build trigger).
    let header = manifest_dir.join("../go-c-export/nvms_core.h");
    if header.exists() {
        println!("cargo:rerun-if-changed={}", header.display());
    }

    // Re-run build.rs when lib.rs changes (the shim is defined there).
    println!("cargo:rerun-if-changed=src/lib.rs");
}
