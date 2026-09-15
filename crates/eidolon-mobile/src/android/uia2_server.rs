//! UiAutomator2 server lifecycle via Appium-shaped `adb` orchestration.
//!
//! APK resolution (see [`crate::android::uia2_assets`]):
//! 1. Env — [`crate::cli::UIA2_APK_ENV`] / [`crate::cli::UIA2_TEST_APK_ENV`]
//! 2. Checkout `assets/uia2/` (optional local drop, SHA-256 verified)
//! 3. Durable cache (`EIDOLON_UIA2_CACHE` or `~/.cache/eidolon/uia2/<ver>/`)
//! 4. Fail loud — or [`Uia2ApkPaths::ensure`] to fetch pinned Appium release APKs
//!
//! Hermetic helpers (argv / package parsing) need no device. Install / start /
//! stop require `adb` + device + [`crate::cli::ACTIONS_ALLOW_ENV`]`=1`.
//! Missing APK or uninstalled packages → [`codes::MOBILE_UIA2_UNAVAILABLE`].
//!
//! Do not unarchive kmobile.

use crate::android::uia2_assets;
use crate::cli::{
    env_uia2_port, require_actions_allowed, UIA2_APK_ENV, UIA2_PORT_ENV, UIA2_TEST_APK_ENV,
};
use crate::codes;
use eidolon_core::error::PhenoError;
use eidolon_core::Result;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Default Appium UiAutomator2 server package id.
pub const DEFAULT_SERVER_PACKAGE: &str = "io.appium.uiautomator2.server";

/// Default Appium UiAutomator2 test/instrumentation package id.
pub const DEFAULT_TEST_PACKAGE: &str = "io.appium.uiautomator2.server.test";

/// Default AndroidJUnitRunner component (Appium UIA2).
pub const DEFAULT_RUNNER: &str = "androidx.test.runner.AndroidJUnitRunner";

/// Default device listen port (Appium UIA2 server).
pub const DEFAULT_DEVICE_PORT: u16 = 6790;

fn uia2_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::MOBILE_UIA2_UNAVAILABLE,
        format!(
            "Uia2Server::{method} unavailable — {detail}; resolve APKs via \
             {UIA2_APK_ENV}+{UIA2_TEST_APK_ENV} (override) → assets/uia2 → \
             durable cache, or `eidolon-fetch-uia2` / Uia2ApkPaths::ensure(); \
             feature `mobile-uia2`; install/start gated by \
             EIDOLON_MOBILE_ALLOW_ACTIONS=1; see \
             crates/eidolon-mobile/assets/uia2/README.md \
             (do not unarchive kmobile)"
        ),
    )
}

/// Resolved Appium UiAutomator2 APK paths (server + instrumentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uia2ApkPaths {
    pub server: PathBuf,
    pub test: PathBuf,
}

impl Uia2ApkPaths {
    /// Resolve APKs: env override → checkout assets → durable cache.
    ///
    /// Does not download. Prefer [`Self::ensure`] for the happy path when the
    /// cache may be cold. Env-only legacy name: [`Self::from_env`] delegates here.
    pub fn resolve() -> Result<Self> {
        uia2_assets::resolve()
    }

    /// Resolve, fetching pinned official APKs into the durable cache on miss.
    ///
    /// Requires feature `mobile-uia2`. Env overrides still win (no download).
    #[cfg(feature = "mobile-uia2")]
    pub fn ensure() -> Result<Self> {
        uia2_assets::ensure()
    }

    /// Alias for [`Self::resolve`] (historical name; no longer env-only).
    pub fn from_env() -> Result<Self> {
        Self::resolve()
    }

    /// Construct when caller already validated paths exist.
    pub fn new(server: PathBuf, test: PathBuf) -> Self {
        Self { server, test }
    }
}

/// Package presence snapshot from `pm list packages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Uia2PackageStatus {
    pub server_installed: bool,
    pub test_installed: bool,
}

impl Uia2PackageStatus {
    pub fn both_installed(self) -> bool {
        self.server_installed && self.test_installed
    }
}

/// Parse `pm list packages` stdout for Appium UIA2 package ids.
pub fn parse_package_status(pm_list_stdout: &str) -> Uia2PackageStatus {
    let mut status = Uia2PackageStatus::default();
    for line in pm_list_stdout.lines() {
        let line = line.trim();
        let pkg = line.strip_prefix("package:").unwrap_or(line);
        if pkg == DEFAULT_SERVER_PACKAGE {
            status.server_installed = true;
        } else if pkg == DEFAULT_TEST_PACKAGE {
            status.test_installed = true;
        }
    }
    status
}

/// Build `adb install -r <apk>` argv (hermetic; no process).
pub fn install_apk_args(apk: &Path) -> Vec<String> {
    vec!["install".into(), "-r".into(), apk.display().to_string()]
}

/// Build `adb forward tcp:<local> tcp:<device>` argv.
pub fn forward_port_args(local: u16, device: u16) -> Vec<String> {
    vec![
        "forward".into(),
        format!("tcp:{local}"),
        format!("tcp:{device}"),
    ]
}

/// Build `adb shell am instrument -w <test_pkg>/<runner>` argv.
pub fn instrument_start_args(test_package: &str, runner: &str) -> Vec<String> {
    vec![
        "shell".into(),
        "am".into(),
        "instrument".into(),
        "-w".into(),
        format!("{test_package}/{runner}"),
    ]
}

/// Build `adb shell am force-stop <server_pkg>` argv.
pub fn force_stop_args(server_package: &str) -> Vec<String> {
    vec![
        "shell".into(),
        "am".into(),
        "force-stop".into(),
        server_package.into(),
    ]
}

/// Build `adb shell pm list packages` argv.
pub fn pm_list_packages_args() -> Vec<&'static str> {
    vec!["shell", "pm", "list", "packages"]
}

/// Build `adb shell pidof <package>` argv (presence probe).
pub fn pidof_args(package: &str) -> Vec<String> {
    vec!["shell".into(), "pidof".into(), package.into()]
}

/// True when `pidof` stdout has a non-empty pid token.
pub fn parse_pidof_running(stdout: &str) -> bool {
    stdout.split_whitespace().any(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()))
}

/// UiAutomator2 lifecycle controller over an `adb` binary path.
///
/// Probe methods are ungated. Install / start / stop / forward require
/// [`require_actions_allowed`].
pub struct Uia2Server {
    adb: PathBuf,
    device_id: Option<String>,
    port: u16,
}

impl Uia2Server {
    /// Construct with resolved `adb` path (caller proved `adb version`).
    pub fn new(adb: PathBuf) -> Self {
        Self {
            adb,
            device_id: None,
            port: env_uia2_port(),
        }
    }

    pub fn with_device_id(mut self, id: impl Into<String>) -> Self {
        self.device_id = Some(id.into());
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    /// Whether both Appium UIA2 packages are installed (device required).
    pub fn packages_installed(&self) -> Result<Uia2PackageStatus> {
        let output = self
            .adb_cmd(&pm_list_packages_args())
            .map_err(|e| uia2_unavailable("packages_installed", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(uia2_unavailable(
                "packages_installed",
                format!("pm list packages failed: {}", stderr.trim()),
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_package_status(&stdout))
    }

    /// True when server process appears in `pidof` (best-effort).
    pub fn server_process_running(&self) -> Result<bool> {
        let args = pidof_args(DEFAULT_SERVER_PACKAGE);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .adb_cmd(&arg_refs)
            .map_err(|e| uia2_unavailable("server_process_running", e))?;
        // pidof returns non-zero when no process — treat as not running.
        if !output.status.success() {
            return Ok(false);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_pidof_running(&stdout))
    }

    /// Fail-loud unless packages are installed (no silent degrade).
    pub fn require_packages_installed(&self) -> Result<Uia2PackageStatus> {
        let status = self.packages_installed()?;
        if !status.both_installed() {
            return Err(uia2_unavailable(
                "require_packages_installed",
                format!(
                    "server_installed={} test_installed={} — install via \
                     Uia2Server::install_from_env (env / vendored cache) \
                     or adb install",
                    status.server_installed, status.test_installed
                ),
            ));
        }
        Ok(status)
    }

    /// Install resolved APKs (env → assets → cache; fetch on miss when
    /// `mobile-uia2`). Gated by [`require_actions_allowed`].
    pub fn install_from_env(&self) -> Result<()> {
        require_actions_allowed("uia2_install")?;
        #[cfg(feature = "mobile-uia2")]
        let paths = Uia2ApkPaths::ensure()?;
        #[cfg(not(feature = "mobile-uia2"))]
        let paths = Uia2ApkPaths::resolve()?;
        self.install_apks(&paths)
    }

    /// Install provided APKs (`adb install -r`). Gated.
    pub fn install_apks(&self, paths: &Uia2ApkPaths) -> Result<()> {
        require_actions_allowed("uia2_install")?;
        if !paths.server.is_file() {
            return Err(uia2_unavailable(
                "install_apks",
                format!("server APK missing: {}", paths.server.display()),
            ));
        }
        if !paths.test.is_file() {
            return Err(uia2_unavailable(
                "install_apks",
                format!("test APK missing: {}", paths.test.display()),
            ));
        }
        self.run_install(&paths.server)?;
        self.run_install(&paths.test)?;
        Ok(())
    }

    /// Forward host↔device UIA2 port (default [`DEFAULT_DEVICE_PORT`]). Gated.
    pub fn forward_port(&self) -> Result<()> {
        require_actions_allowed("uia2_forward")?;
        let args = forward_port_args(self.port, self.port);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .adb_cmd(&arg_refs)
            .map_err(|e| uia2_unavailable("forward_port", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(uia2_unavailable(
                "forward_port",
                format!(
                    "adb forward tcp:{} failed ({UIA2_PORT_ENV}): {}",
                    self.port,
                    stderr.trim()
                ),
            ));
        }
        Ok(())
    }

    /// Start instrumentation (`am instrument -w`). Gated. Blocks until runner exits
    /// unless caller backgrounds separately — honest CLI orchestration. HTTP
    /// session verbs: `Uia2HttpClient` behind feature `mobile-uia2`.
    pub fn start_instrumentation(&self) -> Result<Output> {
        require_actions_allowed("uia2_start")?;
        self.require_packages_installed()?;
        let args = instrument_start_args(DEFAULT_TEST_PACKAGE, DEFAULT_RUNNER);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.adb_cmd(&arg_refs)
            .map_err(|e| uia2_unavailable("start_instrumentation", e))
    }

    /// Force-stop the server package. Gated.
    pub fn stop(&self) -> Result<()> {
        require_actions_allowed("uia2_stop")?;
        let args = force_stop_args(DEFAULT_SERVER_PACKAGE);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .adb_cmd(&arg_refs)
            .map_err(|e| uia2_unavailable("stop", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(uia2_unavailable(
                "stop",
                format!("force-stop failed: {}", stderr.trim()),
            ));
        }
        Ok(())
    }

    /// Probe APK resolution without install. Ungated. No network.
    pub fn hermetic_apk_env_probe() -> Result<Uia2ApkPaths> {
        Uia2ApkPaths::resolve()
    }

    fn run_install(&self, apk: &Path) -> Result<()> {
        let args = install_apk_args(apk);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .adb_cmd(&arg_refs)
            .map_err(|e| uia2_unavailable("install", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(uia2_unavailable(
                "install",
                format!("adb install failed for {}: {}", apk.display(), stderr.trim()),
            ));
        }
        Ok(())
    }

    fn adb_cmd(&self, args: &[&str]) -> std::io::Result<Output> {
        let mut cmd = Command::new(&self.adb);
        if let Some(id) = &self.device_id {
            cmd.args(["-s", id]);
        }
        cmd.args(args).output()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::env_uia2_apk;

    #[test]
    fn parse_package_status_detects_appium_ids() {
        let raw = "package:com.android.settings\n\
                   package:io.appium.uiautomator2.server\n\
                   package:io.appium.uiautomator2.server.test\n";
        let s = parse_package_status(raw);
        assert!(s.both_installed());
        assert!(s.server_installed && s.test_installed);
    }

    #[test]
    fn parse_package_status_partial() {
        let s = parse_package_status("package:io.appium.uiautomator2.server\n");
        assert!(s.server_installed);
        assert!(!s.test_installed);
        assert!(!s.both_installed());
    }

    #[test]
    fn argv_builders_are_hermetic() {
        let apk = PathBuf::from("/tmp/server.apk");
        assert_eq!(
            install_apk_args(&apk),
            vec!["install", "-r", "/tmp/server.apk"]
        );
        assert_eq!(
            forward_port_args(6790, 6790),
            vec!["forward", "tcp:6790", "tcp:6790"]
        );
        assert_eq!(
            instrument_start_args(DEFAULT_TEST_PACKAGE, DEFAULT_RUNNER),
            vec![
                "shell",
                "am",
                "instrument",
                "-w",
                "io.appium.uiautomator2.server.test/androidx.test.runner.AndroidJUnitRunner"
            ]
        );
        assert_eq!(
            force_stop_args(DEFAULT_SERVER_PACKAGE),
            vec!["shell", "am", "force-stop", DEFAULT_SERVER_PACKAGE]
        );
        assert_eq!(pm_list_packages_args(), vec!["shell", "pm", "list", "packages"]);
    }

    #[test]
    fn parse_pidof_running_digits() {
        assert!(parse_pidof_running("12345\n"));
        assert!(parse_pidof_running("123 456"));
        assert!(!parse_pidof_running(""));
        assert!(!parse_pidof_running("not-a-pid"));
    }

    #[test]
    fn resolve_succeeds_or_fails_loud_without_silent_empty() {
        // When env unset: vendored/cache may satisfy resolve; otherwise fail loud.
        if env_uia2_apk().is_some() {
            return;
        }
        match Uia2ApkPaths::resolve() {
            Ok(paths) => {
                assert!(paths.server.is_file(), "resolved server must exist");
                assert!(paths.test.is_file(), "resolved test must exist");
            }
            Err(err) => {
                assert_eq!(
                    err.unsupported_code(),
                    Some(codes::MOBILE_UIA2_UNAVAILABLE)
                );
                assert_eq!(err.status_code(), 501);
            }
        }
    }

    #[test]
    fn default_constants_match_appium() {
        assert_eq!(DEFAULT_SERVER_PACKAGE, "io.appium.uiautomator2.server");
        assert_eq!(DEFAULT_TEST_PACKAGE, "io.appium.uiautomator2.server.test");
        assert_eq!(DEFAULT_DEVICE_PORT, 6790);
    }
}
