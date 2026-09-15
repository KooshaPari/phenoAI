use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info};

use crate::config::{Config, ProjectConfig};
use crate::error::KMobileError;

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// Build the project
    Build { target: Option<String> },
    /// Clean the project
    Clean,
    /// Get project status
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStatus {
    pub name: String,
    pub path: PathBuf,
    pub platform: String,
    pub build_status: BuildStatus,
    pub tests_status: TestStatus,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildStatus {
    Success,
    Failed,
    InProgress,
    NotBuilt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestStatus {
    Passed,
    Failed,
    Running,
    NotRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub status: DependencyStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyStatus {
    Installed,
    Missing,
    Outdated,
}

#[derive(Debug, Clone)]
pub struct ProjectManager {
    #[allow(dead_code)]
    config: Config,
    current_project: Option<ProjectConfig>,
}

impl ProjectManager {
    pub async fn new(config: &Config) -> Result<Self> {
        let current_project = Self::detect_current_project(config).await?;

        Ok(Self {
            config: config.clone(),
            current_project,
        })
    }

    async fn detect_current_project(config: &Config) -> Result<Option<ProjectConfig>> {
        let current_dir = std::env::current_dir()?;

        // Check if we're in a configured project
        for project in &config.projects {
            if current_dir.starts_with(&project.path) {
                return Ok(Some(project.clone()));
            }
        }

        // Try to detect project type from files
        if let Ok(project) = Self::detect_project_from_files(&current_dir).await {
            return Ok(Some(project));
        }

        Ok(None)
    }

    async fn detect_project_from_files(path: &std::path::Path) -> Result<ProjectConfig> {
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

        // Android project detection
        if path.join("build.gradle").exists() || path.join("build.gradle.kts").exists() {
            project.platform = "android".to_string();
            project.build_command = Some("./gradlew assembleDebug".to_string());
            project.test_command = Some("./gradlew test".to_string());
        }
        // iOS project detection
        else if path.join("ios").exists()
            || path.read_dir()?.any(|entry| {
                if let Ok(e) = entry {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "xcodeproj" || ext == "xcworkspace")
                } else {
                    false
                }
            })
        {
            project.platform = "ios".to_string();
            project.build_command = Some("xcodebuild -scheme Debug".to_string());
            project.test_command = Some("xcodebuild test -scheme Debug".to_string());
        }
        // React Native project detection
        else if path.join("package.json").exists()
            && path.join("android").exists()
            && path.join("ios").exists()
        {
            project.platform = "react-native".to_string();
            project.build_command = Some("npx react-native run-android".to_string());
            project.test_command = Some("npm test".to_string());
        }
        // Flutter project detection
        else if path.join("pubspec.yaml").exists() {
            project.platform = "flutter".to_string();
            project.build_command = Some("flutter build apk".to_string());
            project.test_command = Some("flutter test".to_string());
        }

        Ok(project)
    }

    pub async fn init_project(&self, name: &str, template: Option<&str>) -> Result<()> {
        info!(
            "Initializing project: {} with template: {:?}",
            name, template
        );

        let project_path = std::env::current_dir()?.join(name);
        fs::create_dir_all(&project_path)?;

        match template {
            Some("android") => self.init_android_project(&project_path, name).await?,
            Some("ios") => self.init_ios_project(&project_path, name).await?,
            Some("react-native") => self.init_react_native_project(&project_path, name).await?,
            Some("flutter") => self.init_flutter_project(&project_path, name).await?,
            _ => self.init_basic_project(&project_path, name).await?,
        }

        Ok(())
    }

    async fn init_android_project(&self, path: &PathBuf, name: &str) -> Result<()> {
        debug!("Initializing Android project at {:?}", path);

        // Create basic Android project structure
        let dirs = [
            "app/src/main/java",
            "app/src/main/res/layout",
            "app/src/main/res/values",
            "app/src/test/java",
            "app/src/androidTest/java",
        ];

        for dir in &dirs {
            fs::create_dir_all(path.join(dir))?;
        }

        // Create build.gradle
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
        
        testInstrumentationRunner "androidx.test.runner.AndroidJUnitRunner"
    }}
    
    buildTypes {{
        release {{
            minifyEnabled false
            proguardFiles getDefaultProguardFile('proguard-android-optimize.txt'), 'proguard-rules.pro'
        }}
    }}
}}

dependencies {{
    implementation 'androidx.appcompat:appcompat:1.6.1'
    implementation 'com.google.android.material:material:1.10.0'
    implementation 'androidx.constraintlayout:constraintlayout:2.1.4'
    testImplementation 'junit:junit:4.13.2'
    androidTestImplementation 'androidx.test.ext:junit:1.1.5'
    androidTestImplementation 'androidx.test.espresso:espresso-core:3.5.1'
}}
"#
        );

        fs::write(path.join("app/build.gradle"), build_gradle)?;

        // Create settings.gradle
        let settings_gradle = format!(
            r#"
rootProject.name = "{name}"
include ':app'
"#
        );

        fs::write(path.join("settings.gradle"), settings_gradle)?;

        // Create MainActivity
        let main_activity = format!(
            r#"
package com.kmobile.{name};

import androidx.appcompat.app.AppCompatActivity;
import android.os.Bundle;

public class MainActivity extends AppCompatActivity {{
    @Override
    protected void onCreate(Bundle savedInstanceState) {{
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_main);
    }}
}}
"#
        );

        fs::write(
            path.join("app/src/main/java/MainActivity.java"),
            main_activity,
        )?;

        Ok(())
    }

    async fn init_ios_project(&self, path: &PathBuf, name: &str) -> Result<()> {
        debug!("Initializing iOS project at {:?}", path);

        // Use xcodegen or create basic project structure
        let output = Command::new("xcodegen")
            .args(["generate"])
            .current_dir(path)
            .output();

        if output.is_err() {
            // Fallback to basic structure
            let dirs = [
                &format!("{name}/Sources"),
                &format!("{name}/Resources"),
                &format!("{name}Tests"),
            ];

            for dir in &dirs {
                fs::create_dir_all(path.join(dir))?;
            }

            // Create basic project.yml for xcodegen
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
    resources:
      - {name}/Resources
  {name}Tests:
    type: bundle.unit-test
    platform: iOS
    sources:
      - {name}Tests
    dependencies:
      - target: {name}
"#
            );

            fs::write(path.join("project.yml"), project_yml)?;
        }

        Ok(())
    }

    async fn init_react_native_project(&self, path: &PathBuf, name: &str) -> Result<()> {
        debug!("Initializing React Native project at {:?}", path);

        let output = Command::new("npx")
            .args(["react-native", "init", name])
            .current_dir(path.parent().unwrap())
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectInitError(format!(
                "Failed to initialize React Native project: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    async fn init_flutter_project(&self, path: &PathBuf, name: &str) -> Result<()> {
        debug!("Initializing Flutter project at {:?}", path);

        let output = Command::new("flutter")
            .args(["create", name])
            .current_dir(path.parent().unwrap())
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::ProjectInitError(format!(
                "Failed to initialize Flutter project: {error_msg}"
            ))
            .into());
        }

        Ok(())
    }

    async fn init_basic_project(&self, path: &PathBuf, name: &str) -> Result<()> {
        debug!("Initializing basic project at {:?}", path);

        // Create basic project structure
        let dirs = ["src", "tests", "docs"];

        for dir in &dirs {
            fs::create_dir_all(path.join(dir))?;
        }

        // Create kmobile.toml
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

        // Create README.md
        let readme = format!(
            r#"# {name}

A mobile project created with KMobile.

## Getting Started

1. Configure your build and test commands in `kmobile.toml`
2. Run `kmobile project build` to build the project
3. Run `kmobile test run` to run tests

## KMobile Commands

- `kmobile device list` - List connected devices
- `kmobile simulator list` - List available simulators
- `kmobile project build` - Build the project
- `kmobile test run` - Run tests
"#
        );

        fs::write(path.join("README.md"), readme)?;

        Ok(())
    }

    pub async fn build_project(&self, target: Option<&str>) -> Result<()> {
        info!("Building project with target: {:?}", target);

        let project = self.current_project.as_ref().ok_or_else(|| {
            KMobileError::ProjectNotFound("No project found in current directory".to_string())
        })?;

        let build_command = project
            .build_command
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("No build command configured".to_string()))?;

        let mut cmd_parts = build_command.split_whitespace();
        let command = cmd_parts.next().unwrap();
        let args: Vec<&str> = cmd_parts.collect();

        let output = Command::new(command)
            .args(&args)
            .current_dir(&project.path)
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::BuildError(format!("Build failed: {error_msg}")).into());
        }

        info!("Project built successfully");
        Ok(())
    }

    pub async fn clean_project(&self) -> Result<()> {
        info!("Cleaning project");

        let project = self.current_project.as_ref().ok_or_else(|| {
            KMobileError::ProjectNotFound("No project found in current directory".to_string())
        })?;

        match project.platform.as_str() {
            "android" => {
                let output = Command::new("./gradlew")
                    .args(["clean"])
                    .current_dir(&project.path)
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(
                        KMobileError::BuildError(format!("Clean failed: {error_msg}")).into(),
                    );
                }
            }
            "ios" => {
                let output = Command::new("xcodebuild")
                    .args(["clean"])
                    .current_dir(&project.path)
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(
                        KMobileError::BuildError(format!("Clean failed: {error_msg}")).into(),
                    );
                }
            }
            "react-native" => {
                // Clean React Native project
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
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(
                        KMobileError::BuildError(format!("Clean failed: {error_msg}")).into(),
                    );
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

    pub async fn get_project_status(&self) -> Result<String> {
        let project = self.current_project.as_ref().ok_or_else(|| {
            KMobileError::ProjectNotFound("No project found in current directory".to_string())
        })?;

        let status = ProjectStatus {
            name: project.name.clone(),
            path: project.path.clone(),
            platform: project.platform.clone(),
            build_status: BuildStatus::NotBuilt,
            tests_status: TestStatus::NotRun,
            dependencies: Vec::new(),
        };

        let status_json = serde_json::to_string_pretty(&status)?;
        Ok(status_json)
    }
}
