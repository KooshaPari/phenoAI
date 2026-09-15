//! Concrete CLI adapter implementing [`kmobile_core::ports::SimulatorPort`].
//!
//! Discovers Android emulators (via `emulator -list-avds`) and iOS
//! simulators (via `xcrun simctl list devices --json`), and exposes
//! start / stop / reset / install operations against them.

use std::process::Command;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use kmobile_core::config::Config;
use kmobile_core::error::KMobileError;
use kmobile_core::ports::{SimulatorInfo, SimulatorPort};

/// Lifecycle state of a simulator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SimulatorStatus {
    Booted,
    Shutdown,
    Booting,
    ShuttingDown,
}

/// Internal record for one simulator the adapter has seen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorRecord {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
    pub status: SimulatorStatus,
    pub device_type: String,
}

/// CLI-side [`SimulatorPort`] implementation.
pub struct CliSimulatorAdapter {
    config: Config,
    android_emulators: Vec<SimulatorRecord>,
    ios_simulators: Vec<SimulatorRecord>,
}

impl CliSimulatorAdapter {
    /// Build the adapter and refresh the simulator cache.
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let mut adapter = Self {
            config,
            android_emulators: Vec::new(),
            ios_simulators: Vec::new(),
        };
        adapter.refresh().await?;
        Ok(adapter)
    }

    /// Snapshot of the cached Android emulators.
    #[expect(dead_code)]
    pub fn android_emulators(&self) -> &[SimulatorRecord] {
        &self.android_emulators
    }

    /// Snapshot of the cached iOS simulators.
    #[expect(dead_code)]
    pub fn ios_simulators(&self) -> &[SimulatorRecord] {
        &self.ios_simulators
    }

    /// Re-scan `emulator -list-avds` and `simctl list devices` and update
    /// the cache. Failure on one platform does not abort the other.
    pub async fn refresh(&mut self) -> anyhow::Result<()> {
        info!("Refreshing simulator list");
        if let Err(e) = self.refresh_android().await {
            warn!("Failed to refresh Android emulators: {e}");
        }
        if let Err(e) = self.refresh_ios().await {
            warn!("Failed to refresh iOS simulators: {e}");
        }
        Ok(())
    }

    async fn refresh_android(&mut self) -> anyhow::Result<()> {
        let emulator = self.emulator_path()?;
        debug!("Running `emulator -list-avds`");
        let output = Command::new(emulator).args(["-list-avds"]).output()?;
        if !output.status.success() {
            return Err(
                KMobileError::CommandError("Failed to list Android emulators".to_string()).into(),
            );
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        self.android_emulators.clear();
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let avd = line.trim();
            let status = self.android_emulator_status(avd).await?;
            self.android_emulators.push(SimulatorRecord {
                id: avd.to_string(),
                name: avd.to_string(),
                platform: "android".to_string(),
                version: "unknown".to_string(),
                status,
                device_type: "emulator".to_string(),
            });
        }
        info!("Found {} Android emulator(s)", self.android_emulators.len());
        Ok(())
    }

    async fn android_emulator_status(&self, avd: &str) -> anyhow::Result<SimulatorStatus> {
        let adb = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;
        let output = Command::new(adb).args(["devices"]).output()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains(avd) && line.contains("device") {
                    return Ok(SimulatorStatus::Booted);
                }
            }
        }
        Ok(SimulatorStatus::Shutdown)
    }

    async fn refresh_ios(&mut self) -> anyhow::Result<()> {
        debug!("Running `xcrun simctl list devices --json`");
        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", "--json"])
            .output()?;
        if !output.status.success() {
            debug!("`simctl` unavailable, no iOS simulators");
            self.ios_simulators.clear();
            return Ok(());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        self.ios_simulators.clear();
        if let Ok(json) = serde_json::from_str::<Value>(&stdout) {
            if let Some(devices) = json.get("devices").and_then(|d| d.as_object()) {
                for (runtime, list) in devices {
                    if let Some(arr) = list.as_array() {
                        for entry in arr {
                            let udid = entry.get("udid").and_then(|v| v.as_str());
                            let name = entry.get("name").and_then(|v| v.as_str());
                            let state = entry.get("state").and_then(|v| v.as_str());
                            if let (Some(udid), Some(name), Some(state)) = (udid, name, state) {
                                let status = match state {
                                    "Booted" => SimulatorStatus::Booted,
                                    "Booting" => SimulatorStatus::Booting,
                                    "Shutting Down" => SimulatorStatus::ShuttingDown,
                                    _ => SimulatorStatus::Shutdown,
                                };
                                let version = runtime
                                    .replace("com.apple.CoreSimulator.SimRuntime.", "")
                                    .replace('-', ".");
                                self.ios_simulators.push(SimulatorRecord {
                                    id: udid.to_string(),
                                    name: name.to_string(),
                                    platform: "ios".to_string(),
                                    version,
                                    status,
                                    device_type: "simulator".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
        info!("Found {} iOS simulator(s)", self.ios_simulators.len());
        Ok(())
    }

    fn emulator_path(&self) -> anyhow::Result<std::path::PathBuf> {
        if let Some(path) = &self.config.android.emulator_path {
            return Ok(path.clone());
        }
        if let Some(sdk) = &self.config.android.sdk_path {
            return Ok(sdk.join("emulator/emulator"));
        }
        Err(KMobileError::ConfigError("Emulator path not configured".to_string()).into())
    }

    fn is_android(&self, id: &str) -> bool {
        self.android_emulators.iter().any(|s| s.id == id)
    }

    fn is_ios(&self, id: &str) -> bool {
        self.ios_simulators.iter().any(|s| s.id == id)
    }

    async fn start_android(&self, avd: &str) -> anyhow::Result<()> {
        let emulator = self.emulator_path()?;
        let mut cmd = Command::new(emulator);
        cmd.args(["-avd", avd, "-no-audio", "-no-window"]);
        let child = cmd.spawn()?;
        debug!("Started Android emulator {avd} with pid {}", child.id());
        Ok(())
    }

    async fn start_ios(&self, id: &str) -> anyhow::Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "boot", id])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorStartError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn stop_android(&self, _avd: &str) -> anyhow::Result<()> {
        let adb = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;
        let output = Command::new(adb).args(["devices"]).output()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("emulator") && line.contains("device") {
                    if let Some(device_id) = line.split_whitespace().next() {
                        let _ = Command::new(adb)
                            .args(["-s", device_id, "emu", "kill"])
                            .output()?;
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    async fn stop_ios(&self, id: &str) -> anyhow::Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "shutdown", id])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorStopError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn reset_android(&self, avd: &str) -> anyhow::Result<()> {
        let emulator = self.emulator_path()?;
        let output = Command::new(emulator)
            .args(["-avd", avd, "-wipe-data", "-no-audio", "-no-window"])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorResetError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn reset_ios(&self, id: &str) -> anyhow::Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "erase", id])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorResetError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn install_android(&self, _avd: &str, app: &str) -> anyhow::Result<()> {
        let adb = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;
        let output = Command::new(adb).args(["devices"]).output()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("emulator") && line.contains("device") {
                    if let Some(device_id) = line.split_whitespace().next() {
                        let install = Command::new(adb)
                            .args(["-s", device_id, "install", "-r", app])
                            .output()?;
                        if !install.status.success() {
                            let stderr = String::from_utf8_lossy(&install.stderr);
                            return Err(KMobileError::AppInstallError(format!("{stderr}")).into());
                        }
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    async fn install_ios(&self, id: &str, app: &str) -> anyhow::Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "install", id, app])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::AppInstallError(format!("{stderr}")).into());
        }
        Ok(())
    }
}

#[async_trait]
impl SimulatorPort for CliSimulatorAdapter {
    async fn list_simulators(&self) -> anyhow::Result<Vec<SimulatorInfo>> {
        let mut all = Vec::with_capacity(self.android_emulators.len() + self.ios_simulators.len());
        for s in &self.android_emulators {
            all.push(SimulatorInfo {
                id: s.id.clone(),
                name: s.name.clone(),
                platform: s.platform.clone(),
            });
        }
        for s in &self.ios_simulators {
            all.push(SimulatorInfo {
                id: s.id.clone(),
                name: s.name.clone(),
                platform: s.platform.clone(),
            });
        }
        Ok(all)
    }

    async fn start_simulator(&self, id: &str) -> anyhow::Result<()> {
        info!("Starting simulator: {id}");
        if self.is_android(id) {
            self.start_android(id).await
        } else if self.is_ios(id) {
            self.start_ios(id).await
        } else {
            Err(KMobileError::SimulatorNotFound(id.to_string()).into())
        }
    }

    async fn stop_simulator(&self, id: &str) -> anyhow::Result<()> {
        info!("Stopping simulator: {id}");
        if self.is_android(id) {
            self.stop_android(id).await
        } else if self.is_ios(id) {
            self.stop_ios(id).await
        } else {
            Err(KMobileError::SimulatorNotFound(id.to_string()).into())
        }
    }

    async fn reset_simulator(&self, id: &str) -> anyhow::Result<()> {
        info!("Resetting simulator: {id}");
        if self.is_android(id) {
            self.reset_android(id).await
        } else if self.is_ios(id) {
            self.reset_ios(id).await
        } else {
            Err(KMobileError::SimulatorNotFound(id.to_string()).into())
        }
    }

    async fn install_app(&self, id: &str, app: &str) -> anyhow::Result<()> {
        info!("Installing app {app} on simulator {id}");
        if self.is_android(id) {
            self.install_android(id, app).await
        } else if self.is_ios(id) {
            self.install_ios(id, app).await
        } else {
            Err(KMobileError::SimulatorNotFound(id.to_string()).into())
        }
    }
}
