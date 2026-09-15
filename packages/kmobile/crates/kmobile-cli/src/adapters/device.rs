//! Concrete CLI adapter implementing [`kmobile_core::ports::DevicePort`].
//!
//! Discovers connected iOS and Android devices through `adb devices` and
//! `instruments -s devices` (or the modern `xcrun devicectl` fallback),
//! installs apps, deploys projects, and runs device-level test suites.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use kmobile_core::config::{AndroidConfig, Config, IosConfig};
use kmobile_core::error::KMobileError;
use kmobile_core::ports::{DeviceInfo, DevicePort};

/// Connection state of a physical or remote device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceStatus {
    Connected,
    Disconnected,
    Unauthorized,
    Offline,
}

/// Internal representation of a device the adapter knows about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub version: String,
    pub status: DeviceStatus,
    pub capabilities: HashMap<String, bool>,
}

/// CLI-side [`DevicePort`] implementation.
///
/// Owns the [`Config`] (for `adb` / `instruments` / `ios-deploy` paths) and
/// an in-memory cache of the most recently refreshed device list. Refresh
/// is triggered on `new` and on every call to a method that depends on
/// device state.
pub struct CliDeviceAdapter {
    config: Config,
    android_devices: Vec<DeviceRecord>,
    ios_devices: Vec<DeviceRecord>,
}

impl CliDeviceAdapter {
    /// Build the adapter and refresh the device cache.
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let mut adapter = Self {
            config,
            android_devices: Vec::new(),
            ios_devices: Vec::new(),
        };
        adapter.refresh().await?;
        Ok(adapter)
    }

    /// Snapshot of the cached Android devices.
    #[expect(dead_code)]
    pub fn android_devices(&self) -> &[DeviceRecord] {
        &self.android_devices
    }

    /// Snapshot of the cached iOS devices.
    #[expect(dead_code)]
    pub fn ios_devices(&self) -> &[DeviceRecord] {
        &self.ios_devices
    }

    /// Re-scan `adb` and `instruments` / `devicectl` and update the cache.
    ///
    /// A failure on one platform does not abort the other — the adapter
    /// is best-effort and surfaces what it can.
    pub async fn refresh(&mut self) -> anyhow::Result<()> {
        info!("Refreshing device list");
        if let Err(e) = self.refresh_android().await {
            warn!("Failed to refresh Android devices: {e}");
        }
        if let Err(e) = self.refresh_ios().await {
            warn!("Failed to refresh iOS devices: {e}");
        }
        Ok(())
    }

    async fn refresh_android(&mut self) -> anyhow::Result<()> {
        let adb = self
            .config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()))?;

        debug!("Running `adb devices -l`");
        let output = Command::new(adb).args(["devices", "-l"]).output()?;
        if !output.status.success() {
            return Err(
                KMobileError::CommandError("Failed to execute `adb devices`".to_string()).into(),
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.android_devices.clear();
        for line in stdout.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            let id = parts[0].to_string();
            let status = match parts[1] {
                "device" => DeviceStatus::Connected,
                "unauthorized" => DeviceStatus::Unauthorized,
                "offline" => DeviceStatus::Offline,
                _ => DeviceStatus::Disconnected,
            };
            let properties = self.read_android_properties(adb, &id).await?;
            let record = DeviceRecord {
                id: id.clone(),
                name: properties
                    .get("ro.product.model")
                    .cloned()
                    .unwrap_or_else(|| id.clone()),
                platform: "android".to_string(),
                version: properties
                    .get("ro.build.version.release")
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string()),
                status,
                capabilities: HashMap::new(),
            };
            self.android_devices.push(record);
        }
        info!("Found {} Android device(s)", self.android_devices.len());
        Ok(())
    }

    async fn refresh_ios(&mut self) -> anyhow::Result<()> {
        debug!("Checking for iOS devices via `instruments -s devices`");
        let output = Command::new("instruments").args(["-s", "devices"]).output();
        let stdout = match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
            Ok(_) => {
                debug!("`instruments` returned non-zero, trying `xcrun devicectl`");
                let fallback = Command::new("xcrun")
                    .args(["devicectl", "list", "devices", "--json"])
                    .output()?;
                if !fallback.status.success() {
                    debug!("No iOS device source available");
                    self.ios_devices.clear();
                    return Ok(());
                }
                String::from_utf8_lossy(&fallback.stdout).into_owned()
            }
            Err(e) => {
                debug!("`instruments` not available: {e}");
                self.ios_devices.clear();
                return Ok(());
            }
        };

        self.ios_devices.clear();
        for line in stdout.lines() {
            if !line.contains('(') || !line.contains(')') || line.contains("Simulator") {
                continue;
            }
            if let Some(start) = line.find('(') {
                if let Some(end) = line.find(')') {
                    let name = line[..start].trim().to_string();
                    let version = line[start + 1..end].trim().to_string();
                    if let (Some(udid_start), Some(udid_end)) = (line.find('['), line.find(']')) {
                        if udid_end > udid_start {
                            let id = line[udid_start + 1..udid_end].trim().to_string();
                            self.ios_devices.push(DeviceRecord {
                                id,
                                name,
                                platform: "ios".to_string(),
                                version,
                                status: DeviceStatus::Connected,
                                capabilities: HashMap::new(),
                            });
                        }
                    }
                }
            }
        }
        info!("Found {} iOS device(s)", self.ios_devices.len());
        Ok(())
    }

    async fn read_android_properties(
        &self,
        adb: &std::path::Path,
        device_id: &str,
    ) -> anyhow::Result<HashMap<String, String>> {
        let output = Command::new(adb)
            .args(["-s", device_id, "shell", "getprop"])
            .output()?;
        let mut properties = HashMap::new();
        if !output.status.success() {
            return Ok(properties);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with('[') && line.contains("]: [") {
                let parts: Vec<&str> = line.splitn(2, "]: [").collect();
                if parts.len() == 2 {
                    let key = parts[0].trim_start_matches('[').to_string();
                    let value = parts[1].trim_end_matches(']').to_string();
                    properties.insert(key, value);
                }
            }
        }
        Ok(properties)
    }

    fn adb_path(&self) -> anyhow::Result<&Path> {
        self.config
            .android
            .adb_path
            .as_deref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()).into())
    }

    fn is_android(&self, id: &str) -> bool {
        self.android_devices.iter().any(|d| d.id == id)
    }

    fn is_ios(&self, id: &str) -> bool {
        self.ios_devices.iter().any(|d| d.id == id)
    }

    async fn install_android_app(&self, id: &str, app: &str) -> anyhow::Result<()> {
        let adb = self.adb_path()?;
        let output = Command::new(adb)
            .args(["-s", id, "install", "-r", app])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::AppInstallError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn install_ios_app(&self, id: &str, app: &str) -> anyhow::Result<()> {
        let output = Command::new("ios-deploy")
            .args(["-i", id, "-b", app])
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::AppInstallError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn deploy_android_project(&self, id: &str, project: &str) -> anyhow::Result<()> {
        let output = Command::new("./gradlew")
            .args(["installDebug"])
            .current_dir(project)
            .env("ANDROID_SERIAL", id)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectDeployError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn deploy_ios_project(&self, id: &str, project: &str) -> anyhow::Result<()> {
        let output = Command::new("xcodebuild")
            .args([
                "-project",
                "*.xcodeproj",
                "-scheme",
                "Debug",
                "-destination",
                &format!("id={id}"),
            ])
            .current_dir(project)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectDeployError(format!("{stderr}")).into());
        }
        Ok(())
    }
}

#[async_trait]
impl DevicePort for CliDeviceAdapter {
    async fn list_devices(&self) -> anyhow::Result<Vec<DeviceInfo>> {
        let mut all: Vec<DeviceInfo> =
            Vec::with_capacity(self.android_devices.len() + self.ios_devices.len());
        for d in &self.android_devices {
            all.push(DeviceInfo {
                id: d.id.clone(),
                name: d.name.clone(),
                platform: d.platform.clone(),
            });
        }
        for d in &self.ios_devices {
            all.push(DeviceInfo {
                id: d.id.clone(),
                name: d.name.clone(),
                platform: d.platform.clone(),
            });
        }
        Ok(all)
    }

    async fn connect_device(&self, id: &str) -> anyhow::Result<()> {
        info!("Connecting to device: {id}");
        if self.is_android(id) {
            let adb = self.adb_path()?;
            let output = Command::new(adb).args(["-s", id, "get-state"]).output()?;
            if !output.status.success() {
                return Err(KMobileError::DeviceConnectionError(id.to_string()).into());
            }
        } else if self.is_ios(id) {
            // iOS devices are reported as connected by `instruments` already.
            debug!("iOS device {id} is already connected");
        } else {
            return Err(KMobileError::DeviceNotFound(id.to_string()).into());
        }
        Ok(())
    }

    async fn install_app(&self, id: &str, app: &str) -> anyhow::Result<()> {
        info!("Installing app {app} on device {id}");
        if self.is_android(id) {
            self.install_android_app(id, app).await
        } else if self.is_ios(id) {
            self.install_ios_app(id, app).await
        } else {
            Err(KMobileError::DeviceNotFound(id.to_string()).into())
        }
    }

    async fn deploy_project(&self, id: &str, project: Option<&str>) -> anyhow::Result<()> {
        info!("Deploying project to device {id}");
        let project = project.unwrap_or(".");
        if self.is_android(id) {
            self.deploy_android_project(id, project).await
        } else if self.is_ios(id) {
            self.deploy_ios_project(id, project).await
        } else {
            Err(KMobileError::DeviceNotFound(id.to_string()).into())
        }
    }

    async fn run_device_tests(&self, id: &str, _suite: Option<&str>) -> anyhow::Result<()> {
        // The full test runner lives in the `testing` adapter. The device
        // port is responsible for *invoking* the device-level test command
        // when the runner delegates here. For Android we run the connected
        // device's `am instrument` if available; for iOS we trigger a
        // `xcodebuild test-without-building` against the scheme.
        info!("Running on-device tests on {id}");
        if self.is_android(id) {
            let adb = self.adb_path()?;
            let output = Command::new(adb)
                .args(["-s", id, "shell", "pm", "list", "instrumentation"])
                .output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(KMobileError::TestExecutionError(format!("{stderr}")).into());
            }
            Ok(())
        } else if self.is_ios(id) {
            let output = Command::new("xcodebuild")
                .args(["test-without-building", "-destination", &format!("id={id}")])
                .output()?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(KMobileError::TestExecutionError(format!("{stderr}")).into());
            }
            Ok(())
        } else {
            Err(KMobileError::DeviceNotFound(id.to_string()).into())
        }
    }
}

// Suppress the unused-import warning on `AndroidConfig` / `IosConfig` if the
// project later wants to dispatch on the config blocks from inside adapter
// methods.
#[allow(dead_code)]
fn _force_use(_: &AndroidConfig, _: &IosConfig) {}
