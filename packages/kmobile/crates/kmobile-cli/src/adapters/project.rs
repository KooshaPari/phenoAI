//! Concrete CLI adapter implementing [`kmobile_core::ports::ProjectPort`].
//!
//! Detects the project in the current working directory (Android, iOS,
//! React Native, Flutter, or generic), then drives `gradle` /
//! `xcodebuild` / `flutter` / `npx` to build, clean, and report status.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use kmobile_core::config::{Config, ProjectConfig};
use kmobile_core::error::KMobileError;
use kmobile_core::ports::{ProjectPort, ProjectStatus};

/// State of the most recent build attempt.
#[expect(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BuildStatus {
    Success,
    Failed,
    InProgress,
    NotBuilt,
}

/// State of the most recent test run.
#[expect(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    Failed,
    Running,
    NotRun,
}

/// A project dependency entry reported in `get_project_status`.
#[expect(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub status: DependencyStatus,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DependencyStatus {
    Installed,
    Missing,
    Outdated,
}

/// CLI-side [`ProjectPort`] implementation.
pub struct CliProjectAdapter {
    #[expect(dead_code)]
    config: Config,
    current_project: Option<ProjectConfig>,
}

impl CliProjectAdapter {
    /// Build the adapter and detect the project in the current directory.
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let current_project = Self::detect_current_project(&config).await?;
        Ok(Self {
            config,
            current_project,
        })
    }

    /// Replace the detected project (used by tests and by `init_project`).
    #[expect(dead_code)]
    pub fn set_current_project(&mut self, project: Option<ProjectConfig>) {
        self.current_project = project;
    }

    /// Borrow the currently-detected project.
    #[expect(dead_code)]
    pub fn current_project(&self) -> Option<&ProjectConfig> {
        self.current_project.as_ref()
    }

    async fn detect_current_project(config: &Config) -> anyhow::Result<Option<ProjectConfig>> {
        let current_dir = std::env::current_dir()?;
        for project in &config.projects {
            if current_dir.starts_with(&project.path) {
                return Ok(Some(project.clone()));
            }
        }
        if let Ok(project) = Self::detect_from_files(&current_dir).await {
            return Ok(Some(project));
        }
        Ok(None)
    }

    async fn detect_from_files(path: &Path) -> anyhow::Result<ProjectConfig> {
        let mut project = ProjectConfig {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: path.to_path_buf(),
            platform: "unknown".to_string(),
            build_command: None,
            test_command: None,
            metadata: HashMap::new(),
        };

        if path.join("build.gradle").exists() || path.join("build.gradle.kts").exists() {
            project.platform = "android".to_string();
            project.build_command = Some("./gradlew assembleDebug".to_string());
            project.test_command = Some("./gradlew test".to_string());
        } else if path.join("ios").exists()
            || path.read_dir()?.filter_map(Result::ok).any(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "xcodeproj" || ext == "xcworkspace")
            })
        {
            project.platform = "ios".to_string();
            project.build_command = Some("xcodebuild -scheme Debug".to_string());
            project.test_command = Some("xcodebuild test -scheme Debug".to_string());
        } else if path.join("package.json").exists()
            && path.join("android").exists()
            && path.join("ios").exists()
        {
            project.platform = "react-native".to_string();
            project.build_command = Some("npx react-native run-android".to_string());
            project.test_command = Some("npm test".to_string());
        } else if path.join("pubspec.yaml").exists() {
            project.platform = "flutter".to_string();
            project.build_command = Some("flutter build apk".to_string());
            project.test_command = Some("flutter test".to_string());
        }

        Ok(project)
    }

    async fn init_android(&self, path: &PathBuf, name: &str) -> anyhow::Result<()> {
        debug!("Initializing Android project at {path:?}");
        for dir in [
            "app/src/main/java",
            "app/src/main/res/layout",
            "app/src/main/res/values",
            "app/src/test/java",
            "app/src/androidTest/java",
        ] {
            fs::create_dir_all(path.join(dir))?;
        }
        let build_gradle = format!(
            r#"
plugins {{
    id 'com.android.application'
}}

android {{
    compileSdk 34
    defaultConfig {{
        applicationId "com.kmobile.{name}"
        minSdk 21
        targetSdk 34
        versionCode 1
        versionName "1.0"
    }}
    buildTypes {{
        release {{
            minifyEnabled false
        }}
    }}
}}

dependencies {{
    implementation 'androidx.appcompat:appcompat:1.6.1'
    testImplementation 'junit:junit:4.13.2'
}}
"#
        );
        fs::write(path.join("app/build.gradle"), build_gradle)?;
        let settings = format!("rootProject.name = \"{name}\"\ninclude ':app'\n");
        fs::write(path.join("settings.gradle"), settings)?;
        Ok(())
    }

    async fn init_ios(&self, path: &PathBuf, name: &str) -> anyhow::Result<()> {
        debug!("Initializing iOS project at {path:?}");
        let output = Command::new("xcodegen")
            .args(["generate"])
            .current_dir(path)
            .output();
        if output.is_err() {
            for dir in [
                format!("{name}/Sources"),
                format!("{name}/Resources"),
                format!("{name}Tests"),
            ] {
                fs::create_dir_all(path.join(dir))?;
            }
            let project_yml = format!(
                r#"
name: {name}
options:
  bundleIdPrefix: com.kmobile
targets:
  {name}:
    type: application
    platform: iOS
    deploymentTarget: "14.0"
    sources:
      - {name}/Sources
"#
            );
            fs::write(path.join("project.yml"), project_yml)?;
        }
        Ok(())
    }

    async fn init_react_native(&self, path: &PathBuf, name: &str) -> anyhow::Result<()> {
        debug!("Initializing React Native project at {path:?}");
        let parent = path
            .parent()
            .ok_or_else(|| KMobileError::ProjectInitError("path has no parent".into()))?;
        let output = Command::new("npx")
            .args(["react-native", "init", name])
            .current_dir(parent)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectInitError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn init_flutter(&self, path: &PathBuf, name: &str) -> anyhow::Result<()> {
        debug!("Initializing Flutter project at {path:?}");
        let parent = path
            .parent()
            .ok_or_else(|| KMobileError::ProjectInitError("path has no parent".into()))?;
        let output = Command::new("flutter")
            .args(["create", name])
            .current_dir(parent)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectInitError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn init_basic(&self, path: &PathBuf, name: &str) -> anyhow::Result<()> {
        debug!("Initializing basic project at {path:?}");
        for dir in ["src", "tests", "docs"] {
            fs::create_dir_all(path.join(dir))?;
        }
        let kmobile_toml = format!(
            r#"
[project]
name = "{name}"
version = "0.1.0"
platform = "multi"

[build]
command = "echo 'Build command not configured'"

[test]
command = "echo 'Test command not configured'"
"#
        );
        fs::write(path.join("kmobile.toml"), kmobile_toml)?;
        Ok(())
    }

    async fn run_build_command(&self, project: &ProjectConfig) -> anyhow::Result<()> {
        let build_command = project
            .build_command
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("No build command configured".to_string()))?;
        let mut parts = build_command.split_whitespace();
        let command = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();
        if command.is_empty() {
            return Err(KMobileError::ConfigError("Build command is empty".to_string()).into());
        }
        let output = Command::new(command)
            .args(&args)
            .current_dir(&project.path)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::BuildError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn clean_for_platform(&self, project: &ProjectConfig) -> anyhow::Result<()> {
        match project.platform.as_str() {
            "android" => {
                let output = Command::new("./gradlew")
                    .args(["clean"])
                    .current_dir(&project.path)
                    .output()?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::BuildError(format!("{stderr}")).into());
                }
            }
            "ios" => {
                let output = Command::new("xcodebuild")
                    .args(["clean"])
                    .current_dir(&project.path)
                    .output()?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::BuildError(format!("{stderr}")).into());
                }
            }
            "react-native" => {
                let _ = fs::remove_dir_all(project.path.join("node_modules"));
                let _ = fs::remove_dir_all(project.path.join("android/build"));
                let _ = fs::remove_dir_all(project.path.join("ios/build"));
            }
            "flutter" => {
                let output = Command::new("flutter")
                    .args(["clean"])
                    .current_dir(&project.path)
                    .output()?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::BuildError(format!("{stderr}")).into());
                }
            }
            _ => {
                info!(
                    "Clean command not implemented for platform: {}",
                    project.platform
                );
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ProjectPort for CliProjectAdapter {
    async fn init_project(&self, name: &str, template: Option<&str>) -> anyhow::Result<()> {
        info!("Initializing project: {name} (template: {template:?})");
        let project_path = std::env::current_dir()?.join(name);
        fs::create_dir_all(&project_path)?;
        match template {
            Some("android") => self.init_android(&project_path, name).await?,
            Some("ios") => self.init_ios(&project_path, name).await?,
            Some("react-native") => self.init_react_native(&project_path, name).await?,
            Some("flutter") => self.init_flutter(&project_path, name).await?,
            _ => self.init_basic(&project_path, name).await?,
        }
        Ok(())
    }

    async fn build_project(&self, _target: Option<&str>) -> anyhow::Result<()> {
        info!("Building project");
        let project = self.current_project.as_ref().ok_or_else(|| {
            KMobileError::ProjectNotFound("No project in current directory".into())
        })?;
        self.run_build_command(project).await
    }

    async fn clean_project(&self) -> anyhow::Result<()> {
        info!("Cleaning project");
        let project = self.current_project.as_ref().ok_or_else(|| {
            KMobileError::ProjectNotFound("No project in current directory".into())
        })?;
        self.clean_for_platform(project).await
    }

    async fn get_project_status(&self) -> anyhow::Result<ProjectStatus> {
        let project = self.current_project.as_ref().ok_or_else(|| {
            KMobileError::ProjectNotFound("No project in current directory".into())
        })?;
        Ok(ProjectStatus {
            name: project.name.clone(),
            state: format!("platform={}", project.platform),
        })
    }
}
