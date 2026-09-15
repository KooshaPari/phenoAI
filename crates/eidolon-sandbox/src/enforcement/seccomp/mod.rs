//! Linux seccomp-bpf enforcement.
//!
//! wraps: seccompiler 0.5 — BPF filter compile + `apply_filter` (rust-vmm/seccompiler)
//!
//! # Availability
//!
//! Live apply requires **all** of:
//! - feature `sandbox-seccomp`
//! - `target_os = "linux"` + little-endian
//! - a supported arch (`x86_64` / `aarch64` / `riscv64`)
//! - kernel seccomp filter support
//!
//! Otherwise [`apply`] returns
//! [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError) with
//! [`codes::SANDBOX_SECCOMP_UNSUPPORTED`](crate::codes::SANDBOX_SECCOMP_UNSUPPORTED).
//!
//! # Profiles
//!
//! Selected via [`EIDOLON_SECCOMP_PROFILE`](crate::enforcement::SECCOMP_PROFILE_ENV) /
//! [`SeccompProfile`](super::plan::SeccompProfile):
//!
//! | Spec | Mode | Notes |
//! |---|---|---|
//! | `block-dangerous` (default) | blocklist | Allow unmatched; `EPERM` high-risk set |
//! | `oci-default` | allowlist | Embedded Docker/OCI-style unconditional allows |
//! | `/path/to.json` | allowlist | OCI/Docker seccomp JSON; fail-loud if invalid/empty |
//!
//! Invalid profile specs fail at plan time with [`PhenoError::BadRequest`].

use crate::codes;
use crate::enforcement::plan::EnforcementPlan;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Result of installing a seccomp filter on the current thread/process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeccompStatus {
    /// Profile label applied (`block-dangerous`, `oci-default`, or custom path).
    pub profile: String,
    /// Number of syscall numbers mapped to deny/`EPERM` (block-dangerous).
    pub blocked_syscalls: usize,
    /// Number of syscall names on the allowlist (oci-default / custom). `0` for
    /// block-dangerous.
    pub allowed_syscalls: usize,
    /// Whether `PR_SET_NO_NEW_PRIVS` was set as part of filter install
    /// (always `true` on success via seccompiler).
    pub no_new_privs: bool,
}

/// Host probe: seccomp filter path looks available (Linux + feature + arch).
pub fn seccomp_ready() -> bool {
    #[cfg(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    ))]
    {
        linux::arch_supported()
    }
    #[cfg(not(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    )))]
    {
        false
    }
}

/// Apply the seccomp profile from `plan` to the **current thread**.
///
/// Fail-loud off-platform / feature-off / unsupported arch / kernel refusal.
/// Filter install is irreversible for the thread (and sets no-new-privs).
pub fn apply(plan: &EnforcementPlan) -> Result<SeccompStatus> {
    #[cfg(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    ))]
    {
        linux::apply_plan(&plan.seccomp)
    }
    #[cfg(not(all(
        feature = "sandbox-seccomp",
        target_os = "linux",
        target_endian = "little"
    )))]
    {
        let _ = plan;
        Err(unsupported(
            "seccomp apply requires Linux (little-endian) + feature \
             `sandbox-seccomp` (this host/build cannot install BPF filters)",
        ))
    }
}

fn unsupported(message: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_SECCOMP_UNSUPPORTED, message)
}

#[cfg(all(
    feature = "sandbox-seccomp",
    target_os = "linux",
    target_endian = "little"
))]
mod linux {
    use super::*;
    use crate::enforcement::plan::{SeccompPlan, SeccompProfile};
    use crate::enforcement::seccomp_oci::{load_oci_allowlist_file, oci_default_allowlist};
    use crate::enforcement::seccomp_profile::profile_label;
    use crate::enforcement::seccomp_syscall_nr::resolve_allowlist;
    // wraps: seccompiler 0.5 — SeccompFilter / apply_filter
    use seccompiler::{apply_filter, BpfProgram, SeccompAction, SeccompFilter, TargetArch};
    use std::collections::BTreeMap;
    use std::convert::TryInto;

    pub(super) fn arch_supported() -> bool {
        TargetArch::try_from(std::env::consts::ARCH).is_ok()
    }

    pub(super) fn apply_plan(plan: &SeccompPlan) -> Result<SeccompStatus> {
        let arch: TargetArch = std::env::consts::ARCH.try_into().map_err(|_| {
            unsupported(format!(
                "seccomp unsupported host arch {} (need x86_64/aarch64/riscv64)",
                std::env::consts::ARCH
            ))
        })?;

        match &plan.profile {
            SeccompProfile::BlockDangerous => apply_block_dangerous(arch),
            SeccompProfile::OciDefault => {
                let names = oci_default_allowlist()?;
                apply_allowlist(arch, &names, profile_label(&plan.profile))
            }
            SeccompProfile::Custom(path) => {
                let names = load_oci_allowlist_file(path)?;
                apply_allowlist(arch, &names, profile_label(&plan.profile))
            }
        }
    }

    fn apply_block_dangerous(arch: TargetArch) -> Result<SeccompStatus> {
        let blocked = dangerous_syscalls();
        let count = blocked.len();
        let rules: BTreeMap<i64, Vec<seccompiler::SeccompRule>> =
            blocked.into_iter().map(|nr| (nr, Vec::new())).collect();

        let filter: BpfProgram = SeccompFilter::new(
            rules,
            SeccompAction::Allow,
            SeccompAction::Errno(libc::EPERM as u32),
            arch,
        )
        .map_err(|e| unsupported(format!("seccomp filter build failed: {e}")))?
        .try_into()
        .map_err(|e| unsupported(format!("seccomp BPF compile failed: {e}")))?;

        apply_filter(&filter).map_err(|e| {
            unsupported(format!(
                "seccomp apply_filter failed: {e} (kernel may lack CONFIG_SECCOMP_FILTER)"
            ))
        })?;

        Ok(SeccompStatus {
            profile: "block-dangerous".into(),
            blocked_syscalls: count,
            allowed_syscalls: 0,
            no_new_privs: true,
        })
    }

    fn apply_allowlist(
        arch: TargetArch,
        names: &[String],
        profile: String,
    ) -> Result<SeccompStatus> {
        let host_arch = std::env::consts::ARCH;
        let resolved = resolve_allowlist(host_arch, names);
        if resolved.is_empty() {
            return Err(PhenoError::BadRequest(format!(
                "seccomp allowlist profile {profile} resolved 0 syscalls for arch {host_arch}"
            )));
        }
        if resolved.len() < 16 {
            return Err(PhenoError::BadRequest(format!(
                "seccomp allowlist profile {profile} resolved only {} syscalls for arch {host_arch} \
                 (need >= 16 known names)",
                resolved.len()
            )));
        }

        let allowed_syscalls = resolved.len();
        let rules: BTreeMap<i64, Vec<seccompiler::SeccompRule>> = resolved
            .into_iter()
            .map(|(_name, nr)| (nr, Vec::new()))
            .collect();

        let filter: BpfProgram = SeccompFilter::new(
            rules,
            SeccompAction::Errno(libc::EPERM as u32),
            SeccompAction::Allow,
            arch,
        )
        .map_err(|e| unsupported(format!("seccomp allowlist filter build failed: {e}")))?
        .try_into()
        .map_err(|e| unsupported(format!("seccomp allowlist BPF compile failed: {e}")))?;

        apply_filter(&filter).map_err(|e| {
            unsupported(format!(
                "seccomp apply_filter failed: {e} (kernel may lack CONFIG_SECCOMP_FILTER)"
            ))
        })?;

        Ok(SeccompStatus {
            profile,
            blocked_syscalls: 0,
            allowed_syscalls,
            no_new_privs: true,
        })
    }

    /// Compile an allowlist to BPF without installing (hermetic Linux unit aid).
    #[cfg(test)]
    pub(super) fn compile_allowlist_only(
        arch: TargetArch,
        names: &[String],
    ) -> Result<usize> {
        let resolved = resolve_allowlist(std::env::consts::ARCH, names);
        if resolved.is_empty() {
            return Err(unsupported("empty allowlist after arch resolve"));
        }
        let count = resolved.len();
        let rules: BTreeMap<i64, Vec<seccompiler::SeccompRule>> = resolved
            .into_iter()
            .map(|(_n, nr)| (nr, Vec::new()))
            .collect();
        let _filter: BpfProgram = SeccompFilter::new(
            rules,
            SeccompAction::Errno(libc::EPERM as u32),
            SeccompAction::Allow,
            arch,
        )
        .map_err(|e| unsupported(format!("filter build: {e}")))?
        .try_into()
        .map_err(|e| unsupported(format!("bpf compile: {e}")))?;
        Ok(count)
    }

    /// High-risk syscalls blocked with `EPERM`. Keep modest — not an allowlist.
    fn dangerous_syscalls() -> Vec<i64> {
        vec![
            libc::SYS_reboot,
            libc::SYS_mount,
            libc::SYS_umount2,
            libc::SYS_pivot_root,
            libc::SYS_swapon,
            libc::SYS_swapoff,
            libc::SYS_init_module,
            libc::SYS_finit_module,
            libc::SYS_delete_module,
            libc::SYS_kexec_load,
            #[cfg(target_arch = "x86_64")]
            libc::SYS_kexec_file_load,
            libc::SYS_bpf,
            libc::SYS_perf_event_open,
            libc::SYS_userfaultfd,
        ]
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn oci_default_compiles_bpf() {
            let arch: TargetArch = std::env::consts::ARCH.try_into().expect("arch");
            let names = oci_default_allowlist().expect("names");
            let len = compile_allowlist_only(arch, &names).expect("compile");
            assert!(len >= 16);
        }

        #[test]
        fn tiny_allowlist_compiles() {
            let arch: TargetArch = std::env::consts::ARCH.try_into().expect("arch");
            let names = vec![
                "read".into(),
                "write".into(),
                "exit".into(),
                "exit_group".into(),
                "close".into(),
                "openat".into(),
                "mmap".into(),
                "munmap".into(),
                "brk".into(),
                "rt_sigaction".into(),
                "rt_sigprocmask".into(),
                "clone".into(),
                "futex".into(),
                "getpid".into(),
                "clock_gettime".into(),
                "nanosleep".into(),
            ];
            let len = compile_allowlist_only(arch, &names).expect("compile");
            assert_eq!(len, 16);
        }
    }
}
