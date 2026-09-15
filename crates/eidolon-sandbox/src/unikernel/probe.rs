//! Hermetic rootfs / kernel path resolution (no bundled images).
//!
//! Resolution order for each artifact:
//! 1. Explicit path argument (if provided and is a file)
//! 2. Environment override ([`ROOTFS_PATH_ENV`] / [`KERNEL_PATH_ENV`])
//! 3. Otherwise `None` — callers must fail-loud (never invent a default home path)

use std::path::{Path, PathBuf};

/// Env override for the guest rootfs / disk image (`EIDOLON_ROOTFS`).
pub const ROOTFS_PATH_ENV: &str = "EIDOLON_ROOTFS";

/// Env override for the guest kernel image (`EIDOLON_KERNEL`).
pub const KERNEL_PATH_ENV: &str = "EIDOLON_KERNEL";

/// Resolve a rootfs path via explicit argument or [`ROOTFS_PATH_ENV`].
///
/// Returns `Some` only when the path exists and is a regular file.
pub fn resolve_rootfs(explicit: Option<&Path>) -> Option<PathBuf> {
    resolve_file(explicit, ROOTFS_PATH_ENV)
}

/// `true` when [`resolve_rootfs`] finds a file.
pub fn rootfs_ready(explicit: Option<&Path>) -> bool {
    resolve_rootfs(explicit).is_some()
}

/// Resolve a kernel path via explicit argument or [`KERNEL_PATH_ENV`].
pub fn resolve_kernel(explicit: Option<&Path>) -> Option<PathBuf> {
    resolve_file(explicit, KERNEL_PATH_ENV)
}

/// `true` when [`resolve_kernel`] finds a file.
pub fn kernel_ready(explicit: Option<&Path>) -> bool {
    resolve_kernel(explicit).is_some()
}

fn resolve_file(explicit: Option<&Path>, env_key: &str) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        // Explicit path that does not exist: do not fall through to env —
        // callers asked for this path; resolution fails so validation can
        // emit SANDBOX_ROOTFS_MISSING / SANDBOX_KERNEL_MISSING against it.
        if !p.as_os_str().is_empty() {
            return None;
        }
    }
    if let Ok(override_path) = std::env::var(env_key) {
        let trimmed = override_path.trim();
        if trimmed.is_empty() {
            return None;
        }
        let p = PathBuf::from(trimmed);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    // Serialize env mutations across tests in this module.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_file(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "eidolon-uk-probe-{}-{}-{}",
            name,
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let mut f = std::fs::File::create(&path).expect("create");
        writeln!(f, "probe-fixture").expect("write");
        path
    }

    #[test]
    fn explicit_wins_when_file() {
        let _g = ENV_LOCK.lock().unwrap();
        let path = temp_file("explicit");
        assert_eq!(resolve_rootfs(Some(path.as_path())), Some(path.clone()));
        assert!(rootfs_ready(Some(path.as_path())));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn explicit_missing_does_not_fall_through() {
        let _g = ENV_LOCK.lock().unwrap();
        let path = temp_file("env-only");
        std::env::set_var(ROOTFS_PATH_ENV, &path);
        assert!(resolve_rootfs(Some(Path::new("/no/such/rootfs.img"))).is_none());
        assert_eq!(resolve_rootfs(None), Some(path.clone()));
        std::env::remove_var(ROOTFS_PATH_ENV);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn env_kernel_resolution() {
        let _g = ENV_LOCK.lock().unwrap();
        let path = temp_file("kernel");
        std::env::set_var(KERNEL_PATH_ENV, &path);
        assert_eq!(resolve_kernel(None), Some(path.clone()));
        assert!(kernel_ready(None));
        std::env::remove_var(KERNEL_PATH_ENV);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_env_ignored() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var(ROOTFS_PATH_ENV, "   ");
        assert!(resolve_rootfs(None).is_none());
        std::env::remove_var(ROOTFS_PATH_ENV);
    }
}
