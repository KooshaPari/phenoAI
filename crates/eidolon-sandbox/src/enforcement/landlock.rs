//! Linux Landlock filesystem (and optional net) enforcement.
//!
//! wraps: landlock 0.4 — Landlock LSM helpers (landlock-lsm/rust-landlock)
//!
//! # Availability
//!
//! Live apply requires **all** of:
//! - feature `sandbox-landlock`
//! - `target_os = "linux"`
//! - a kernel that supports Landlock ABI ≥ V1
//!
//! Otherwise [`apply`] returns
//! [`PhenoError::UnsupportedPlatform`](eidolon_core::PhenoError) with
//! [`codes::SANDBOX_LANDLOCK_UNSUPPORTED`](crate::codes::SANDBOX_LANDLOCK_UNSUPPORTED).

use crate::codes;
use crate::enforcement::plan::EnforcementPlan;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;

/// Result of a successful Landlock restrict_self.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandlockStatus {
    /// Landlock ABI the ruleset targeted (string form for stable logging).
    pub abi: &'static str,
    /// Number of path-beneath rules installed.
    pub rules_installed: usize,
    /// Whether TCP bind/connect were *requested* via BestEffort net rights
    /// (ABI V4+). Older kernels may silently ignore; callers must not treat
    /// this as a hard guarantee that NetworkPolicy::Deny is enforced.
    pub net_handled: bool,
}

/// Host probe: Landlock appears available (Linux + feature; best-effort ABI check).
pub fn landlock_ready() -> bool {
    #[cfg(all(feature = "sandbox-landlock", target_os = "linux"))]
    {
        linux::kernel_supports_landlock()
    }
    #[cfg(not(all(feature = "sandbox-landlock", target_os = "linux")))]
    {
        false
    }
}

/// Apply Landlock restrictions described by `plan` to the **current process**.
///
/// Fail-loud off-platform / feature-off / unsupported kernel. Does not fork —
/// callers that need per-sandbox isolation must apply inside the child after
/// fork/clone (or before `exec`).
pub fn apply(plan: &EnforcementPlan) -> Result<LandlockStatus> {
    #[cfg(all(feature = "sandbox-landlock", target_os = "linux"))]
    {
        linux::apply_plan(plan)
    }
    #[cfg(not(all(feature = "sandbox-landlock", target_os = "linux")))]
    {
        let _ = plan;
        Err(unsupported(
            "Landlock apply requires Linux + feature `sandbox-landlock` \
             (this host/build cannot enforce filesystem Landlock rules)",
        ))
    }
}

fn unsupported(message: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_LANDLOCK_UNSUPPORTED, message)
}

#[cfg(all(feature = "sandbox-landlock", target_os = "linux"))]
mod linux {
    use super::*;
    // wraps: landlock 0.4 — Landlock LSM helpers
    use landlock::{
        Access, AccessFs, AccessNet, BitFlags, CompatLevel, Compatible, PathBeneath, PathFd,
        Ruleset, RulesetAttr, RulesetCreatedAttr, ABI,
    };
    use std::path::Path;

    pub(super) fn kernel_supports_landlock() -> bool {
        Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessFs::from_all(ABI::V1))
            .and_then(|r| r.create())
            .is_ok()
    }

    pub(super) fn apply_plan(plan: &EnforcementPlan) -> Result<LandlockStatus> {
        let abi = ABI::V1;
        let access_all = AccessFs::from_all(abi);
        let access_read = AccessFs::from_read(abi);
        let access_rw = access_read | AccessFs::from_write(abi);
        let access_exec = access_read | AccessFs::Execute;

        let mut ruleset = Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(access_all)
            .map_err(|e| {
                unsupported(format!(
                    "Landlock kernel does not support ABI V1 filesystem rights: {e}"
                ))
            })?;

        // Best-effort TCP (ABI V4+). `BestEffort` keeps the ruleset usable on
        // older kernels; `net_handled` records whether we *requested* net rights.
        let mut net_handled = false;
        if plan.landlock_net_deny {
            ruleset = ruleset
                .set_compatibility(CompatLevel::BestEffort)
                .handle_access(AccessNet::BindTcp | AccessNet::ConnectTcp)
                .map_err(|e| {
                    unsupported(format!("Landlock net handle_access failed unexpectedly: {e}"))
                })?;
            net_handled = true;
        }

        let mut created = ruleset.create().map_err(|e| {
            unsupported(format!("Landlock ruleset create failed: {e}"))
        })?;

        let mut rules_installed = 0usize;
        for path in &plan.landlock_fs.read_only {
            created = add_rule(created, path, access_read)?;
            rules_installed += 1;
        }
        for path in &plan.landlock_fs.read_write {
            created = add_rule(created, path, access_rw)?;
            rules_installed += 1;
        }
        for path in &plan.landlock_fs.execute {
            created = add_rule(created, path, access_exec)?;
            rules_installed += 1;
        }

        created.restrict_self().map_err(|e| {
            PhenoError::Platform(format!("Landlock restrict_self failed: {e}"))
        })?;

        Ok(LandlockStatus {
            abi: "V1",
            rules_installed,
            net_handled,
        })
    }

    fn add_rule<R>(
        ruleset: R,
        path: &Path,
        access: BitFlags<AccessFs>,
    ) -> Result<R>
    where
        R: RulesetCreatedAttr,
    {
        let fd = PathFd::new(path).map_err(|e| {
            PhenoError::BadRequest(format!("Landlock path {path:?} is not openable: {e}"))
        })?;
        ruleset
            .add_rule(PathBeneath::new(fd, access))
            .map_err(|e| PhenoError::Platform(format!("Landlock add_rule for {path:?} failed: {e}")))
    }
}
