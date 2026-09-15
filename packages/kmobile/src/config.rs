use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    pub version: String,
    pub android: AndroidConfig,
    pub ios: IosConfig,
    pub testing: TestingConfig,
    pub mcp: McpConfig,
    pub api: ApiConfig,
    pub projects: Vec<ProjectConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AndroidConfig {
    pub sdk_path: Option<PathBuf>,
    pub adb_path: Option<PathBuf>,
    pub emulator_path: Option<PathBuf>,
    pub default_emulator: Option<String>,
    pub build_tools_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IosConfig {
    pub xcode_path: Option<PathBuf>,
    pub simctl_path: Option<PathBuf>,
    pub default_simulator: Option<String>,
    pub developer_team: Option<String>,
    pub provisioning_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestingConfig {
    pub framework: String,
    pub timeout: u64,
    pub parallel: bool,
    pub screenshot_on_failure: bool,
    pub video_recording: bool,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
    pub host: String,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub enabled: bool,
    pub port: u16,
    pub host: String,
    pub auth: Option<AuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub method: String,
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub path: PathBuf,
    pub platform: String,
    pub build_command: Option<String>,
    pub test_command: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "kmobile".to_string(),
            version: "0.1.0".to_string(),
            android: AndroidConfig::default(),
            ios: IosConfig::default(),
            testing: TestingConfig::default(),
            mcp: McpConfig::default(),
            api: ApiConfig::default(),
            projects: Vec::new(),
        }
    }
}

impl Default for TestingConfig {
    fn default() -> Self {
        Self {
            framework: "kmobile".to_string(),
            timeout: 30,
            parallel: true,
            screenshot_on_failure: true,
            video_recording: false,
            output_dir: PathBuf::from("./test-results"),
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 3001,
            host: "localhost".to_string(),
            tools: vec![
                "device_list".to_string(),
                "simulator_control".to_string(),
                "app_deploy".to_string(),
                "test_run".to_string(),
            ],
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 3000,
            host: "localhost".to_string(),
            auth: None,
        }
    }
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = path.unwrap_or("kmobile.toml");

        if std::path::Path::new(config_path).exists() {
            let content = std::fs::read_to_string(config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let default_config = Config::default();
            default_config.save(config_path)?;
            Ok(default_config)
        }
    }

    pub fn save(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
    pub fn detect_android_sdk(&mut self) -> Result<()> {
        // Try to detect Android SDK path
        if let Ok(sdk_path) = std::env::var("ANDROID_SDK_ROOT") {
            self.android.sdk_path = Some(PathBuf::from(sdk_path));
        } else if let Ok(sdk_path) = std::env::var("ANDROID_HOME") {
            self.android.sdk_path = Some(PathBuf::from(sdk_path));
        }

        // Try to detect ADB path
        if let Some(sdk_path) = &self.android.sdk_path {
            let adb_path = sdk_path.join("platform-tools").join("adb");
            if adb_path.exists() {
                self.android.adb_path = Some(adb_path);
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn detect_ios_tools(&mut self) -> Result<()> {
        // Try to detect Xcode path
        if let Ok(output) = std::process::Command::new("xcode-select")
            .arg("-p")
            .output()
        {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                let path = path_str.trim();
                self.ios.xcode_path = Some(PathBuf::from(path));
            }
        }

        // Try to detect simctl path
        if let Ok(output) = std::process::Command::new("which").arg("simctl").output() {
            if output.status.success() {
                let path_str = String::from_utf8_lossy(&output.stdout);
                let path = path_str.trim();
                self.ios.simctl_path = Some(PathBuf::from(path));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_name_returns_expected_value() {
        let config = Config {
            name: "my-test-project".to_string(),
            version: "1.0.0".to_string(),
            android: AndroidConfig::default(),
            ios: IosConfig::default(),
            testing: TestingConfig::default(),
            mcp: McpConfig::default(),
            api: ApiConfig::default(),
            projects: Vec::new(),
        };
        assert_eq!(config.name(), "my-test-project");
    }

    #[test]
    fn config_default_name_is_kmobile() {
        let config = Config::default();
        assert_eq!(config.name(), "kmobile");
    }
}
