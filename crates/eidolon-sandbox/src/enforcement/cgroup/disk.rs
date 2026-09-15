//! Disk / I/O limit helpers for cgroup v2.
//!
//! # Honesty
//!
//! Linux cgroup v2 has **no** portable “disk capacity (bytes)” controller.
//! Capacity quotas require filesystem project quotas (XFS/ext4) outside this
//! hook. When [`SandboxPolicy::disk_mib`](eidolon_core::security::SandboxPolicy)
//! is `Some`, this module maps the ceiling to cgroup v2 **`io.max`** bandwidth
//! throttling (`rbps` / `wbps` = `disk_mib` MiB/s) for a known block device.
//!
//! Device selection (first match wins):
//! 1. [`DISK_DEV_ENV`] — `maj:min` (e.g. `8:0`) or a block-device path
//! 2. [`DISK_PATH_ENV`] — path whose backing device is resolved (Linux)
//! 3. Detect backing device for `/` from `/proc/self/mountinfo` (Linux)
//!
//! If `disk_mib` is set and no device can be resolved / `io.max` cannot be
//! written, apply fails loud with
//! [`codes::SANDBOX_CGROUP_DISK_UNAVAILABLE`](crate::codes::SANDBOX_CGROUP_DISK_UNAVAILABLE).
//! Set `disk_mib: None` to skip disk/I/O enforcement.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::path::{Path, PathBuf};

/// Block device major:minor or path (`8:0`, `/dev/sda`).
pub const DISK_DEV_ENV: &str = "EIDOLON_CGROUP_DISK_DEV";

/// Filesystem path whose backing block device should be throttled.
pub const DISK_PATH_ENV: &str = "EIDOLON_CGROUP_DISK_PATH";

/// Convert policy disk MiB into `io.max` bytes-per-second (rbps/wbps).
///
/// Maps capacity intent → bandwidth stand-in: `disk_mib` MiB/s.
pub fn bytes_per_sec_from_disk_mib(disk_mib: u32) -> u64 {
    u64::from(disk_mib).saturating_mul(1024 * 1024)
}

/// Format a cgroup v2 `io.max` line for `maj:min` + dual bandwidth caps.
pub fn io_max_line(major: u32, minor: u32, disk_mib: u32) -> String {
    let bps = bytes_per_sec_from_disk_mib(disk_mib);
    format!("{major}:{minor} rbps={bps} wbps={bps}")
}

/// Parse `maj:min` (decimal). Returns `None` if the string is not that form.
pub fn parse_maj_min(spec: &str) -> Option<(u32, u32)> {
    let (maj_s, min_s) = spec.split_once(':')?;
    if maj_s.is_empty() || min_s.is_empty() {
        return None;
    }
    let major = maj_s.parse().ok()?;
    let minor = min_s.parse().ok()?;
    Some((major, minor))
}

/// Resolve the block device for disk I/O throttling.
///
/// Pure / env-driven pieces run on all platforms; path and mountinfo detection
/// require Linux. Returns [`PhenoError::UnsupportedPlatform`] with
/// [`codes::SANDBOX_CGROUP_DISK_UNAVAILABLE`] when unresolved.
pub fn resolve_disk_device() -> Result<(u32, u32)> {
    if let Ok(raw) = std::env::var(DISK_DEV_ENV) {
        let spec = raw.trim();
        if spec.is_empty() {
            return Err(disk_unavailable(format!(
                "{DISK_DEV_ENV} is set but empty — expected maj:min or block device path"
            )));
        }
        if let Some(mm) = parse_maj_min(spec) {
            return Ok(mm);
        }
        return maj_min_from_block_path(Path::new(spec));
    }

    let path = std::env::var(DISK_PATH_ENV)
        .ok()
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("/"));

    detect_backing_device(&path).ok_or_else(|| {
        disk_unavailable(format!(
            "SandboxPolicy.disk_mib is set but no cgroup disk device is available \
             (set {DISK_DEV_ENV}=maj:min or a block device path, or {DISK_PATH_ENV} \
             to a path on a resolvable mount; cgroup v2 has no portable capacity \
             controller — Eidolon maps disk_mib → io.max rbps/wbps)"
        ))
    })
}

pub(super) fn disk_unavailable(message: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_CGROUP_DISK_UNAVAILABLE, message)
}

fn maj_min_from_block_path(path: &Path) -> Result<(u32, u32)> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path).map_err(|e| {
            disk_unavailable(format!(
                "cannot stat {DISK_DEV_ENV} path {}: {e}",
                path.display()
            ))
        })?;
        // S_IFBLK == 0o060000
        if (meta.mode() & 0o170000) != 0o060000 {
            return Err(disk_unavailable(format!(
                "{DISK_DEV_ENV} path {} is not a block device (use maj:min or /dev/...)",
                path.display()
            )));
        }
        let rdev = meta.rdev();
        Ok(linux_major_minor(rdev))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Err(disk_unavailable(format!(
            "{DISK_DEV_ENV} block-device paths require Linux; use maj:min (e.g. 8:0)"
        )))
    }
}

fn detect_backing_device(path: &Path) -> Option<(u32, u32)> {
    #[cfg(target_os = "linux")]
    {
        detect_backing_device_linux(path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_major_minor(rdev: u64) -> (u32, u32) {
    // Match glibc makedev/major/minor for 64-bit rdev.
    let major = (((rdev >> 8) & 0xfff) | ((rdev >> 32) & !0xfff)) as u32;
    let minor = ((rdev & 0xff) | ((rdev >> 12) & !0xff)) as u32;
    (major, minor)
}

/// Best-effort: longest mount-point prefix of `path` in `/proc/self/mountinfo`.
#[cfg(target_os = "linux")]
fn detect_backing_device_linux(path: &Path) -> Option<(u32, u32)> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let target = abs.to_string_lossy();
    let content = std::fs::read_to_string("/proc/self/mountinfo").ok()?;

    let mut best: Option<(usize, u32, u32)> = None;
    for line in content.lines() {
        // mountinfo fields: id parent maj:min root mount_point …
        let mut fields = line.split(' ');
        let _id = fields.next()?;
        let _parent = fields.next()?;
        let maj_min = fields.next()?;
        let _root = fields.next()?;
        let mount_point = fields.next()?;
        if !path_covered_by_mount(&target, mount_point) {
            continue;
        }
        let (maj, min) = parse_maj_min(maj_min)?;
        let score = mount_point.len();
        if best.map(|(s, _, _)| score > s).unwrap_or(true) {
            best = Some((score, maj, min));
        }
    }
    best.map(|(_, maj, min)| (maj, min))
}

#[cfg(target_os = "linux")]
fn path_covered_by_mount(target: &str, mount_point: &str) -> bool {
    if mount_point == "/" {
        return target.starts_with('/');
    }
    target == mount_point
        || target
            .strip_prefix(mount_point)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_maj_min_accepts_decimal() {
        assert_eq!(parse_maj_min("8:0"), Some((8, 0)));
        assert_eq!(parse_maj_min("259:1"), Some((259, 1)));
        assert_eq!(parse_maj_min(""), None);
        assert_eq!(parse_maj_min("8"), None);
        assert_eq!(parse_maj_min(":0"), None);
    }

    #[test]
    fn io_max_line_maps_mib_to_bps() {
        assert_eq!(
            io_max_line(8, 0, 100),
            "8:0 rbps=104857600 wbps=104857600"
        );
        assert_eq!(bytes_per_sec_from_disk_mib(1), 1024 * 1024);
    }

    #[test]
    fn resolve_honors_maj_min_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(DISK_DEV_ENV, "8:16");
        let got = resolve_disk_device().expect("env maj:min");
        assert_eq!(got, (8, 16));
        std::env::remove_var(DISK_DEV_ENV);
    }

    #[test]
    fn resolve_empty_dev_env_fail_loud() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(DISK_DEV_ENV, "  ");
        let err = resolve_disk_device().unwrap_err();
        assert_eq!(
            err.unsupported_code(),
            Some(codes::SANDBOX_CGROUP_DISK_UNAVAILABLE)
        );
        std::env::remove_var(DISK_DEV_ENV);
    }
}
