//! Durable xdg-desktop-portal restore-token store for RemoteDesktop + Screencast.
//!
//! Opt-in via [`WAYLAND_RESTORE_ENV`] (`EIDOLON_DESKTOP_WAYLAND_RESTORE=1`).
//! Default path: `$XDG_STATE_HOME/eidolon/wayland-restore-token` (never `/tmp`).
//! Override with [`WAYLAND_RESTORE_TOKEN_PATH_ENV`].
//!
//! wraps: ashpd 0.11 `PersistMode` + `restore_token` on SelectDevices/SelectSources.

use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Env gate: set to `1` / `true` / `yes` to enable restore-token persistence.
pub const WAYLAND_RESTORE_ENV: &str = "EIDOLON_DESKTOP_WAYLAND_RESTORE";

/// Env override for the on-disk restore-token file (must not be under `/tmp` by default).
pub const WAYLAND_RESTORE_TOKEN_PATH_ENV: &str = "EIDOLON_DESKTOP_WAYLAND_RESTORE_TOKEN_PATH";

const TOKEN_FILE_NAME: &str = "wayland-restore-token";

/// Runtime restore settings loaded before a portal session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreConfig {
    pub enabled: bool,
    pub path: PathBuf,
    pub token: Option<String>,
}

impl RestoreConfig {
    /// Load restore settings. When disabled, returns no token.
    ///
    /// When enabled, resolves the token path and loads any stored token. Store I/O
    /// errors fail loud with [`codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO`].
    pub fn load() -> Result<Self> {
        if !restore_enabled() {
            return Ok(Self {
                enabled: false,
                path: PathBuf::new(),
                token: None,
            });
        }
        let path = resolve_token_path()?;
        let token = load_token(&path)?;
        Ok(Self {
            enabled: true,
            path,
            token,
        })
    }
}

/// Whether restore-token persistence is enabled via env.
pub fn restore_enabled() -> bool {
    matches!(
        std::env::var(WAYLAND_RESTORE_ENV).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

/// Resolve the on-disk token path (env override or XDG state home).
pub fn resolve_token_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var(WAYLAND_RESTORE_TOKEN_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }
    let state_home = xdg_state_home()?;
    Ok(state_home.join("eidolon").join(TOKEN_FILE_NAME))
}

fn xdg_state_home() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("XDG_STATE_HOME") {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            return Err(restore_io(
                "XDG_STATE_HOME",
                "env var is set but empty — set an absolute path or unset it",
            ));
        }
        return Ok(path);
    }
    let home = std::env::var("HOME").map_err(|e| {
        restore_io(
            "HOME",
            &format!("required for default XDG state path (~/.local/state): {e}"),
        )
    })?;
    Ok(PathBuf::from(home).join(".local").join("state"))
}

/// Load a stored restore token. Missing file → `Ok(None)`. I/O errors fail loud.
pub fn load_token(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path).map_err(|e| restore_io("read", &e.to_string()))?;
    let token = content.trim();
    if token.is_empty() {
        return Ok(None);
    }
    Ok(Some(token.to_owned()))
}

/// Persist a portal-issued restore token (atomic write, `0600` on Unix).
pub fn save_token(path: &Path, token: &str) -> Result<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(restore_io("save", "refusing to persist empty restore token"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| restore_io("create_dir", &e.to_string()))?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp)
            .map_err(|e| restore_io("write_tmp", &e.to_string()))?;
        file.write_all(token.as_bytes())
            .map_err(|e| restore_io("write_tmp", &e.to_string()))?;
        file.write_all(b"\n")
            .map_err(|e| restore_io("write_tmp", &e.to_string()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| restore_io("chmod", &e.to_string()))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| restore_io("rename", &e.to_string()))?;
    log::debug!(
        "Wayland portal restore token persisted to {}",
        path.display()
    );
    Ok(())
}

fn restore_io(context: &str, detail: &str) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO,
        format!(
            "Wayland restore token {context} failed: {detail} — see docs/EXTRACTION_PLAN.md"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        saved_restore: Option<String>,
        saved_path: Option<String>,
        saved_xdg: Option<String>,
        saved_home: Option<String>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let lock = ENV_LOCK.lock().expect("env lock");
            let saved_restore = std::env::var(WAYLAND_RESTORE_ENV).ok();
            let saved_path = std::env::var(WAYLAND_RESTORE_TOKEN_PATH_ENV).ok();
            let saved_xdg = std::env::var("XDG_STATE_HOME").ok();
            let saved_home = std::env::var("HOME").ok();
            Self {
                _lock: lock,
                saved_restore,
                saved_path,
                saved_xdg,
                saved_home,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            restore_env(WAYLAND_RESTORE_ENV, self.saved_restore.as_deref());
            restore_env(
                WAYLAND_RESTORE_TOKEN_PATH_ENV,
                self.saved_path.as_deref(),
            );
            restore_env("XDG_STATE_HOME", self.saved_xdg.as_deref());
            restore_env("HOME", self.saved_home.as_deref());
        }
    }

    fn restore_env(key: &str, value: Option<&str>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn restore_enabled_parses_truthy_values() {
        let _guard = EnvGuard::new();
        std::env::set_var(WAYLAND_RESTORE_ENV, "1");
        assert!(restore_enabled());
        std::env::set_var(WAYLAND_RESTORE_ENV, "true");
        assert!(restore_enabled());
        std::env::remove_var(WAYLAND_RESTORE_ENV);
        assert!(!restore_enabled());
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn resolve_token_path_uses_xdg_state_home() {
        let _guard = EnvGuard::new();
        let base = std::env::temp_dir().join(format!(
            "eidolon-wayland-restore-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&base);
        std::env::set_var("XDG_STATE_HOME", base.to_string_lossy().as_ref());
        std::env::remove_var(WAYLAND_RESTORE_TOKEN_PATH_ENV);
        let path = resolve_token_path().expect("resolve");
        assert_eq!(
            path,
            base.join("eidolon").join(TOKEN_FILE_NAME)
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn load_save_token_round_trip() {
        let _guard = EnvGuard::new();
        let dir = std::env::temp_dir().join(format!(
            "eidolon-wayland-restore-rt-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("token");
        assert_eq!(load_token(&path).expect("load missing"), None);
        save_token(&path, "portal-restore-abc").expect("save");
        assert_eq!(
            load_token(&path).expect("load"),
            Some("portal-restore-abc".into())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn restore_config_disabled_uses_do_not_persist() {
        let _guard = EnvGuard::new();
        std::env::remove_var(WAYLAND_RESTORE_ENV);
        let cfg = RestoreConfig::load().expect("load");
        assert!(!cfg.enabled);
        assert!(cfg.token.is_none());
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn restore_config_enabled_loads_token() {
        let _guard = EnvGuard::new();
        let dir = std::env::temp_dir().join(format!(
            "eidolon-wayland-restore-cfg-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("token");
        save_token(&path, "stored-token").expect("seed");
        std::env::set_var(WAYLAND_RESTORE_ENV, "1");
        std::env::set_var(WAYLAND_RESTORE_TOKEN_PATH_ENV, path.to_string_lossy().as_ref());
        let cfg = RestoreConfig::load().expect("load");
        assert!(cfg.enabled);
        assert_eq!(cfg.token.as_deref(), Some("stored-token"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Traces to: FR-EIDOLON-001
    #[test]
    fn restore_io_uses_stable_code() {
        let err = restore_io("test", "detail");
        assert_eq!(
            err.unsupported_code(),
            Some(codes::DESKTOP_LINUX_WAYLAND_RESTORE_IO)
        );
    }
}
