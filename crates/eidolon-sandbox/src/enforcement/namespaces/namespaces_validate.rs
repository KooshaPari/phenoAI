//! Validation helpers for USER ns maps, subid parsing, and pivot rootfs resolution.

use std::path::PathBuf;

use eidolon_core::error::PhenoError;
use eidolon_core::Result;

use super::{
    NS_GID_MAP_ENV, NS_PID_ENV, NS_PIVOT_ENV, NS_PIVOT_ROOTFS_ENV, NS_UID_MAP_ENV, NS_USER_ENV,
    NS_USER_SUBIDS_ENV, SUBGID_FILE, SUBUID_FILE,
};
use crate::enforcement::plan::UserNsIdMap;

/// Whether `EIDOLON_SANDBOX_NS_PID` requests PID namespace isolation.
pub fn env_requests_pid_ns() -> bool {
    env_truthy(NS_PID_ENV)
}

/// Whether `EIDOLON_SANDBOX_NS_PIVOT` requests mount-ns `pivot_root`.
pub fn env_requests_pivot_root() -> bool {
    env_truthy(NS_PIVOT_ENV)
}

/// Whether `EIDOLON_SANDBOX_NS_USER` requests USER namespace + uid/gid maps.
pub fn env_requests_user_ns() -> bool {
    env_truthy(NS_USER_ENV)
}

/// Whether `EIDOLON_SANDBOX_NS_USER_SUBIDS` requests `/etc/subuid` delegation.
pub fn env_requests_user_subids() -> bool {
    env_truthy(NS_USER_SUBIDS_ENV)
}

/// Resolve USER ns maps from env when [`env_requests_user_ns`] is true.
///
/// Defaults: map host euid/egid → 0 inside the ns (`0 <id> 1`). Optional
/// `EIDOLON_SANDBOX_NS_UID_MAP` / `EIDOLON_SANDBOX_NS_GID_MAP` override the
/// ranges (`inside:outside:count`, comma-separated for multiple). When
/// `EIDOLON_SANDBOX_NS_USER_SUBIDS=1` and map env is unset, read
/// `/etc/subuid` + `/etc/subgid`. Fail-loud on malformed overrides or missing
/// subordinate delegation.
pub fn env_user_ns_maps() -> Result<Option<(Vec<UserNsIdMap>, Vec<UserNsIdMap>)>> {
    if !env_requests_user_ns() {
        return Ok(None);
    }
    let uid_maps = resolve_uid_maps_from_env()?;
    let gid_maps = resolve_gid_maps_from_env()?;
    Ok(Some((uid_maps, gid_maps)))
}

fn resolve_uid_maps_from_env() -> Result<Vec<UserNsIdMap>> {
    match std::env::var(NS_UID_MAP_ENV) {
        Ok(s) if !s.is_empty() => parse_id_map_spec(&s),
        _ if env_requests_user_subids() => subid_uid_maps_for_current_user(),
        _ => Ok(vec![identity_uid_map()]),
    }
}

fn resolve_gid_maps_from_env() -> Result<Vec<UserNsIdMap>> {
    match std::env::var(NS_GID_MAP_ENV) {
        Ok(s) if !s.is_empty() => parse_id_map_spec(&s),
        _ if env_requests_user_subids() => subid_gid_maps_for_current_user(),
        _ => Ok(vec![identity_gid_map()]),
    }
}

fn identity_uid_map() -> UserNsIdMap {
    UserNsIdMap {
        inside: 0,
        outside: host_euid(),
        count: 1,
    }
}

fn identity_gid_map() -> UserNsIdMap {
    UserNsIdMap {
        inside: 0,
        outside: host_egid(),
        count: 1,
    }
}

/// Parse one or more `inside:outside:count` ranges (comma-separated).
pub fn parse_id_map_spec(s: &str) -> Result<Vec<UserNsIdMap>> {
    let mut maps = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        maps.push(parse_id_map_line(part)?);
    }
    if maps.is_empty() {
        return Err(PhenoError::BadRequest(
            "USER ns map spec is empty after parsing".into(),
        ));
    }
    Ok(maps)
}

/// Parse shadow-utils `/etc/subuid` or `/etc/subgid` and return the range for
/// `username` (`subid_start`, `count`). Returns `Ok(None)` when absent.
pub fn parse_subid_file(content: &str, username: &str) -> Result<Option<(u32, u32)>> {
    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<_> = line.split(':').collect();
        if parts.len() != 3 || parts[0] != username {
            continue;
        }
        let start = parse_subid_field(parts[1], "start")?;
        let count = parse_subid_field(parts[2], "count")?;
        if count == 0 {
            return Err(PhenoError::BadRequest(format!(
                "subid count for {username:?} must be >= 1 in {line:?}"
            )));
        }
        return Ok(Some((start, count)));
    }
    Ok(None)
}

fn parse_subid_field(raw: &str, label: &str) -> Result<u32> {
    raw.trim()
        .parse()
        .map_err(|_| PhenoError::BadRequest(format!("subid {label} is not u32: {raw:?}")))
}

fn subid_uid_maps_for_current_user() -> Result<Vec<UserNsIdMap>> {
    subid_maps_for_current_user(SUBUID_FILE, true)
}

fn subid_gid_maps_for_current_user() -> Result<Vec<UserNsIdMap>> {
    subid_maps_for_current_user(SUBGID_FILE, false)
}

fn subid_maps_for_current_user(path: &str, is_uid: bool) -> Result<Vec<UserNsIdMap>> {
    let username = login_username()?;
    let content = std::fs::read_to_string(path).map_err(|e| {
        PhenoError::BadRequest(format!(
            "read {path} for USER ns subids failed: {e} \
             (set explicit {NS_UID_MAP_ENV}/{NS_GID_MAP_ENV} or install shadow-utils subids)"
        ))
    })?;
    let (start, count) = parse_subid_file(&content, &username)?.ok_or_else(|| {
        PhenoError::BadRequest(format!(
            "no subid range for login user {username:?} in {path} \
             (delegate with usermod/subuid or set explicit map env)"
        ))
    })?;
    let identity = if is_uid {
        identity_uid_map()
    } else {
        identity_gid_map()
    };
    Ok(vec![
        identity,
        UserNsIdMap {
            inside: 1,
            outside: start,
            count,
        },
    ])
}

fn login_username() -> Result<String> {
    if let Ok(user) = std::env::var("USER") {
        if !user.is_empty() {
            return Ok(user);
        }
    }
    if let Ok(user) = std::env::var("LOGNAME") {
        if !user.is_empty() {
            return Ok(user);
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CStr;
        unsafe {
            let ptr = libc::getlogin();
            if !ptr.is_null() {
                let name = CStr::from_ptr(ptr).to_string_lossy();
                if !name.is_empty() {
                    return Ok(name.into_owned());
                }
            }
        }
    }
    Err(PhenoError::BadRequest(
        "cannot resolve login username for /etc/subuid lookup \
         (set USER/LOGNAME or explicit EIDOLON_SANDBOX_NS_UID_MAP)"
            .into(),
    ))
}

fn parse_id_map_line(s: &str) -> Result<UserNsIdMap> {
    let parts: Vec<_> = s.trim().split(':').collect();
    if parts.len() != 3 {
        return Err(PhenoError::BadRequest(format!(
            "USER ns map must be inside:outside:count, got {s:?}"
        )));
    }
    let parse = |p: &str, label: &str| -> Result<u32> {
        p.parse()
            .map_err(|_| PhenoError::BadRequest(format!("USER ns map {label} is not u32: {p:?}")))
    };
    let inside = parse(parts[0], "inside")?;
    let outside = parse(parts[1], "outside")?;
    let count = parse(parts[2], "count")?;
    if count == 0 {
        return Err(PhenoError::BadRequest(
            "USER ns map count must be >= 1".into(),
        ));
    }
    Ok(UserNsIdMap {
        inside,
        outside,
        count,
    })
}

fn host_euid() -> u32 {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::geteuid() }
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

fn host_egid() -> u32 {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::getegid() }
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// Resolve pivot rootfs from env-shaped inputs (hermetic / testable).
///
/// When `pivot_requested` is false → `Ok(None)`.
/// When true, `rootfs_env` must be a non-empty path or this returns
/// [`PhenoError::BadRequest`] (fail-loud — no silent skip).
pub fn resolve_pivot_rootfs(
    pivot_requested: bool,
    rootfs_env: Option<&str>,
) -> Result<Option<PathBuf>> {
    if !pivot_requested {
        return Ok(None);
    }
    match rootfs_env {
        Some(p) if !p.is_empty() => Ok(Some(PathBuf::from(p))),
        _ => Err(PhenoError::BadRequest(format!(
            "{NS_PIVOT_ENV} is set but {NS_PIVOT_ROOTFS_ENV} is missing or empty \
             (pivot_root requires an explicit rootfs directory)"
        ))),
    }
}

/// Read pivot rootfs from process env (`EIDOLON_SANDBOX_NS_PIVOT` +
/// `EIDOLON_SANDBOX_NS_PIVOT_ROOTFS`).
pub fn env_pivot_rootfs_path() -> Result<Option<PathBuf>> {
    let raw = std::env::var(NS_PIVOT_ROOTFS_ENV).ok();
    resolve_pivot_rootfs(env_requests_pivot_root(), raw.as_deref())
}

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}
