//! Concrete CLI adapter implementing [`kmobile_core::ports::TestingPort`].
//!
//! Loads a test suite from the configured output directory (or builds a
//! default suite if none exists), executes each test case against the
//! optional target device using `adb shell input …` / `xcodebuild test`,
//! then saves a JSON report and returns a [`kmobile_core::ports::TestResult`].

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use kmobile_core::config::Config;
use kmobile_core::error::KMobileError;
use kmobile_core::ports::{TestResult, TestingPort};

/// Atomic action a test step can perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestAction {
    Tap,
    Swipe,
    Type,
    Wait,
    Assert,
    Screenshot,
    Launch,
    Background,
    Foreground,
}

/// One interaction within a [`TestCase`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    pub action: TestAction,
    pub target: Option<String>,
    pub value: Option<String>,
    pub wait_time: Option<Duration>,
}

/// A single, named test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<TestStep>,
    pub expected_result: Option<String>,
    pub timeout: Option<Duration>,
}

/// Aggregate configuration for a suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    pub timeout: Duration,
    pub screenshot_on_failure: bool,
    pub video_recording: bool,
    pub parallel_execution: bool,
    pub retry_count: u32,
}

/// Group of [`TestCase`]s executed together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub name: String,
    pub tests: Vec<TestCase>,
    pub config: TestConfig,
}

/// Status of one test case execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Timeout,
}

/// Result of a single test case run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub test_name: String,
    pub status: TestStatus,
    pub duration: Duration,
    pub error_message: Option<String>,
    pub screenshots: Vec<String>,
}

/// Saved report containing every case result plus an aggregate summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub suite_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub results: Vec<CaseResult>,
    pub passed: usize,
    pub failed: usize,
}

/// CLI-side [`TestingPort`] implementation.
pub struct CliTestingAdapter {
    config: Config,
    test_output_dir: PathBuf,
    #[allow(dead_code)]
    current_suite: Option<TestSuite>,
}

impl CliTestingAdapter {
    /// Build the adapter. Ensures the test output directory exists.
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let test_output_dir = config.testing.output_dir.clone();
        fs::create_dir_all(&test_output_dir)?;
        Ok(Self {
            config,
            test_output_dir,
            current_suite: None,
        })
    }

    /// Borrow the suite that was last loaded.
    #[expect(dead_code)]
    pub fn current_suite(&self) -> Option<&TestSuite> {
        self.current_suite.as_ref()
    }

    /// The directory reports are written to.
    #[expect(dead_code)]
    pub fn output_dir(&self) -> &PathBuf {
        &self.test_output_dir
    }

    #[expect(dead_code)]
    async fn load_suite(&mut self, name: Option<&str>) -> anyhow::Result<TestSuite> {
        let suite_path = match name {
            Some(n) => self.test_output_dir.join(format!("{n}.json")),
            None => self.test_output_dir.join("default.json"),
        };

        let suite = if suite_path.exists() {
            let content = fs::read_to_string(&suite_path)?;
            serde_json::from_str(&content)?
        } else {
            // Synthesize a minimal default suite and persist it so subsequent
            // runs pick it up unchanged.
            let default_name = name.unwrap_or("default").to_string();
            let suite = TestSuite {
                name: default_name.clone(),
                tests: vec![TestCase {
                    name: "app_launch".to_string(),
                    description: Some("Test app launch".to_string()),
                    steps: vec![TestStep {
                        action: TestAction::Launch,
                        target: Some("com.example.app".to_string()),
                        value: None,
                        wait_time: Some(Duration::from_secs(5)),
                    }],
                    expected_result: Some("App launches successfully".to_string()),
                    timeout: Some(Duration::from_secs(30)),
                }],
                config: TestConfig {
                    timeout: Duration::from_secs(30),
                    screenshot_on_failure: true,
                    video_recording: false,
                    parallel_execution: false,
                    retry_count: 0,
                },
            };
            let content = serde_json::to_string_pretty(&suite)?;
            fs::write(&suite_path, content)?;
            suite
        };

        self.current_suite = Some(suite.clone());
        Ok(suite)
    }

    async fn run_case(&self, case: &TestCase, device: Option<&str>) -> anyhow::Result<CaseResult> {
        let start = Instant::now();
        let mut screenshots = Vec::new();

        for (idx, step) in case.steps.iter().enumerate() {
            if let Err(e) = self.execute_step(step, device, &mut screenshots).await {
                warn!("Step {} of {} failed: {e}", idx + 1, case.name);
                if self.config.testing.screenshot_on_failure {
                    let path = format!("{}_{}_failure.png", case.name, idx + 1);
                    if let Err(ss_err) = self.take_screenshot(device, &path).await {
                        warn!("Failure screenshot capture failed: {ss_err}");
                    } else {
                        screenshots.push(path);
                    }
                }
                return Ok(CaseResult {
                    test_name: case.name.clone(),
                    status: TestStatus::Failed,
                    duration: start.elapsed(),
                    error_message: Some(e.to_string()),
                    screenshots,
                });
            }
        }

        Ok(CaseResult {
            test_name: case.name.clone(),
            status: TestStatus::Passed,
            duration: start.elapsed(),
            error_message: None,
            screenshots,
        })
    }

    async fn execute_step(
        &self,
        step: &TestStep,
        device: Option<&str>,
        screenshots: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        debug!("Executing step: {:?}", step.action);
        match &step.action {
            TestAction::Tap => {
                if let (Some(target), Some(device)) = (step.target.as_deref(), device) {
                    self.adb_input(device, &["tap", target]).await?;
                }
            }
            TestAction::Swipe => {
                if let (Some(target), Some(device)) = (step.target.as_deref(), device) {
                    self.adb_input(device, &["swipe", target]).await?;
                }
            }
            TestAction::Type => {
                if let (Some(text), Some(device)) = (step.value.as_deref(), device) {
                    self.adb_input(device, &["text", text]).await?;
                }
            }
            TestAction::Wait => {
                if let Some(wait) = step.wait_time {
                    tokio::time::sleep(wait).await;
                }
            }
            TestAction::Assert => {
                if let (Some(target), Some(device)) = (step.target.as_deref(), device) {
                    let output = Command::new(self.adb_path()?)
                        .args(["-s", device, "shell", "dumpsys", "window", "windows"])
                        .output()?;
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        if !stdout.contains(target) {
                            return Err(KMobileError::TestExecutionError(format!(
                                "Element not found: {target}"
                            ))
                            .into());
                        }
                    }
                }
            }
            TestAction::Screenshot => {
                let default_name = format!("screenshot_{}.png", Utc::now().timestamp());
                let name = step.value.as_deref().unwrap_or(&default_name);
                self.take_screenshot(device, name).await?;
                screenshots.push(name.to_string());
            }
            TestAction::Launch => {
                if let (Some(app), Some(device)) = (step.target.as_deref(), device) {
                    let output = Command::new(self.adb_path()?)
                        .args(["-s", device, "shell", "am", "start", "-n", app])
                        .output()?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        return Err(KMobileError::TestExecutionError(format!("{stderr}")).into());
                    }
                }
            }
            TestAction::Background => {
                if let Some(device) = device {
                    self.adb_input(device, &["keyevent", "KEYCODE_HOME"])
                        .await?;
                }
            }
            TestAction::Foreground => {
                if let Some(device) = device {
                    self.adb_input(device, &["keyevent", "KEYCODE_APP_SWITCH"])
                        .await?;
                }
            }
        }

        if let Some(wait) = step.wait_time {
            tokio::time::sleep(wait).await;
        }
        Ok(())
    }

    fn adb_path(&self) -> anyhow::Result<&PathBuf> {
        self.config
            .android
            .adb_path
            .as_ref()
            .ok_or_else(|| KMobileError::ConfigError("ADB path not configured".to_string()).into())
    }

    async fn adb_input(&self, device: &str, args: &[&str]) -> anyhow::Result<()> {
        let adb = self.adb_path()?;
        let mut owned = vec!["-s", device, "shell", "input"];
        owned.extend_from_slice(args);
        let output = Command::new(adb).args(&owned).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KMobileError::TestExecutionError(format!("{stderr}")).into());
        }
        Ok(())
    }

    async fn take_screenshot(&self, device: Option<&str>, path: &str) -> anyhow::Result<()> {
        let full_path = self.test_output_dir.join(path);
        if let Some(device) = device {
            let adb = self.adb_path()?;
            let output = Command::new(adb)
                .args(["-s", device, "exec-out", "screencap", "-p"])
                .output()?;
            if output.status.success() {
                fs::write(&full_path, &output.stdout)?;
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(KMobileError::TestExecutionError(format!("{stderr}")).into());
            }
        }
        Ok(())
    }

    fn save_report(&self, report: &TestReport) -> anyhow::Result<()> {
        let path = self
            .test_output_dir
            .join(format!("{}_report.json", report.suite_name));
        let content = serde_json::to_string_pretty(report)?;
        fs::write(&path, content)?;
        Ok(())
    }

    fn summarise(cases: &[CaseResult]) -> (usize, usize) {
        let passed = cases
            .iter()
            .filter(|c| matches!(c.status, TestStatus::Passed))
            .count();
        let failed = cases
            .iter()
            .filter(|c| matches!(c.status, TestStatus::Failed))
            .count();
        (passed, failed)
    }
}

#[async_trait]
impl TestingPort for CliTestingAdapter {
    async fn run_tests(
        &self,
        suite: Option<&str>,
        device: Option<&str>,
    ) -> anyhow::Result<TestResult> {
        // The trait method takes `&self`, so we cannot cache the loaded
        // suite on the adapter. We re-load it here as a side-effect.
        let suite_name = suite.unwrap_or("default").to_string();
        let path = self.test_output_dir.join(format!("{suite_name}.json"));
        let suite: TestSuite = if path.exists() {
            let content = fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            TestSuite {
                name: suite_name.clone(),
                tests: Vec::new(),
                config: TestConfig {
                    timeout: Duration::from_secs(30),
                    screenshot_on_failure: self.config.testing.screenshot_on_failure,
                    video_recording: self.config.testing.video_recording,
                    parallel_execution: self.config.testing.parallel,
                    retry_count: 0,
                },
            }
        };

        info!("Running tests — suite: {suite_name}, device: {device:?}");
        let start = Utc::now();
        let mut results = Vec::with_capacity(suite.tests.len());
        for case in &suite.tests {
            results.push(self.run_case(case, device).await?);
        }
        let (passed, failed) = Self::summarise(&results);
        let report = TestReport {
            suite_name: suite_name.clone(),
            start_time: start,
            end_time: Some(Utc::now()),
            results,
            passed,
            failed,
        };
        self.save_report(&report)?;
        Ok(TestResult {
            suite: suite_name,
            passed,
            failed,
        })
    }

    async fn record_test(&self, output: &str) -> anyhow::Result<()> {
        info!("Recording test to {output}");
        // Recording needs an interactive device + recorder session; the
        // current adapter does not implement it but accepts the call so
        // the CLI does not error out.
        warn!("Test recording is not yet implemented in the CLI adapter");
        Ok(())
    }

    async fn replay_test(&self, file: &str) -> anyhow::Result<()> {
        info!("Replaying test from {file}");
        let path = PathBuf::from(file);
        if !path.exists() {
            return Err(KMobileError::TestFileNotFound(file.to_string()).into());
        }
        let content = fs::read_to_string(&path)?;
        let case: TestCase = serde_json::from_str(&content)?;
        let result = self.run_case(&case, None).await?;
        match result.status {
            TestStatus::Passed => println!("PASS: {}", case.name),
            TestStatus::Failed => println!(
                "FAIL: {}: {}",
                case.name,
                result.error_message.unwrap_or_default()
            ),
            _ => println!("SKIP: {}", case.name),
        }
        Ok(())
    }

    async fn run_device_tests(&self, id: &str, suite: Option<&str>) -> anyhow::Result<TestResult> {
        info!("Running on-device tests on {id}");
        self.run_tests(suite, Some(id)).await
    }
}
