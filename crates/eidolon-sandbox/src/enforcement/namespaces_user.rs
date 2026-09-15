//! USER namespace uid/gid map writers.
//!
//! After `unshare(CLONE_NEWUSER)`, the kernel leaves the process with empty
//! maps until `/proc/self/{uid,gid}_map` are written. We write
//! `setgroups` deny (required before `gid_map` on modern kernels), then the
//! planned uid/gid map lines.
//!
//! Single-ID identity maps (`0 <host_euid|egid> 1`) write `/proc` directly.
//! Multi-range maps invoke `newuidmap` / `newgidmap` (shadow-utils).
//!
//! wraps: Linux `/proc/self/{setgroups,uid_map,gid_map}` (no crate; direct FS)
//! wraps: shadow-utils `newuidmap` / `newgidmap` (external; path override via env)
//!
//! Fail-loud on any write — never claim
//! [`crate::enforcement::NamespaceStatus::user_ns_mapped`] on silent skip.

use crate::codes;
use crate::enforcement::plan::UserNsIdMap;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Env override for `newuidmap` binary path.
pub const NEWUIDMAP_ENV: &str = "EIDOLON_SANDBOX_NEWUIDMAP";

/// Env override for `newgidmap` binary path.
pub const NEWGIDMAP_ENV: &str = "EIDOLON_SANDBOX_NEWGIDMAP";

/// Whether uid maps must go through `newuidmap` instead of a direct `/proc` write.
pub(crate) fn uid_maps_need_newidmap(uid_maps: &[UserNsIdMap], host_euid: u32) -> bool {
    maps_need_newidmap(uid_maps, host_euid)
}

/// Whether gid maps must go through `newgidmap` instead of a direct `/proc` write.
pub(crate) fn gid_maps_need_newidmap(gid_maps: &[UserNsIdMap], host_egid: u32) -> bool {
    maps_need_newidmap(gid_maps, host_egid)
}

/// Unprivileged direct writes only allow a single `0 <host_id> 1` identity line.
pub(crate) fn maps_need_newidmap(maps: &[UserNsIdMap], host_id: u32) -> bool {
    match maps.len() {
        0 => false,
        1 => {
            let m = &maps[0];
            m.count > 1 || m.inside != 0 || m.outside != host_id
        }
        _ => true,
    }
}

/// Write `setgroups` deny + uid/gid maps for the current process.
///
/// Typical unprivileged single-ID map: `0 <host_euid> 1` / `0 <host_egid> 1`
/// so the caller is root inside the new USER ns. Multi-range maps use
/// `newuidmap` / `newgidmap`.
pub(crate) fn write_maps(uid_maps: &[UserNsIdMap], gid_maps: &[UserNsIdMap]) -> Result<()> {
    write_proc("/proc/self/setgroups", b"deny")?;

    let euid = unsafe { libc::geteuid() };
    let egid = unsafe { libc::getegid() };

    apply_id_maps("uid", uid_maps, euid, NEWUIDMAP_ENV, "newuidmap", "/proc/self/uid_map")?;
    apply_id_maps("gid", gid_maps, egid, NEWGIDMAP_ENV, "newgidmap", "/proc/self/gid_map")?;
    Ok(())
}

fn apply_id_maps(
    kind: &str,
    maps: &[UserNsIdMap],
    host_id: u32,
    tool_env: &str,
    default_name: &str,
    proc_path: &str,
) -> Result<()> {
    if maps.is_empty() {
        return Err(PhenoError::BadRequest(format!(
            "USER ns {kind} maps are empty; refusing silent skip when user ns was requested"
        )));
    }
    if maps_need_newidmap(maps, host_id) {
        invoke_newidmap(tool_env, default_name, kind, maps)
    } else {
        write_proc(proc_path, format!("{}\n", maps[0].proc_line()).as_bytes())
    }
}

/// wraps: shadow-utils `newuidmap` / `newgidmap`
fn invoke_newidmap(
    tool_env: &str,
    default_name: &str,
    kind: &str,
    maps: &[UserNsIdMap],
) -> Result<()> {
    let tool = resolve_idmap_tool(tool_env, default_name)?;
    let pid = std::process::id().to_string();
    let mut args: Vec<String> = Vec::with_capacity(1 + maps.len() * 3);
    args.push(pid);
    for m in maps {
        args.push(m.inside.to_string());
        args.push(m.outside.to_string());
        args.push(m.count.to_string());
    }
    let output = Command::new(&tool)
        .args(&args)
        .output()
        .map_err(|e| {
            PhenoError::unsupported_platform(
                codes::SANDBOX_NAMESPACES_UNSUPPORTED,
                format!(
                    "spawn {tool:?} for USER ns {kind} map failed: {e} \
                     (install shadow-utils or set {tool_env})"
                ),
            )
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_NAMESPACES_UNSUPPORTED,
        format!(
            "{tool:?} for USER ns {kind} map exited {}: {stderr} \
             (check /etc/sub{kind} delegation or map env; refusing silent skip)",
            output.status
        ),
    ))
}

fn resolve_idmap_tool(env_var: &str, default_name: &str) -> Result<PathBuf> {
    if let Ok(p) = std::env::var(env_var) {
        let p = p.trim();
        if !p.is_empty() {
            let path = PathBuf::from(p);
            if path.is_file() {
                return Ok(path);
            }
            return Err(PhenoError::unsupported_platform(
                codes::SANDBOX_NAMESPACES_UNSUPPORTED,
                format!(
                    "{env_var}={p:?} is not an executable file \
                     (refusing silent skip for multi-range USER ns maps)"
                ),
            ));
        }
    }
    for candidate in [
        PathBuf::from(format!("/usr/bin/{default_name}")),
        PathBuf::from(format!("/bin/{default_name}")),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    if let Some(path) = find_on_path(default_name) {
        return Ok(path);
    }
    Err(PhenoError::unsupported_platform(
        codes::SANDBOX_NAMESPACES_UNSUPPORTED,
        format!(
            "{default_name} not found (install shadow-utils or set {env_var}); \
             required for multi-range USER ns maps"
        ),
    ))
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn write_proc(path: &str, bytes: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new().write(true).open(path).map_err(|e| {
        PhenoError::unsupported_platform(
            codes::SANDBOX_NAMESPACES_UNSUPPORTED,
            format!(
                "open {path} for USER ns map failed: {e} \
                 (need CAP_SETUID/SETGID in parent or a writable userns; \
                 refusing silent skip when user ns was requested)"
            ),
        )
    })?;
    f.write_all(bytes).map_err(|e| {
        PhenoError::unsupported_platform(
            codes::SANDBOX_NAMESPACES_UNSUPPORTED,
            format!(
                "write {path} for USER ns map failed: {e} \
                 (refusing silent skip — maps are required when user ns is requested)"
            ),
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod hermetic {
    use super::*;

    #[test]
    fn write_proc_rejects_missing_path() {
        let err = write_proc("/proc/self/eidolon_no_such_map", b"0 0 1\n").unwrap_err();
        assert!(matches!(err, PhenoError::UnsupportedPlatform { .. }));
    }

    #[test]
    fn map_line_format_matches_kernel() {
        let m = UserNsIdMap {
            inside: 0,
            outside: 1000,
            count: 1,
        };
        assert_eq!(m.proc_line(), "0 1000 1");
    }

    #[test]
    fn identity_single_map_uses_direct_proc() {
        let maps = vec![UserNsIdMap {
            inside: 0,
            outside: 1000,
            count: 1,
        }];
        assert!(!maps_need_newidmap(&maps, 1000));
    }

    #[test]
    fn multi_count_needs_newidmap() {
        let maps = vec![UserNsIdMap {
            inside: 0,
            outside: 100_000,
            count: 65_536,
        }];
        assert!(maps_need_newidmap(&maps, 1000));
    }

    #[test]
    fn dual_range_needs_newidmap() {
        let maps = vec![
            UserNsIdMap {
                inside: 0,
                outside: 1000,
                count: 1,
            },
            UserNsIdMap {
                inside: 1,
                outside: 100_000,
                count: 65_536,
            },
        ];
        assert!(maps_need_newidmap(&maps, 1000));
    }

    #[test]
    fn resolve_idmap_tool_missing_fails_loud() {
        std::env::set_var(NEWUIDMAP_ENV, "/nonexistent/eidolon-newuidmap");
        let err = resolve_idmap_tool(NEWUIDMAP_ENV, "newuidmap").unwrap_err();
        std::env::remove_var(NEWUIDMAP_ENV);
        assert!(matches!(err, PhenoError::UnsupportedPlatform { .. }));
    }
}
