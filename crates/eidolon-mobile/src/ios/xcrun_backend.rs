//! Live iOS driver via system `xcrun` / `simctl` / `xcodebuild` (`mobile-ios`).
//!
//! Fresh CLI port — do not unarchive kmobile. Hermetic path: version probe on
//! construct. List devices via `simctl list`. Screenshot via `simctl io` when
//! gated. Tap / swipe / text / viewport via [`super::XcuiBridge`]
//! (`EIDOLON_IOS_XCUI_BUNDLE` wins, else discovered `eidolon-xcui-helper`, or
//! best-effort AppleScript when `EIDOLON_IOS_ALLOW_APPLESCRIPT=1`) — never
//! pretend success when missing.

use super::{probe, resolve_xcui_bridge, IosXcTestDriver, XcuiBridge};
use crate::cli::{env_device_id, require_actions_allowed};
use crate::codes;
use crate::{DeviceInfo, Modality};
use eidolon_core::error::PhenoError;
use eidolon_core::traits::MobileAutomator;
use eidolon_core::{AutomationEvent, Result, Viewport};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn ios_unavailable(method: &str, detail: impl std::fmt::Display) -> PhenoError {
    PhenoError::unsupported_platform(
        codes::MOBILE_IOS_STUB,
        format!(
            "IosXcTestDriver::{method} unavailable — Xcode CLI not reachable \
             ({detail}); enable feature `mobile-ios` and install `xcrun` \
             (EIDOLON_XCRUN), or use IosStub (docs/EXTRACTION_PLAN.md; do not \
             unarchive kmobile routinely)"
        ),
    )
}

/// Live `xcrun`-backed iOS automation client (`MobileAutomator`).
///
/// Replaces the fail-loud [`super::IosStub`] path when feature `mobile-ios`
/// is enabled and system tools answer version probes.
pub struct XcrunIosDriver {
    xcrun: PathBuf,
    xcodebuild: Option<PathBuf>,
    simctl: Option<PathBuf>,
    version_line: String,
    device_id: Option<String>,
    /// XCUI / AppleScript action bridge (resolved at construct; re-check env
    /// via [`Self::refresh_xcui_bridge`] after setting helper env in tests).
    xcui: Box<dyn XcuiBridge>,
}

impl XcrunIosDriver {
    /// Resolve tools and prove `xcrun --version` answers.
    pub fn try_new() -> Result<Self> {
        let Some(xcrun) = probe::resolve_xcrun() else {
            return Err(ios_unavailable("try_new", "xcrun not found on PATH"));
        };
        let Some(version_line) = Self::run_xcrun_version(&xcrun) else {
            return Err(ios_unavailable(
                "try_new",
                format!("xcrun --version failed ({})", xcrun.display()),
            ));
        };
        Ok(Self {
            xcrun,
            xcodebuild: probe::resolve_xcodebuild(),
            simctl: probe::resolve_simctl(),
            version_line,
            device_id: env_device_id(),
            xcui: resolve_xcui_bridge(),
        })
    }

    /// Host probe only — does not construct a client.
    pub fn host_xcrun_ready() -> bool {
        probe::xcrun_cli_ready()
    }

    /// `xcrun --version` line captured at construct.
    pub fn cli_version(&self) -> &str {
        &self.version_line
    }

    /// Optional device / simulator UDID (`EIDOLON_MOBILE_DEVICE` or setter).
    pub fn device_id(&self) -> Option<&str> {
        self.device_id.as_deref()
    }

    /// Pin a simulator / device UDID for screenshot / gated ops.
    pub fn with_device_id(mut self, id: impl Into<String>) -> Self {
        self.device_id = Some(id.into());
        self
    }

    /// Re-resolve [`XcuiBridge`] from current env (tests / late helper config).
    pub fn refresh_xcui_bridge(&mut self) {
        self.xcui = resolve_xcui_bridge();
    }

    /// Inject a bridge (tests). Prefer env + [`Self::refresh_xcui_bridge`].
    pub fn with_xcui_bridge(mut self, bridge: Box<dyn XcuiBridge>) -> Self {
        self.xcui = bridge;
        self
    }

    /// Active XCUI backend name (`bundle` / `applescript` / `missing`).
    pub fn xcui_backend(&self) -> &'static str {
        self.xcui.backend_name()
    }

    /// Whether the XCUI / AppleScript bridge reports ready.
    pub fn xcui_ready(&self) -> bool {
        self.xcui.ready()
    }

    /// List available iOS simulators via `xcrun simctl list devices -j`.
    ///
    /// Ungated (read-only). Returns empty vec when no available devices —
    /// never invents entries.
    pub fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let simctl = self.simctl.as_ref().ok_or_else(|| {
            ios_unavailable("list_devices", "simctl not resolvable via xcrun")
        })?;
        let output = Command::new(simctl)
            .args(["list", "devices", "-j"])
            .output()
            .map_err(|e| ios_unavailable("list_devices", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ios_unavailable(
                "list_devices",
                format!("simctl list failed: {}", stderr.trim()),
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_simctl_devices_json(&stdout)
    }

    /// Hermetic preflight: re-check `xcrun --version` (no simulator boot).
    pub fn hermetic_probe(&self) -> Result<()> {
        if Self::run_xcrun_version(&self.xcrun).is_none() {
            return Err(ios_unavailable(
                "hermetic_probe",
                format!("xcrun --version failed ({})", self.xcrun.display()),
            ));
        }
        Ok(())
    }

    /// Run an XCTest suite via `xcodebuild test` when a project path is given.
    ///
    /// Without `scheme`/`project`, fails loud — we never pretend a suite ran.
    pub fn run_xcodebuild_test(
        &self,
        project: &str,
        scheme: &str,
        destination: Option<&str>,
    ) -> Result<String> {
        require_actions_allowed("run_xcodebuild_test")?;
        let xcodebuild = self.xcodebuild.as_ref().ok_or_else(|| {
            ios_unavailable("run_xcodebuild_test", "xcodebuild not on PATH")
        })?;
        if project.trim().is_empty() || scheme.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "run_xcodebuild_test requires non-empty project and scheme".into(),
            ));
        }
        let dest = destination
            .map(str::to_string)
            .or_else(|| {
                self.device_id
                    .as_ref()
                    .map(|id| format!("platform=iOS Simulator,id={id}"))
            })
            .unwrap_or_else(|| "platform=iOS Simulator".to_string());
        let output = Command::new(xcodebuild)
            .args([
                "test",
                "-project",
                project,
                "-scheme",
                scheme,
                "-destination",
                &dest,
            ])
            .output()
            .map_err(|e| ios_unavailable("run_xcodebuild_test", e))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Err(PhenoError::Internal(format!(
                "{}: xcodebuild test failed: {}",
                codes::MOBILE_IOS_STUB,
                stderr.trim().lines().last().unwrap_or("unknown")
            )));
        }
        Ok(format!("{stdout}{stderr}"))
    }

    fn run_xcrun_version(bin: &PathBuf) -> Option<String> {
        let output = Command::new(bin).arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(str::to_string)
    }

    fn require_device(&self) -> Result<&str> {
        self.device_id.as_deref().ok_or_else(|| {
            PhenoError::BadRequest(format!(
                "device id required — set {} or call with_device_id",
                crate::cli::DEVICE_ENV
            ))
        })
    }
}

fn parse_simctl_devices_json(raw: &str) -> Result<Vec<DeviceInfo>> {
    let v: Value = serde_json::from_str(raw).map_err(|e| {
        PhenoError::Internal(format!(
            "{}: simctl JSON parse failed: {e}",
            codes::MOBILE_IOS_STUB
        ))
    })?;
    let mut out = Vec::new();
    let Some(devices) = v.get("devices").and_then(|d| d.as_object()) else {
        return Ok(out);
    };
    for (runtime, list) in devices {
        let Some(arr) = list.as_array() else {
            continue;
        };
        let platform = if runtime.contains("iOS") {
            "ios"
        } else if runtime.contains("tvOS") {
            "tvos"
        } else if runtime.contains("watchOS") {
            "watchos"
        } else {
            "ios"
        };
        for dev in arr {
            let available = dev
                .get("isAvailable")
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            if !available {
                continue;
            }
            let id = dev
                .get("udid")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                continue;
            }
            let name = dev
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("simulator")
                .to_string();
            let state = dev
                .get("state")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            out.push(DeviceInfo {
                id,
                name: format!("{name} ({state})"),
                platform: platform.to_string(),
                modality: Modality::Mobile,
                kind: Some("ios-simulator".into()),
                transport: Some("xcrun-simctl".into()),
            });
        }
    }
    Ok(out)
}

#[async_trait::async_trait]
impl MobileAutomator for XcrunIosDriver {
    async fn get_viewport(&self) -> Result<Viewport> {
        // Read-only introspection via XCUI helper (ungated like Android wm size).
        let udid = self.require_device()?;
        self.xcui.viewport(udid)
    }

    async fn screenshot(&self, path: &str) -> Result<()> {
        require_actions_allowed("screenshot")?;
        let udid = self.require_device()?;
        let simctl = self.simctl.as_ref().ok_or_else(|| {
            ios_unavailable("screenshot", "simctl not resolvable")
        })?;
        if path.trim().is_empty() {
            return Err(PhenoError::BadRequest(
                "screenshot path must be non-empty".into(),
            ));
        }
        let output = Command::new(simctl)
            .args(["io", udid, "screenshot", path])
            .output()
            .map_err(|e| ios_unavailable("screenshot", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PhenoError::Internal(format!(
                "{}: simctl screenshot failed: {}",
                codes::MOBILE_IOS_STUB,
                stderr.trim()
            )));
        }
        Ok(())
    }

    async fn tap(&self, x: i32, y: i32) -> Result<()> {
        require_actions_allowed("tap")?;
        let udid = self.require_device()?;
        self.xcui.tap(udid, x, y)
    }

    async fn swipe(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> Result<()> {
        require_actions_allowed("swipe")?;
        let udid = self.require_device()?;
        self.xcui.swipe(udid, x1, y1, x2, y2)
    }

    async fn input_text(&self, text: &str) -> Result<()> {
        require_actions_allowed("input_text")?;
        let udid = self.require_device()?;
        self.xcui.input_text(udid, text)
    }

    async fn record_event(&self, event: AutomationEvent) -> Result<()> {
        log::debug!("Recorded event (xcrun ios): {:?}", event);
        Ok(())
    }
}

#[async_trait::async_trait]
impl IosXcTestDriver for XcrunIosDriver {
    fn xctest_ready(&self) -> bool {
        // Ready when xcodebuild answers OR an XCUI helper bridge is configured.
        (self.xcodebuild.is_some() && probe::xcodebuild_cli_ready()) || self.xcui.ready()
    }

    fn xcrun_ready(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simctl_skips_unavailable() {
        let raw = r#"{
          "devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-18-0": [
              {
                "udid": "AAAA-BBBB",
                "name": "iPhone 16",
                "state": "Shutdown",
                "isAvailable": true
              },
              {
                "udid": "CCCC-DDDD",
                "name": "Broken",
                "state": "Shutdown",
                "isAvailable": false
              }
            ]
          }
        }"#;
        let devices = parse_simctl_devices_json(raw).unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "AAAA-BBBB");
        assert_eq!(devices[0].platform, "ios");
        assert_eq!(devices[0].transport.as_deref(), Some("xcrun-simctl"));
    }

    #[test]
    fn parse_empty_devices_object() {
        let devices = parse_simctl_devices_json(r#"{"devices":{}}"#).unwrap();
        assert!(devices.is_empty());
    }
}
