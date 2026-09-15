//! Live Android driver via system `adb` (`mobile-android`).
//!
//! Fresh CLI port — do not unarchive kmobile. Hermetic path: `adb version` on
//! construct. List devices via `adb devices -l`. Destructive `input` /
//! screencap properly gated by `EIDOLON_MOBILE_ALLOW_ACTIONS=1`. When gated,
//! `adb shell input tap|swipe|text` and `exec-out screencap` are **real**
//! (fail loud on empty device list / non-zero exit — never pretend success).
//!
//! UiAutomator2 instrumentation: use [`Self::uia2`] / [`Self::uia2_http`]
//! (feature `mobile-uia2`) with env override or pinned Appium APK cache —
//! see [`super::uia2_server`], [`super::uia2_assets`], and [`super::uia2_http`].

use super::{probe, AndroidUiAutomatorDriver};
use crate::cli::{env_device_id, require_actions_allowed};
use crate::codes;
use crate::{DeviceInfo, Modality};
use eidolon_core::error::PhenoError;
use eidolon_core::traits::MobileAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};
use std::path::PathBuf;
use std::process::Command;

fn android_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::MOBILE_ANDROID_STUB,
        format!(
            "AndroidUiAutomatorDriver::{method} unavailable — `adb` CLI not \
             reachable ({detail}); enable feature `mobile-android` and install \
             platform-tools (EIDOLON_ADB), or use AndroidStub \
             (docs/EXTRACTION_PLAN.md; do not unarchive kmobile routinely)"
        ),
    )
}

/// Live `adb`-backed Android automation client (`MobileAutomator`).
pub struct AdbAndroidDriver {
    adb: PathBuf,
    version_line: String,
    device_id: Option<String>,
}

impl AdbAndroidDriver {
    /// Resolve `adb` and prove it answers `version`.
    pub fn try_new() -> Result<Self> {
        let Some(adb) = probe::resolve_adb() else {
            return Err(android_unavailable("try_new", "adb not found on PATH"));
        };
        let Some(version_line) = Self::run_version(&adb) else {
            return Err(android_unavailable(
                "try_new",
                format!("adb version failed ({})", adb.display()),
            ));
        };
        Ok(Self {
            adb,
            version_line,
            device_id: env_device_id(),
        })
    }

    /// Host probe only — does not construct a client.
    pub fn host_adb_ready() -> bool {
        probe::adb_cli_ready()
    }

    /// `adb version` line captured at construct.
    pub fn cli_version(&self) -> &str {
        &self.version_line
    }

    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    pub fn with_device_id(mut self, id: impl Into<String>) -> Self {
        self.device_id = Some(id.into());
        self
    }

    /// List attached devices via `adb devices -l` (ungated, read-only).
    pub fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let output = self
            .adb_cmd(&["devices", "-l"])
            .map_err(|e| android_unavailable("list_devices", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(android_unavailable(
                "list_devices",
                format!("adb devices failed: {}", stderr.trim()),
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_adb_devices(&stdout))
    }

    /// Hermetic preflight: re-check `adb version` (does not require a device).
    pub fn hermetic_probe(&self) -> Result<()> {
        if Self::run_version(&self.adb).is_none() {
            return Err(android_unavailable(
                "hermetic_probe",
                format!("adb version failed ({})", self.adb.display()),
            ));
        }
        Ok(())
    }

    /// Run a UiAutomator shell command (`adb shell uiautomator …`).
    pub fn uiautomator_execute(&self, args: &[&str]) -> Result<String> {
        require_actions_allowed("uiautomator_execute")?;
        let mut full = vec!["shell", "uiautomator"];
        full.extend_from_slice(args);
        let output = self
            .adb_cmd(&full)
            .map_err(|e| android_unavailable("uiautomator_execute", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Err(PhenoError::Internal(format!(
                "{}: uiautomator failed: {}",
                codes::MOBILE_ANDROID_STUB,
                stderr.trim().lines().last().unwrap_or("unknown")
            )));
        }
        Ok(stdout.into_owned())
    }

    /// Construct a [`super::Uia2Server`] handle sharing this driver's `adb` /
    /// device id (Appium-shaped install/start/stop).
    ///
    /// Does not install or start the server. Callers probe via
    /// [`super::Uia2Server::packages_installed`] / `server_process_running`
    /// and install via [`super::Uia2Server::install_from_env`] (env → assets →
    /// durable cache / `Uia2ApkPaths::ensure`).
    pub fn uia2(&self) -> super::Uia2Server {
        let mut server = super::Uia2Server::new(self.adb.clone());
        if let Some(id) = &self.device_id {
            server = server.with_device_id(id.clone());
        }
        server
    }

    /// Probe whether Appium UIA2 packages are installed on the target device.
    ///
    /// Fail-loud on adb errors; returns `Ok(false)` when packages absent
    /// (never invents readiness).
    pub fn uia2_packages_ready(&self) -> Result<bool> {
        Ok(self.uia2().packages_installed()?.both_installed())
    }

    /// Appium UIA2 HTTP session client against the forwarded local port.
    ///
    /// Fail-loud unless [`Self::uia2_server_ready`] is true (packages installed
    /// + process running). Destructive session/click verbs still need
    /// `EIDOLON_MOBILE_ALLOW_ACTIONS=1`. Feature `mobile-uia2`.
    ///
    /// wraps: ureq via [`super::Uia2HttpClient`].
    #[cfg(feature = "mobile-uia2")]
    pub fn uia2_http(&self) -> Result<super::Uia2HttpClient> {
        if !self.uia2_server_ready() {
            return Err(PhenoError::unsupported_platform(
                codes::MOBILE_UIA2_UNAVAILABLE,
                "AdbAndroidDriver::uia2_http unavailable — UIA2 server not \
                 ready (packages missing or process not running); install/start \
                 via Uia2Server::install_from_env (env / assets / \
                 eidolon-fetch-uia2), adb forward, then retry \
                 (crates/eidolon-mobile/assets/uia2/README.md; do not \
                 unarchive kmobile)",
            ));
        }
        let port = self.uia2().port();
        Ok(super::Uia2HttpClient::new(super::DEFAULT_HTTP_HOST, port))
    }

    /// Like [`Self::uia2_http`] but skips the process readiness probe.
    ///
    /// Still fail-loud on HTTP errors. Use when the caller already verified
    /// forward + server (e.g. integration tests). Feature `mobile-uia2`.
    #[cfg(feature = "mobile-uia2")]
    pub fn uia2_http_at_port(&self, port: u16) -> super::Uia2HttpClient {
        super::Uia2HttpClient::new(super::DEFAULT_HTTP_HOST, port)
    }

    fn run_version(bin: &PathBuf) -> Option<String> {
        let output = Command::new(bin).arg("version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    fn adb_cmd(&self, args: &[&str]) -> std::io::Result<std::process::Output> {
        let mut cmd = Command::new(&self.adb);
        if let Some(id) = &self.device_id {
            cmd.args(["-s", id]);
        }
        cmd.args(args).output()
    }

    fn require_device_or_any(&self) -> Result<()> {
        // `adb` without `-s` targets the sole device; with multiple, callers
        // must set device_id. We only enforce when list shows >1 and unset.
        let devices = self.list_devices()?;
        if self.device_id.is_none() && devices.len() > 1 {
            return Err(PhenoError::BadRequest(format!(
                "multiple adb devices — set {} or call with_device_id",
                crate::cli::DEVICE_ENV
            )));
        }
        if devices.is_empty() {
            return Err(android_unavailable(
                "device",
                "no devices attached (adb devices empty)",
            ));
        }
        Ok(())
    }

    fn parse_wm_size(raw: &str) -> Option<Viewport> {
        // Typical: "Physical size: 1080x1920"
        for line in raw.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("Physical size:") {
                let dims = rest.trim();
                let mut parts = dims.split('x');
                let w: u32 = parts.next()?.parse().ok()?;
                let h: u32 = parts.next()?.parse().ok()?;
                return Some(Viewport::new(w, h, 1.0));
            }
            if let Some(rest) = line.strip_prefix("Override size:") {
                let dims = rest.trim();
                let mut parts = dims.split('x');
                let w: u32 = parts.next()?.parse().ok()?;
                let h: u32 = parts.next()?.parse().ok()?;
                return Some(Viewport::new(w, h, 1.0));
            }
        }
        None
    }
}

/// Build `adb shell input tap` argv (hermetic; no device).
pub fn input_tap_args(x: i32, y: i32) -> Vec<String> {
    vec![
        "shell".into(),
        "input".into(),
        "tap".into(),
        x.to_string(),
        y.to_string(),
    ]
}

/// Build `adb shell input swipe` argv (hermetic; no device).
pub fn input_swipe_args(x1: i32, y1: i32, x2: i32, y2: i32) -> Vec<String> {
    vec![
        "shell".into(),
        "input".into(),
        "swipe".into(),
        x1.to_string(),
        y1.to_string(),
        x2.to_string(),
        y2.to_string(),
    ]
}

/// Escape text for `adb shell input text` (spaces → `%s`).
///
/// Shell metacharacters beyond space are left to the caller / device; we do
/// not silently strip them.
pub fn escape_adb_input_text(text: &str) -> String {
    text.replace(' ', "%s")
}

/// Build `adb shell input text` argv (hermetic; no device).
pub fn input_text_args(text: &str) -> Vec<String> {
    vec![
        "shell".into(),
        "input".into(),
        "text".into(),
        escape_adb_input_text(text),
    ]
}

/// Parse `adb devices -l` stdout into [`DeviceInfo`] rows (skips offline).
pub fn parse_adb_devices(raw: &str) -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    for line in raw.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(id) = parts.next() else {
            continue;
        };
        let Some(state) = parts.next() else {
            continue;
        };
        if state != "device" {
            continue;
        }
        let mut model = None;
        let mut product = None;
        for token in parts {
            if let Some(v) = token.strip_prefix("model:") {
                model = Some(v.to_string());
            } else if let Some(v) = token.strip_prefix("product:") {
                product = Some(v.to_string());
            }
        }
        let name = model
            .or(product)
            .unwrap_or_else(|| format!("android-{id}"));
        out.push(DeviceInfo {
            id: id.to_string(),
            name,
            platform: "android".into(),
            modality: Modality::Mobile,
            kind: Some("android-adb".into()),
            transport: Some("adb".into()),
        });
    }
    out
}

#[async_trait::async_trait]
impl MobileAutomator for AdbAndroidDriver {
    async fn get_viewport(&self) -> Result<Viewport> {
        self.require_device_or_any()?;
        let output = self
            .adb_cmd(&["shell", "wm", "size"])
            .map_err(|e| android_unavailable("get_viewport", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(android_unavailable(
                "get_viewport",
                format!("wm size failed: {}", stderr.trim()),
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_wm_size(&stdout).ok_or_else(|| {
            android_unavailable("get_viewport", format!("unparsed wm size: {stdout}"))
        })
    }

    async fn screenshot(&self, path: &str) -> Result<()> {
        require_actions_allowed("screenshot")?;
        self.require_device_or_any()?;
        if path.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "screenshot path must be non-empty".into(),
            ));
        }
        // Pull PNG via exec-out screencap.
        let output = self
            .adb_cmd(&["exec-out", "screencap", "-p"])
            .map_err(|e| android_unavailable("screenshot", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PhenoError::Internal(format!(
                "{}: screencap failed: {}",
                codes::MOBILE_ANDROID_STUB,
                stderr.trim()
            )));
        }
        std::fs::write(path, &output.stdout).map_err(|e| {
            PhenoError::Internal(format!(
                "{}: write screenshot failed: {e}",
                codes::MOBILE_ANDROID_STUB
            ))
        })?;
        Ok(())
    }

    async fn tap(&self, x: i32, y: i32) -> Result<()> {
        require_actions_allowed("tap")?;
        self.require_device_or_any()?;
        let args = input_tap_args(x, y);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .adb_cmd(&arg_refs)
            .map_err(|e| android_unavailable("tap", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PhenoError::Internal(format!(
                "{}: input tap failed: {}",
                codes::MOBILE_ANDROID_STUB,
                stderr.trim()
            )));
        }
        Ok(())
    }

    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()> {
        require_actions_allowed("swipe")?;
        self.require_device_or_any()?;
        let args = input_swipe_args(x1, y1, x2, y2);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .adb_cmd(&arg_refs)
            .map_err(|e| android_unavailable("swipe", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PhenoError::Internal(format!(
                "{}: input swipe failed: {}",
                codes::MOBILE_ANDROID_STUB,
                stderr.trim()
            )));
        }
        Ok(())
    }

    async fn input_text(&self, text: &str) -> Result<()> {
        require_actions_allowed("input_text")?;
        self.require_device_or_any()?;
        let args = input_text_args(text);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self
            .adb_cmd(&arg_refs)
            .map_err(|e| android_unavailable("input_text", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PhenoError::Internal(format!(
                "{}: input text failed: {}",
                codes::MOBILE_ANDROID_STUB,
                stderr.trim()
            )));
        }
        Ok(())
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event (adb android): {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl AndroidUiAutomatorDriver for AdbAndroidDriver {
    fn uiautomator_ready(&self) -> bool {
        // Shell `uiautomator` one-shot is available whenever adb is.
        // Appium UIA2 instrumentation server is separate — see uia2_server_ready.
        true
    }

    fn adb_ready(&self) -> bool {
        true
    }

    fn uia2_server_ready(&self) -> bool {
        // Honest: only true when packages are installed AND process is running.
        // Probe failures → false (never invent readiness).
        match self.uia2().packages_installed() {
            Ok(status) if status.both_installed() => {
                self.uia2().server_process_running().unwrap_or(false)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_adb_devices_skips_offline() {
        let raw = "List of devices attached\n\
                   emulator-5554          device product:sdk_gphone model:sdk_gphone_x86\n\
                   deadbeef               offline\n\
                   ";
        let devices = parse_adb_devices(raw);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "emulator-5554");
        assert_eq!(devices[0].platform, "android");
        assert!(devices[0].name.contains("sdk_gphone") || devices[0].name.contains("emulator"));
    }

    #[test]
    fn parse_wm_size_physical() {
        let vp = AdbAndroidDriver::parse_wm_size("Physical size: 1080x2400\n").unwrap();
        assert_eq!(vp.width, 1080);
        assert_eq!(vp.height, 2400);
    }

    #[test]
    fn input_argv_builders_are_hermetic() {
        assert_eq!(
            input_tap_args(10, 20),
            vec!["shell", "input", "tap", "10", "20"]
        );
        assert_eq!(
            input_swipe_args(1, 2, 3, 4),
            vec!["shell", "input", "swipe", "1", "2", "3", "4"]
        );
        assert_eq!(escape_adb_input_text("hello world"), "hello%sworld");
        assert_eq!(
            input_text_args("a b"),
            vec!["shell", "input", "text", "a%sb"]
        );
    }
}
