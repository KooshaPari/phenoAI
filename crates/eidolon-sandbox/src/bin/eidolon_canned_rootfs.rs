//! Build a canned rootfs tree (+ optional Ext4 disk) with `eidolon-vsock-agent`.
//!
//! ```text
//! # Hermetic tree-only (macOS-safe; stub tree + agent bake):
//! cargo run -p eidolon-sandbox --bin eidolon-canned-rootfs --locked -- \
//!   --agent "$EIDOLON_VSOCK_AGENT" --mode tree_only
//!
//! # Live Ext4 (Linux + tools + ROOTFS_PACK_INTEGRATION):
//! ROOTFS_PACK_INTEGRATION=1 cargo run -p eidolon-sandbox \
//!   --features sandbox-rootfs-pack --bin eidolon-canned-rootfs --locked -- \
//!   --agent target/x86_64-unknown-linux-musl/release/eidolon-vsock-agent \
//!   --mode pack_ext4
//! ```
//!
//! Output defaults to `target/canned-rootfs/` (override `EIDOLON_CANNED_ROOTFS_OUT`).
//! See `docs/guides/canned-rootfs.md`.

use eidolon_sandbox::unikernel_pack::{
    build_canned_rootfs, CannedMode, CannedRootfsRequest, AGENT_PATH_ENV, CANNED_ROOTFS_OUT_ENV,
};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("eidolon-canned-rootfs: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> eidolon_core::Result<()> {
    let mut mode = CannedMode::TreeOnly;
    let mut agent: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut base_tree: Option<PathBuf> = None;
    let mut docker_ref: Option<String> = None;
    let mut systemd = true;
    let mut allow_fetch = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "--mode" => {
                i += 1;
                mode = parse_mode(args.get(i).map(String::as_str).unwrap_or(""))?;
            }
            "--agent" => {
                i += 1;
                agent = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| bad("--agent requires a path"))?,
                ));
            }
            "--out" => {
                i += 1;
                out_dir = Some(PathBuf::from(
                    args.get(i).ok_or_else(|| bad("--out requires a path"))?,
                ));
            }
            "--base-tree" => {
                i += 1;
                base_tree = Some(PathBuf::from(
                    args.get(i)
                        .ok_or_else(|| bad("--base-tree requires a path"))?,
                ));
            }
            "--docker-ref" => {
                i += 1;
                docker_ref = Some(
                    args.get(i)
                        .ok_or_else(|| bad("--docker-ref requires an image/container"))?
                        .clone(),
                );
            }
            "--no-systemd" => systemd = false,
            "--fetch-agent" => allow_fetch = true,
            other => {
                return Err(bad(format!(
                    "unknown argument {other:?} — try --help"
                )));
            }
        }
        i += 1;
    }

    let mut req = CannedRootfsRequest::new()
        .with_mode(mode)
        .with_systemd_unit(systemd)
        .with_allow_agent_fetch(allow_fetch);
    if let Some(a) = agent {
        req = req.with_agent_bin(a);
    }
    if let Some(o) = out_dir {
        req = req.with_out_dir(o);
    }
    if let Some(t) = base_tree {
        req = req.with_base_tree(t);
    }
    if let Some(d) = docker_ref {
        req = req.with_docker_ref(d);
    }

    eprintln!(
        "eidolon-canned-rootfs: mode={} (agent via --agent / {AGENT_PATH_ENV} / pin; \
         out via --out / {CANNED_ROOTFS_OUT_ENV})",
        mode.as_str()
    );
    let result = build_canned_rootfs(&req)?;
    eprintln!(
        "eidolon-canned-rootfs: ok\n  mode:  {}\n  out:   {}\n  tree:  {}\n  agent: {}",
        result.mode.as_str(),
        result.out_dir.display(),
        result.rootfs_tree.display(),
        result.bake.agent_guest_path.display()
    );
    if let Some(img) = &result.rootfs_img {
        eprintln!("  image: {}", img.display());
    } else {
        eprintln!(
            "  image: (tree-only — not Ext4; use --mode pack_ext4 on Linux with \
             ROOTFS_PACK_INTEGRATION=1)"
        );
    }
    Ok(())
}

fn parse_mode(s: &str) -> eidolon_core::Result<CannedMode> {
    match s {
        "tree_only" | "tree" => Ok(CannedMode::TreeOnly),
        "pack_ext4" | "ext4" => Ok(CannedMode::PackExt4),
        "docker_to_ext4" | "docker" => Ok(CannedMode::DockerToExt4),
        other => Err(bad(format!(
            "unknown --mode {other:?} (tree_only|pack_ext4|docker_to_ext4)"
        ))),
    }
}

fn bad(msg: impl Into<String>) -> eidolon_core::error::PhenoError {
    eidolon_core::error::PhenoError::BadRequest(msg.into())
}

fn print_help() {
    eprintln!(
        "\
eidolon-canned-rootfs — canned guest rootfs with eidolon-vsock-agent

USAGE:
  eidolon-canned-rootfs [OPTIONS]

OPTIONS:
  --mode <tree_only|pack_ext4|docker_to_ext4>  default: tree_only
  --agent <path>       host eidolon-vsock-agent binary
  --out <dir>          durable output (default: target/canned-rootfs)
  --base-tree <dir>    existing rootfs tree (else stub fixture)
  --docker-ref <ref>   image/container for docker_to_ext4
  --no-systemd         skip systemd unit bake
  --fetch-agent        allow EIDOLON_VSOCK_AGENT_URL+SHA256 pin fetch
  -h, --help           this help

ENV:
  EIDOLON_CANNED_ROOTFS_OUT   durable out dir (not /tmp)
  EIDOLON_VSOCK_AGENT         host agent binary
  EIDOLON_VSOCK_AGENT_URL     optional pin URL (with SHA256)
  EIDOLON_VSOCK_AGENT_SHA256  expected hex digest
  ROOTFS_PACK_INTEGRATION=1   required for pack_ext4 / docker_to_ext4

Honesty: tree_only is macOS-safe. Ext4 release disks need Linux tools + musl
agent. Versioned GH release staging: eidolon-release-rootfs /
docs/guides/gh-ext4-release.md (default pin unpublished until one-shot publish).
See docs/guides/canned-rootfs.md."
    );
}
