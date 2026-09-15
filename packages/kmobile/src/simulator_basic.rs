use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::KMobileError;

#[derive(Subcommand)]
pub enum SimulatorCommands {
    /// List all available simulators
    List,
    /// Start a simulator
    Start { id: String },
    /// Stop a simulator
    Stop { id: String },
    /// Reset a simulator
    Reset { id: String },
    /// Install app on simulator
    Install { id: String, app: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Simulator {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
    pub status: SimulatorStatus,
    pub device_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimulatorStatus {
    Booted,
    Shutdown,
    Booting,
    ShuttingDown,
}

#[derive(Debug, Clone)]
pub struct SimulatorManager {
    config: Config,
    android_emulators: Vec<Simulator>,
    ios_simulators: Vec<Simulator>,
}

impl SimulatorManager {
    pub async fn new(config: &Config) -> Result<Self> {
        let mut manager = Self {
            config: config.clone(),
            android_emulators: Vec::new(),
            ios_simulators: Vec::new(),
        };

        manager.refresh_simulators().await?;
        Ok(manager)
    }

    pub async fn refresh_simulators(&mut self) -> Result<()> {
        info!("Refreshing simulator list");

        // Refresh Android emulators
        if let Err(e) = self.refresh_android_emulators().await {
            warn!("Failed to refresh Android emulators: {}", e);
        }

        // Refresh iOS simulators
        if let Err(e) = self.refresh_ios_simulators().await {
            warn!("Failed to refresh iOS simulators: {}", e);
        }

        Ok(())
    }

    async fn refresh_android_emulators(&mut self) -> Result<()> {
        let emulator_path = if let Some(path) = &self.config.android.emulator_path {
            path
        } else if let Some(sdk_path) = &self.config.android.sdk_path {
            &sdk_path.join("emulator/emulator")
        } else {
            return Err(
                KMobileError::ConfigError("Emulator path not configured".to_string()).into(),
            );
        };

        debug!("Running emulator -list-avds");
        let output = Command::new(emulator_path).args(["-list-avds"]).output()?;

        if !output.status.success() {
            return Err(
                KMobileError::CommandError("Failed to list Android emulators".to_string()).into(),
            );
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.android_emulators.clear();

        for line in output_str.lines() {
            if !line.trim().is_empty() {
                let avd_name = line.trim();

                // Get emulator status
                let status = self.get_android_emulator_status(avd_name).await?;

                let simulator = Simulator {
                    id: avd_name.to_string(),
                    name: avd_name.to_string(),
                    platform: "android".to_string(),
                    version: "unknown".to_string(),
                    status,
                    device_type: "emulator".to_string(),
                };

                self.android_emulators.push(simulator);
            }
        }

        info!("Found {} Android emulators", self.android_emulators.len());
        Ok(())
    }

    async fn refresh_ios_simulators(&mut self) -> Result<()> {
        let _simctl_path = self
            .config
            .ios
            .simctl_path
            .as_ref()
            .map(|p| p.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| "xcrun simctl".to_string());

        debug!("Running simctl list devices");
        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", "--json"])
            .output()?;

        if !output.status.success() {
            debug!("simctl command failed, iOS simulators may not be available");
            return Ok(());
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.ios_simulators.clear();

        // Parse JSON output
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output_str) {
            if let Some(devices) = json.get("devices").and_then(|d| d.as_object()) {
                for (runtime, device_list) in devices {
                    if let Some(devices) = device_list.as_array() {
                        for device in devices {
                            if let (Some(udid), Some(name), Some(state)) = (
                                device.get("udid").and_then(|v| v.as_str()),
                                device.get("name").and_then(|v| v.as_str()),
                                device.get("state").and_then(|v| v.as_str()),
                            ) {
                                let status = match state {
                                    "Booted" => SimulatorStatus::Booted,
                                    "Booting" => SimulatorStatus::Booting,
                                    "Shutting Down" => SimulatorStatus::ShuttingDown,
                                    _ => SimulatorStatus::Shutdown,
                                };

                                let simulator = Simulator {
                                    id: udid.to_string(),
                                    name: name.to_string(),
                                    platform: "ios".to_string(),
                                    version: runtime
                                        .replace("com.apple.CoreSimulator.SimRuntime.", "")
                                        .replace("-", "."),
                                    status,
                                    device_type: "simulator".to_string(),
                                };

                                self.ios_simulators.push(simulator);
                            }
                        }
                    }
                }
            }
        }

        info!("Found {} iOS simulators", self.ios_simulators.len());
        Ok(())
    }

    async fn get_android_emulator_status(&self, avd_name: &str) -> Result<SimulatorStatus> {
        let adb_path = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        let output = Command::new(adb_path).args(["devices"]).output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains(avd_name) && line.contains("device") {
                    return Ok(SimulatorStatus::Booted);
                }
            }
        }

        Ok(SimulatorStatus::Shutdown)
    }

    pub async fn list_simulators(&self) -> Result<Vec<Simulator>> {
        let mut simulators = Vec::new();
        simulators.extend(self.android_emulators.clone());
        simulators.extend(self.ios_simulators.clone());
        Ok(simulators)
    }

    pub async fn start_simulator(&self, simulator_id: &str) -> Result<()> {
        info!("Starting simulator: {}", simulator_id);

        if self.android_emulators.iter().any(|s| s.id == simulator_id) {
            self.start_android_emulator(simulator_id).await?;
        } else if self.ios_simulators.iter().any(|s| s.id == simulator_id) {
            self.start_ios_simulator(simulator_id).await?;
        } else {
            return Err(KMobileError::SimulatorNotFound(simulator_id.to_string()).into());
        }

        Ok(())
    }

    async fn start_android_emulator(&self, avd_name: &str) -> Result<()> {
        let emulator_path = if let Some(path) = &self.config.android.emulator_path {
            path
        } else if let Some(sdk_path) = &self.config.android.sdk_path {
            &sdk_path.join("emulator/emulator")
        } else {
            return Err(
                KMobileError::ConfigError("Emulator path not configured".to_string()).into(),
            );
        };

        let mut cmd = Command::new(emulator_path);
        cmd.args(["-avd", avd_name, "-no-audio", "-no-window"]);

        let child = cmd.spawn()?;
        debug!(
            "Started Android emulator {} with PID {}",
            avd_name,
            child.id()
        );

        Ok(())
    }

    async fn start_ios_simulator(&self, simulator_id: &str) -> Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "boot", simulator_id])
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorStartError(format!(
                "Failed to start iOS simulator: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    pub async fn stop_simulator(&self, simulator_id: &str) -> Result<()> {
        info!("Stopping simulator: {}", simulator_id);

        if self.android_emulators.iter().any(|s| s.id == simulator_id) {
            self.stop_android_emulator(simulator_id).await?;
        } else if self.ios_simulators.iter().any(|s| s.id == simulator_id) {
            self.stop_ios_simulator(simulator_id).await?;
        } else {
            return Err(KMobileError::SimulatorNotFound(simulator_id.to_string()).into());
        }

        Ok(())
    }

    async fn stop_android_emulator(&self, _avd_name: &str) -> Result<()> {
        let adb_path = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        // Find the emulator device ID
        let output = Command::new(adb_path).args(["devices"]).output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("emulator") && line.contains("device") {
                    let device_id = line.split_whitespace().next().unwrap();

                    let _ = Command::new(adb_path)
                        .args(["-s", device_id, "emu", "kill"])
                        .output()?;

                    break;
                }
            }
        }

        Ok(())
    }

    async fn stop_ios_simulator(&self, simulator_id: &str) -> Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "shutdown", simulator_id])
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorStopError(format!(
                "Failed to stop iOS simulator: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    pub async fn reset_simulator(&self, simulator_id: &str) -> Result<()> {
        info!("Resetting simulator: {}", simulator_id);

        if self.android_emulators.iter().any(|s| s.id == simulator_id) {
            self.reset_android_emulator(simulator_id).await?;
        } else if self.ios_simulators.iter().any(|s| s.id == simulator_id) {
            self.reset_ios_simulator(simulator_id).await?;
        } else {
            return Err(KMobileError::SimulatorNotFound(simulator_id.to_string()).into());
        }

        Ok(())
    }

    async fn reset_android_emulator(&self, avd_name: &str) -> Result<()> {
        let emulator_path = if let Some(path) = &self.config.android.emulator_path {
            path
        } else if let Some(sdk_path) = &self.config.android.sdk_path {
            &sdk_path.join("emulator/emulator")
        } else {
            return Err(
                KMobileError::ConfigError("Emulator path not configured".to_string()).into(),
            );
        };

        let output = Command::new(emulator_path)
            .args(["-avd", avd_name, "-wipe-data", "-no-audio", "-no-window"])
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorResetError(format!(
                "Failed to reset Android emulator: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    async fn reset_ios_simulator(&self, simulator_id: &str) -> Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "erase", simulator_id])
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::SimulatorResetError(format!(
                "Failed to reset iOS simulator: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    pub async fn install_app(&self, simulator_id: &str, app_path: &str) -> Result<()> {
        info!("Installing app {} on simulator {}", app_path, simulator_id);

        if self.android_emulators.iter().any(|s| s.id == simulator_id) {
            self.install_android_app(simulator_id, app_path).await?;
        } else if self.ios_simulators.iter().any(|s| s.id == simulator_id) {
            self.install_ios_app(simulator_id, app_path).await?;
        } else {
            return Err(KMobileError::SimulatorNotFound(simulator_id.to_string()).into());
        }

        Ok(())
    }

    async fn install_android_app(&self, _avd_name: &str, app_path: &str) -> Result<()> {
        let adb_path = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        // Find the emulator device ID
        let output = Command::new(adb_path).args(["devices"]).output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("emulator") && line.contains("device") {
                    let device_id = line.split_whitespace().next().unwrap();

                    let install_output = Command::new(adb_path)
                        .args(["-s", device_id, "install", "-r", app_path])
                        .output()?;

                    if !install_output.status.success() {
                        let error_msg = String::from_utf8_lossy(&install_output.stderr);
                        return Err(KMobileError::AppInstallError(format!(
                            "Failed to install app: {error_msg}"
                        ))
                        .into());
                    }

                    break;
                }
            }
        }

        Ok(())
    }

    async fn install_ios_app(&self, simulator_id: &str, app_path: &str) -> Result<()> {
        let output = Command::new("xcrun")
            .args(["simctl", "install", simulator_id, app_path])
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::AppInstallError(format!(
                "Failed to install iOS app: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }
}
