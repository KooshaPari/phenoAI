//! Mount-namespace `pivot_root` into a provided rootfs.
//!
//! wraps: nix 0.31 — `mount::{mount,umount2,MsFlags,MntFlags}` + `unistd::pivot_root`
//!
//! # Sequence (after `unshare(CLONE_NEWNS)`)
//!
//! 1. `mount(MS_REC|MS_PRIVATE)` on `/` so the new mount tree does not
//!    propagate back to the host.
//! 2. Recursive bind-mount of `rootfs` onto itself (pivot requires a mount
//!    point, not merely a directory).
//! 3. Create `rootfs/.eidolon-put-old`, `pivot_root(rootfs, put_old)`.
//! 4. `chdir("/")`, lazy-umount `/.eidolon-put-old`, remove the directory.
//!
//! Fail-loud on any step — never claim [`crate::enforcement::NamespaceStatus::pivot_root_applied`]
//! unless this returns `Ok(())`.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::unistd::pivot_root;
use std::path::Path;

/// Put-old directory name under the new root (also visible as `/.eidolon-put-old`
/// after pivot).
pub(crate) const PUT_OLD_NAME: &str = ".eidolon-put-old";

/// Pivot the calling process into `rootfs` as its new `/`.
///
/// Requires a prior successful `unshare(CLONE_NEWNS)` (and typically
/// `CAP_SYS_ADMIN`). Does **not** silently skip on failure.
pub(crate) fn pivot_into(rootfs: &Path) -> Result<()> {
    if !rootfs.is_dir() {
        return Err(unsupported(format!(
            "pivot_root rootfs is not a directory: {} \
             (set {env} to an existing directory)",
            rootfs.display(),
            env = super::namespaces::NS_PIVOT_ROOTFS_ENV
        )));
    }

    // Detach from host mount propagation before rearranging the tree.
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )
    .map_err(|e| {
        unsupported(format!(
            "mount(MS_REC|MS_PRIVATE) on / before pivot_root failed: {e} \
             (need CAP_SYS_ADMIN in the mount namespace)"
        ))
    })?;

    // pivot_root requires new_root to be a mount point.
    mount(
        Some(rootfs),
        rootfs,
        None::<&str>,
        MsFlags::MS_BIND | MsFlags::MS_REC,
        None::<&str>,
    )
    .map_err(|e| {
        unsupported(format!(
            "bind-mount({}) onto itself before pivot_root failed: {e}",
            rootfs.display()
        ))
    })?;

    let put_old = rootfs.join(PUT_OLD_NAME);
    std::fs::create_dir_all(&put_old).map_err(|e| {
        unsupported(format!(
            "create put_old {} before pivot_root failed: {e}",
            put_old.display()
        ))
    })?;

    pivot_root(rootfs, &put_old).map_err(|e| {
        unsupported(format!(
            "pivot_root({}, {}) failed: {e} \
             (refusing silent skip when pivot was requested)",
            rootfs.display(),
            put_old.display()
        ))
    })?;

    std::env::set_current_dir("/").map_err(|e| {
        unsupported(format!("chdir(/) after pivot_root failed: {e}"))
    })?;

    let old_root = Path::new("/").join(PUT_OLD_NAME);
    umount2(&old_root, MntFlags::MNT_DETACH).map_err(|e| {
        unsupported(format!(
            "umount2({}, MNT_DETACH) after pivot_root failed: {e}",
            old_root.display()
        ))
    })?;
    // Best-effort cleanup; mount is already detached.
    let _ = std::fs::remove_dir(&old_root);

    Ok(())
}

fn unsupported(message: impl Into<String>) -> PhenoError {
    PhenoError::unsupported_platform(codes::SANDBOX_NAMESPACES_UNSUPPORTED, message)
}

#[cfg(test)]
mod hermetic {
    use super::*;

    #[test]
    fn put_old_name_stable() {
        assert_eq!(PUT_OLD_NAME, ".eidolon-put-old");
    }

    #[test]
    fn missing_rootfs_dir_fails_loud() {
        let missing = Path::new("/tmp/eidolon-pivot-rootfs-does-not-exist-xyz");
        let err = pivot_into(missing).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not a directory") || msg.contains("pivot_root"),
            "got {msg}"
        );
    }
}
