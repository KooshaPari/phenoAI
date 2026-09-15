//! Stage a versioned Ext4 rootfs + SHA-256 for `gh release upload`.
//!
//! ```text
//! # From an existing canned rootfs.img (after Linux pack):
//! cargo run -p eidolon-sandbox --features sandbox-rootfs-pack \
//!   --bin eidolon-release-rootfs --locked -- \
//!   --from-img target/canned-rootfs/rootfs.img --version 0.1.0
//!
//! # Build canned PackExt4 then stage (Linux + ROOTFS_PACK_INTEGRATION=1):
//! ROOTFS_PACK_INTEGRATION=1 cargo run -p eidolon-sandbox \
//!   --features sandbox-rootfs-pack --bin eidolon-release-rootfs --locked -- \
//!   --build --agent "$EIDOLON_VSOCK_AGENT" --version 0.1.0
//! ```
//!
//! Does **not** call GitHub — prints `gh release upload` hints. Durable out:
//! `target/rootfs-release/` (`EIDOLON_ROOTFS_RELEASE_OUT`). See
//! `docs/guides/gh-ext4-release.md`.

use eidolon_sandbox::unikernel_pack::{
    build_and_stage_release_rootfs, gh_upload_hints, release_canned_request, stage_release_asset,
    ReleaseAssetRequest, AGENT_PATH_ENV, ROOTFS_RELEASE_OUT_ENV,
};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eidolon-release-rootfs: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> eidolon_core::Result<()> {
    let mut version: Option<String> = None;
    let mut arch = "x86_64".to_string();
    let mut from_img: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut agent: Option<PathBuf> = None;
    let mut canned_out: Option<PathBuf> = None;
    let mut build = false;
    let mut tag: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "--version" => {
                i += 1;
                version = Some(
                    args.get(i)
                        .ok_or_else(|| bad("--version requires a value"))?
                        .clone(),
                );
            }
            "--arch" => {
                i += 1;
                arch = args
                    .get(i)
                    .ok_or_else(|| bad("--arch requires a value"))?
                    .clone();
            }
            "--from-img" => {
                i += 1;
                from_img = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| bad("--from-img requires a path"))?,
                ));
            }
            "--out" => {
                i += 1;
                out_dir = Some(PathBuf::from(
                    args.get(i).ok_or_else(|| bad("--out requires a path"))?,
                ));
            }
            "--agent" => {
                i += 1;
                agent = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| bad("--agent requires a path"))?,
                ));
            }
            "--canned-out" => {
                i += 1;
                canned_out = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| bad("--canned-out requires a path"))?,
                ));
            }
            "--tag" => {
                i += 1;
                tag = Some(
                    args.get(i)
                        .ok_or_else(|| bad("--tag requires a GH release tag"))?
                        .clone(),
                );
            }
            "--build" => build = true,
            other => {
                return Err(bad(format!(
                    "unknown argument {other:?} — try --help"
                )));
            }
        }
        i += 1;
    }

    let version = version.ok_or_else(|| bad("--version is required (e.g. 0.1.0)"))?;
    let release_tag = tag.unwrap_or_else(|| {
        if version.starts_with('v') {
            version.clone()
        } else {
            format!("rootfs-v{version}")
        }
    });

    let result = if build {
        let canned = release_canned_request(agent, canned_out);
        eprintln!(
            "eidolon-release-rootfs: building canned PackExt4 then staging \
             (needs ROOTFS_PACK_INTEGRATION=1 + Linux tools + agent)"
        );
        build_and_stage_release_rootfs(&canned, &version, Some(&arch), out_dir.as_deref())?
    } else {
        let img = from_img.ok_or_else(|| {
            bad("--from-img <path> required unless --build (see --help)")
        })?;
        let mut req = ReleaseAssetRequest::new(img, &version)
            .with_arch(&arch)
            .with_checksum(true);
        if let Some(o) = out_dir {
            req = req.with_out_dir(o);
        }
        eprintln!(
            "eidolon-release-rootfs: staging versioned asset (out via --out / {ROOTFS_RELEASE_OUT_ENV})"
        );
        stage_release_asset(&req)?
    };

    eprintln!(
        "eidolon-release-rootfs: ok\n  version: {}\n  image:   {}\n  sha256:  {}\n  manifest:{}",
        result.manifest.version,
        result.image_path.display(),
        result
            .sha256_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into()),
        result.manifest_path.display()
    );
    if let Some(hex) = &result.manifest.sha256 {
        eprintln!("  digest:  sha256:{hex}");
    }
    eprintln!("\n{}", gh_upload_hints(&result, &release_tag));
    Ok(())
}

fn bad(msg: impl Into<String>) -> eidolon_core::error::PhenoError {
    eidolon_core::error::PhenoError::BadRequest(msg.into())
}

fn print_help() {
    eprintln!(
        "\
eidolon-release-rootfs — stage versioned Ext4 + SHA-256 for gh release upload

USAGE:
  eidolon-release-rootfs --version <ver> --from-img <rootfs.img> [OPTIONS]
  eidolon-release-rootfs --version <ver> --build [--agent <path>] [OPTIONS]

OPTIONS:
  --version <ver>     release version (required; e.g. 0.1.0)
  --arch <arch>       filename arch label (default: x86_64)
  --from-img <path>   existing Ext4 disk to version + checksum
  --build             run canned PackExt4 then stage (Linux + integration)
  --agent <path>      host eidolon-vsock-agent (with --build)
  --canned-out <dir>  canned pipeline out (default: target/canned-rootfs)
  --out <dir>         release staging dir (default: target/rootfs-release)
  --tag <tag>         GH release tag for printed upload hints
  -h, --help          this help

ENV:
  EIDOLON_ROOTFS_RELEASE_OUT   durable staging (not /tmp)
  EIDOLON_VSOCK_AGENT          agent binary ({AGENT_PATH_ENV})
  ROOTFS_PACK_INTEGRATION=1    required for --build

Honesty: does not publish to GitHub. Prefer manual `gh release upload`
(Actions billing may be exhausted). After upload, update
assets/canned-rootfs/release-manifest.json + pin constants.
See docs/guides/gh-ext4-release.md."
    );
}
