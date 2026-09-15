//! Linux cgroup v2 memory / CPU / disk-I/O limit application.
//!
//! No third-party crate — writes the unified hierarchy controllers under
//! `/sys/fs/cgroup` (or the current process cgroup parent).
//!
//! # Availability
//!
//! Live apply requires **all** of:
//! - feature `sandbox-cgroup`
//! - `target_os = "linux"`
//! - a writable cgroup v2 hierarchy the process can create children in
//!
//! Otherwise [`apply`] returns
//! [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError) with
//! [`codes::SANDBOX_CGROUP_UNSUPPORTED`](crate::codes::SANDBOX_CGROUP_UNSUPPORTED).
//!
//! # Disk honesty
//!
//! cgroup v2 has **no** portable disk-capacity controller. When
//! [`CgroupPlan::disk_mib`](super::plan::CgroupPlan::disk_mib) is `Some`,
//! [`apply`] writes **`io.max`** (`rbps`/`wbps` = `disk_mib` MiB/s) for a
//! known block device ([`disk::DISK_DEV_ENV`] / detect). Missing device or
//! unwritable `io.max` →
//! [`codes::SANDBOX_CGROUP_DISK_UNAVAILABLE`](crate::codes::SANDBOX_CGROUP_DISK_UNAVAILABLE).
//! Set `disk_mib: None` to skip I/O throttling. See [`disk`].

mod disk;

pub use disk::{
    bytes_per_sec_from_disk_mib, io_max_line, parse_maj_min, resolve_disk_device, DISK_DEV_ENV,
    DISK_PATH_ENV,
};

use crate::codes;
use crate::enforcement::plan::EnforcementPlan;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use eidolon_core::security::validate_sandbox_id;

/// Result of moving the current process into a limited cgroup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupStatus {
    /// Absolute path of the child cgroup directory created.
    pub cgroup_path: String,
    /// `memory.max` written (bytes).
    pub memory_max_bytes: u64,
    /// `cpu.max` line written (`"$QUOTA $PERIOD"`).
    pub cpu_max: String,
    /// `true` when `io.max` was written for a resolved disk device.
    pub disk_enforced: bool,
    /// `io.max` line written when [`Self::disk_enforced`].
    pub io_max: Option<String>,
}

/// Host probe: cgroup v2 looks present and feature/OS allow live apply.
pub fn cgroup_ready() -> bool {
    #[cfg(all(feature = "sandbox-cgroup", target_os = "linux"))]
    {
        linux::cgroup_v2_writable().is_some()
    }
    #[cfg(not(all(feature = "sandbox-cgroup", target_os = "linux")))]
    {
        false
    }
}

/// Apply cgroup v2 memory/CPU/(optional disk I/O) limits from `plan` to the
/// **current process**.
///
/// Creates `…/eidolon-<sandbox_id>/` under a writable parent, writes
/// `memory.max` + `cpu.max`, optionally `io.max` when `disk_mib` is set,
/// then moves this PID into `cgroup.procs`.
pub fn apply(sandbox_id: &str, plan: &EnforcementPlan) -> Result<CgroupStatus> {
    validate_sandbox_id(sandbox_id)?;
    #[cfg(all(feature = "sandbox-cgroup", target_os = "linux"))]
    {
        linux::apply_plan(sandbox_id, &plan.cgroup)
    }
    #[cfg(not(all(feature = "sandbox-cgroup", target_os = "linux")))]
    {
        let _ = plan;
        Err(unsupported(
            "cgroup apply requires Linux + feature `sandbox-cgroup` \
             (this host/build cannot enforce memory/CPU/disk-I/O cgroup limits)",
        ))
    }
}

fn unsupported(message: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_CGROUP_UNSUPPORTED, message)
}

/// Write memory/CPU/(optional io.max) into a child under `parent`.
///
/// Used by the live Linux path and by hermetic unit tests with a fake cgroup
/// tree. When `join_pid` is `Some`, that PID is written to `cgroup.procs`.
#[cfg(all(feature = "sandbox-cgroup", any(test, target_os = "linux")))]
pub(crate) fn apply_plan_at(
    parent: &std::path::Path,
    sandbox_id: &str,
    plan: &crate::enforcement::plan::CgroupPlan,
    disk_device: Option<(u32, u32)>,
    join_pid: Option<u32>,
) -> Result<CgroupStatus> {
    use std::fs;

    let child = parent.join(format!("eidolon-{sandbox_id}"));
    fs::create_dir_all(&child).map_err(|e| {
        unsupported(format!(
            "failed to create cgroup {}: {e}",
            child.display()
        ))
    })?;

    write_controller(
        &child.join("memory.max"),
        plan.memory_max_bytes.to_string().as_bytes(),
    )?;

    let cpu_max = format!("{} {}", plan.cpu_quota_us, plan.cpu_period_us);
    write_controller(&child.join("cpu.max"), cpu_max.as_bytes())?;

    let (disk_enforced, io_max) = match plan.disk_mib {
        None => (false, None),
        Some(disk_mib) => {
            let (maj, min) = match disk_device {
                Some(mm) => mm,
                None => {
                    return Err(disk::disk_unavailable(format!(
                        "SandboxPolicy.disk_mib={disk_mib} but no block device resolved \
                         (set {}=maj:min); cgroup v2 maps disk_mib → io.max rbps/wbps only",
                        disk::DISK_DEV_ENV
                    )));
                }
            };
            // Best-effort enable io controller on the parent.
            let _ = try_enable_io(parent);
            let line = disk::io_max_line(maj, min, disk_mib);
            write_controller_disk(&child.join("io.max"), line.as_bytes())?;
            (true, Some(line))
        }
    };

    if let Some(pid) = join_pid {
        write_controller(&child.join("cgroup.procs"), pid.to_string().as_bytes())?;
    }

    Ok(CgroupStatus {
        cgroup_path: child.display().to_string(),
        memory_max_bytes: plan.memory_max_bytes,
        cpu_max,
        disk_enforced,
        io_max,
    })
}

#[cfg(all(feature = "sandbox-cgroup", any(test, target_os = "linux")))]
fn try_enable_io(parent: &std::path::Path) {
    let sc = parent.join("cgroup.subtree_control");
    let _ = std::fs::OpenOptions::new()
        .write(true)
        .open(&sc)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(b"+io")
        });
}

#[cfg(all(feature = "sandbox-cgroup", any(test, target_os = "linux")))]
fn write_controller(path: &std::path::Path, value: &[u8]) -> Result<()> {
    use std::fs;
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| {
            unsupported(format!(
                "cannot open cgroup controller {}: {e}",
                path.display()
            ))
        })?;
    f.write_all(value).map_err(|e| {
        PhenoError::Platform(format!(
            "failed writing cgroup controller {}: {e}",
            path.display()
        ))
    })?;
    Ok(())
}

#[cfg(all(feature = "sandbox-cgroup", any(test, target_os = "linux")))]
fn write_controller_disk(path: &std::path::Path, value: &[u8]) -> Result<()> {
    use std::fs;
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| {
            disk::disk_unavailable(format!(
                "cannot open cgroup io.max {}: {e} (is the io controller enabled?)",
                path.display()
            ))
        })?;
    f.write_all(value).map_err(|e| {
        disk::disk_unavailable(format!(
            "failed writing cgroup io.max {}: {e}",
            path.display()
        ))
    })?;
    Ok(())
}

#[cfg(all(feature = "sandbox-cgroup", target_os = "linux"))]
mod linux {
    use super::*;
    use crate::enforcement::plan::CgroupPlan;
    use std::fs;
    use std::path::{Path, PathBuf};

    const CGROUP_ROOT: &str = "/sys/fs/cgroup";

    pub(super) fn cgroup_v2_writable() -> Option<PathBuf> {
        let root = PathBuf::from(CGROUP_ROOT);
        if !root.is_dir() {
            return None;
        }
        if let Some(current) = read_self_cgroup_path() {
            if current.join("cgroup.procs").exists() && can_create_child(&current) {
                return Some(current);
            }
        }
        if can_create_child(&root) {
            return Some(root);
        }
        None
    }

    fn read_self_cgroup_path() -> Option<PathBuf> {
        let content = fs::read_to_string("/proc/self/cgroup").ok()?;
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("0::") {
                let rel = rest.trim();
                let path = if rel.is_empty() || rel == "/" {
                    PathBuf::from(CGROUP_ROOT)
                } else {
                    PathBuf::from(CGROUP_ROOT).join(rel.trim_start_matches('/'))
                };
                return Some(path);
            }
        }
        None
    }

    fn can_create_child(parent: &Path) -> bool {
        let probe = parent.join(".eidolon-cgroup-probe");
        match fs::create_dir(&probe) {
            Ok(()) => {
                let _ = fs::remove_dir(&probe);
                true
            }
            Err(_) => false,
        }
    }

    pub(super) fn apply_plan(sandbox_id: &str, plan: &CgroupPlan) -> Result<CgroupStatus> {
        let parent = cgroup_v2_writable().ok_or_else(|| {
            unsupported(
                "cgroup v2 hierarchy not found or not writable \
                 (need delegated subtree under /sys/fs/cgroup)",
            )
        })?;

        let disk_device = if plan.disk_mib.is_some() {
            Some(disk::resolve_disk_device()?)
        } else {
            None
        };

        apply_plan_at(
            &parent,
            sandbox_id,
            plan,
            disk_device,
            Some(std::process::id()),
        )
    }
}

#[cfg(all(feature = "sandbox-cgroup", test))]
mod hermetic;
