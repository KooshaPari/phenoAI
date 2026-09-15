use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::KMobileError;

#[derive(Subcommand)]
pub enum DeviceCommands {
    /// List all connected devices
    List,
    /// Connect to a specific device
    Connect { id: String },
    /// Install app on device
    Install { id: String, app: String },
    /// Deploy project to device
    Deploy { id: String, project: Option<String> },
    /// Run tests on device
    Test { id: String, suite: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
    pub status: DeviceStatus,
    pub capabilities: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceStatus {
    Connected,
    Disconnected,
    Unauthorized,
    Offline,
}

#[derive(Debug, Clone)]
pub struct DeviceManager {
    config: Config,
    android_devices: Vec<Device>,
    ios_devices: Vec<Device>,
}

impl DeviceManager {
    pub async fn new(config: &Config) -> Result<Self> {
        let mut manager = Self {
            config: config.clone(),
            android_devices: Vec::new(),
            ios_devices: Vec::new(),
        };

        manager.refresh_devices().await?;
        Ok(manager)
    }

    pub async fn refresh_devices(&mut self) -> Result<()> {
        info!("Refreshing device list");

        // Refresh Android devices
        if let Err(e) = self.refresh_android_devices().await {
            warn!("Failed to refresh Android devices: {}", e);
        }

        // Refresh iOS devices
        if let Err(e) = self.refresh_ios_devices().await {
            warn!("Failed to refresh iOS devices: {}", e);
        }

        Ok(())
    }

    async fn refresh_android_devices(&mut self) -> Result<()> {
        let adb_path = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        debug!("Running adb devices");
        let output = Command::new(adb_path).args(["devices", "-l"]).output()?;

        if !output.status.success() {
            return Err(
                KMobileError::CommandError("Failed to execute adb devices".to_string()).into(),
            );
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.android_devices.clear();

        for line in output_str.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let device_id = parts[0].to_string();
                let status = match parts[1] {
                    "device" => DeviceStatus::Connected,
                    "unauthorized" => DeviceStatus::Unauthorized,
                    "offline" => DeviceStatus::Offline,
                    _ => DeviceStatus::Disconnected,
                };

                // Get device properties
                let properties = self.get_android_device_properties(&device_id).await?;

                let device = Device {
                    id: device_id.clone(),
                    name: properties
                        .get("ro.product.model")
                        .unwrap_or(&device_id)
                        .clone(),
                    platform: "android".to_string(),
                    version: properties
                        .get("ro.build.version.release")
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string()),
                    status,
                    capabilities: HashMap::new(),
                };

                self.android_devices.push(device);
            }
        }

        info!("Found {} Android devices", self.android_devices.len());
        Ok(())
    }

    async fn refresh_ios_devices(&mut self) -> Result<()> {
        // For iOS, we need to check for connected devices via instruments
        debug!("Checking for iOS devices");

        let output = Command::new("instruments")
            .args(["-s", "devices"])
            .output()?;

        if !output.status.success() {
            debug!("instruments command failed, iOS devices may not be available");
            return Ok(());
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        self.ios_devices.clear();

        for line in output_str.lines() {
            if line.contains("(") && line.contains(")") && !line.contains("Simulator") {
                // Parse device line: "iPhone 12 Pro (14.5) [UDID]"
                if let Some(start) = line.find('(') {
                    if let Some(end) = line.find(')') {
                        let name = line[..start].trim();
                        let version = line[start + 1..end].trim();

                        if let Some(udid_start) = line.find('[') {
                            if let Some(udid_end) = line.find(']') {
                                let udid = line[udid_start + 1..udid_end].trim();

                                let device = Device {
                                    id: udid.to_string(),
                                    name: name.to_string(),
                                    platform: "ios".to_string(),
                                    version: version.to_string(),
                                    status: DeviceStatus::Connected,
                                    capabilities: HashMap::new(),
                                };

                                self.ios_devices.push(device);
                            }
                        }
                    }
                }
            }
        }

        info!("Found {} iOS devices", self.ios_devices.len());
        Ok(())
    }

    async fn get_android_device_properties(
        &self,
        device_id: &str,
    ) -> Result<HashMap<String, String>> {
        let adb_path = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        let output = Command::new(adb_path)
            .args(["-s", device_id, "shell", "getprop"])
            .output()?;

        let mut properties = HashMap::new();

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.starts_with('[') && line.contains("]: [") {
                    let parts: Vec<&str> = line.splitn(2, "]: [").collect();
                    if parts.len() == 2 {
                        let key = parts[0].trim_start_matches('[').to_string();
                        let value = parts[1].trim_end_matches(']').to_string();
                        properties.insert(key, value);
                    }
                }
            }
        }

        Ok(properties)
    }

    pub async fn list_devices(&self) -> Result<Vec<Device>> {
        let mut devices = Vec::new();
        devices.extend(self.android_devices.clone());
        devices.extend(self.ios_devices.clone());
        Ok(devices)
    }

    pub async fn connect_device(&self, device_id: &str) -> Result<()> {
        info!("Connecting to device: {}", device_id);

        // Check if it's an Android device
        if self.android_devices.iter().any(|d| d.id == device_id) {
            self.connect_android_device(device_id).await?;
        } else if self.ios_devices.iter().any(|d| d.id == device_id) {
            self.connect_ios_device(device_id).await?;
        } else {
            return Err(KMobileError::DeviceNotFound(device_id.to_string()).into());
        }

        Ok(())
    }

    async fn connect_android_device(&self, device_id: &str) -> Result<()> {
        let adb_path = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        let output = Command::new(adb_path)
            .args(["-s", device_id, "get-state"])
            .output()?;

        if !output.status.success() {
            return Err(KMobileError::DeviceConnectionError(device_id.to_string()).into());
        }

        Ok(())
    }

    async fn connect_ios_device(&self, device_id: &str) -> Result<()> {
        // iOS devices are typically already connected when detected
        debug!("iOS device {} is already connected", device_id);
        Ok(())
    }

    pub async fn install_app(&self, device_id: &str, app_path: &str) -> Result<()> {
        info!("Installing app {} on device {}", app_path, device_id);

        if self.android_devices.iter().any(|d| d.id == device_id) {
            self.install_android_app(device_id, app_path).await?;
        } else if self.ios_devices.iter().any(|d| d.id == device_id) {
            self.install_ios_app(device_id, app_path).await?;
        } else {
            return Err(KMobileError::DeviceNotFound(device_id.to_string()).into());
        }

        Ok(())
    }

    async fn install_android_app(&self, device_id: &str, app_path: &str) -> Result<()> {
        let adb_path = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        let output = Command::new(adb_path)
            .args(["-s", device_id, "install", "-r", app_path])
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::AppInstallError(format!(
                "Failed to install app: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    async fn install_ios_app(&self, device_id: &str, app_path: &str) -> Result<()> {
        // Use ios-deploy or similar tool for iOS app installation
        let output = Command::new("ios-deploy")
            .args(["-i", device_id, "-b", app_path])
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

    pub async fn deploy_project(&self, device_id: &str, project_path: Option<&str>) -> Result<()> {
        info!("Deploying project to device {}", device_id);

        let project_path = project_path.unwrap_or(".");

        if self.android_devices.iter().any(|d| d.id == device_id) {
            self.deploy_android_project(device_id, project_path).await?;
        } else if self.ios_devices.iter().any(|d| d.id == device_id) {
            self.deploy_ios_project(device_id, project_path).await?;
        } else {
            return Err(KMobileError::DeviceNotFound(device_id.to_string()).into());
        }

        Ok(())
    }

    async fn deploy_android_project(&self, device_id: &str, project_path: &str) -> Result<()> {
        // Build and deploy Android project
        let output = Command::new("./gradlew")
            .args(["installDebug"])
            .current_dir(project_path)
            .env("ANDROID_SERIAL", device_id)
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectDeployError(format!(
                "Failed to deploy Android project: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    async fn deploy_ios_project(&self, device_id: &str, project_path: &str) -> Result<()> {
        // Build and deploy iOS project using xcodebuild
        let output = Command::new("xcodebuild")
            .args([
                "-project",
                "*.xcodeproj",
                "-scheme",
                "Debug",
                "-destination",
                &format!("id={device_id}"),
            ])
            .current_dir(project_path)
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectDeployError(format!(
                "Failed to deploy iOS project: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }
}
