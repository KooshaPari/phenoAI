use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::error::KMobileError;

#[derive(Subcommand)]
pub enum TestCommands {
    /// Run tests
    Run {
        suite: Option<String>,
        device: Option<String>,
    },
    /// Record a test
    Record { output: String },
    /// Replay a test
    Replay { file: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    pub name: String,
    pub tests: Vec<TestCase>,
    pub config: TestConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<TestStep>,
    pub expected_result: Option<String>,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStep {
    pub action: TestAction,
    pub target: Option<String>,
    pub value: Option<String>,
    pub wait_time: Option<Duration>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    pub timeout: Duration,
    pub screenshot_on_failure: bool,
    pub video_recording: bool,
    pub parallel_execution: bool,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_name: String,
    pub status: TestStatus,
    pub duration: Duration,
    pub error_message: Option<String>,
    pub screenshots: Vec<String>,
    pub video_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReport {
    pub suite_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub results: Vec<TestResult>,
    pub summary: TestSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub timeout: u32,
}

#[derive(Debug, Clone)]
pub struct TestRunner {
    config: Config,
    #[allow(dead_code)]
    current_suite: Option<TestSuite>,
    test_output_dir: PathBuf,
}

impl TestRunner {
    pub async fn new(config: &Config) -> Result<Self> {
        let test_output_dir = config.testing.output_dir.clone();
        fs::create_dir_all(&test_output_dir)?;

        Ok(Self {
            config: config.clone(),
            current_suite: None,
            test_output_dir,
        })
    }

    pub async fn run_tests(&self, suite_name: Option<&str>, device_id: Option<&str>) -> Result<()> {
        self.run_tests_with_summary(suite_name, device_id)
            .await
            .map(|_| ())
    }

    pub async fn run_tests_with_summary(
        &self,
        suite_name: Option<&str>,
        device_id: Option<&str>,
    ) -> Result<TestSummary> {
        info!(
            "Running tests - Suite: {:?}, Device: {:?}",
            suite_name, device_id
        );

        let suite = self.load_test_suite(suite_name).await?;
        let start_time = Utc::now();

        let mut results = Vec::new();

        for test_case in &suite.tests {
            info!("Running test: {}", test_case.name);

            let result = self.run_test_case(test_case, device_id).await?;
            results.push(result);
        }

        let report = TestReport {
            suite_name: suite.name.clone(),
            start_time,
            end_time: Some(Utc::now()),
            results: results.clone(),
            summary: self.generate_summary(&results),
        };

        self.save_test_report(&report).await?;
        self.print_test_summary(&report);

        Ok(report.summary)
    }

    async fn load_test_suite(&self, suite_name: Option<&str>) -> Result<TestSuite> {
        let suite_path = match suite_name {
            Some(name) => self.test_output_dir.join(format!("{name}.json")),
            None => self.test_output_dir.join("default.json"),
        };

        if suite_path.exists() {
            let content = fs::read_to_string(&suite_path)?;
            let suite: TestSuite = serde_json::from_str(&content)?;
            Ok(suite)
        } else {
            // Create a default test suite
            let default_suite = TestSuite {
                name: suite_name.unwrap_or("default").to_string(),
                tests: vec![TestCase {
                    name: "app_launch".to_string(),
                    description: Some("Test app launch".to_string()),
                    steps: vec![
                        TestStep {
                            action: TestAction::Launch,
                            target: Some("com.example.app".to_string()),
                            value: None,
                            wait_time: Some(Duration::from_secs(5)),
                        },
                        TestStep {
                            action: TestAction::Screenshot,
                            target: None,
                            value: Some("launch_screen.png".to_string()),
                            wait_time: None,
                        },
                    ],
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

            // Save the default suite
            let content = serde_json::to_string_pretty(&default_suite)?;
            fs::write(&suite_path, content)?;

            Ok(default_suite)
        }
    }

    async fn run_test_case(
        &self,
        test_case: &TestCase,
        device_id: Option<&str>,
    ) -> Result<TestResult> {
        let start_time = std::time::Instant::now();
        let mut screenshots = Vec::new();

        debug!("Executing test case: {}", test_case.name);

        for (i, step) in test_case.steps.iter().enumerate() {
            match self
                .execute_test_step(step, device_id, &mut screenshots)
                .await
            {
                Ok(_) => debug!("Step {} completed successfully", i + 1),
                Err(e) => {
                    warn!("Step {} failed: {}", i + 1, e);

                    if self.config.testing.screenshot_on_failure {
                        let screenshot_path = format!("{}_{}_failure.png", test_case.name, i + 1);
                        if let Err(screenshot_err) =
                            self.take_screenshot(device_id, &screenshot_path).await
                        {
                            warn!("Failed to take failure screenshot: {}", screenshot_err);
                        } else {
                            screenshots.push(screenshot_path);
                        }
                    }

                    return Ok(TestResult {
                        test_name: test_case.name.clone(),
                        status: TestStatus::Failed,
                        duration: start_time.elapsed(),
                        error_message: Some(e.to_string()),
                        screenshots,
                        video_path: None,
                    });
                }
            }
        }

        Ok(TestResult {
            test_name: test_case.name.clone(),
            status: TestStatus::Passed,
            duration: start_time.elapsed(),
            error_message: None,
            screenshots,
            video_path: None,
        })
    }

    async fn execute_test_step(
        &self,
        step: &TestStep,
        device_id: Option<&str>,
        screenshots: &mut Vec<String>,
    ) -> Result<()> {
        debug!("Executing step: {:?}", step.action);

        match &step.action {
            TestAction::Tap => {
                if let Some(target) = &step.target {
                    self.tap_element(device_id, target).await?;
                }
            }
            TestAction::Swipe => {
                if let Some(target) = &step.target {
                    self.swipe_element(device_id, target).await?;
                }
            }
            TestAction::Type => {
                if let (Some(target), Some(value)) = (&step.target, &step.value) {
                    self.type_text(device_id, target, value).await?;
                }
            }
            TestAction::Wait => {
                if let Some(wait_time) = step.wait_time {
                    tokio::time::sleep(wait_time).await;
                }
            }
            TestAction::Assert => {
                if let Some(target) = &step.target {
                    self.assert_element_exists(device_id, target).await?;
                }
            }
            TestAction::Screenshot => {
                let default_screenshot =
                    format!("screenshot_{}.png", chrono::Utc::now().timestamp());
                let screenshot_path = step.value.as_ref().unwrap_or(&default_screenshot);
                self.take_screenshot(device_id, screenshot_path).await?;
                screenshots.push(screenshot_path.clone());
            }
            TestAction::Launch => {
                if let Some(app_id) = &step.target {
                    self.launch_app(device_id, app_id).await?;
                }
            }
            TestAction::Background => {
                self.background_app(device_id).await?;
            }
            TestAction::Foreground => {
                self.foreground_app(device_id).await?;
            }
        }

        if let Some(wait_time) = step.wait_time {
            tokio::time::sleep(wait_time).await;
        }

        Ok(())
    }

    async fn tap_element(&self, device_id: Option<&str>, target: &str) -> Result<()> {
        debug!("Tapping element: {}", target);

        if let Some(device_id) = device_id {
            // Use ADB for Android devices
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args(["-s", device_id, "shell", "input", "tap", target])
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::TestExecutionError(format!(
                        "Tap failed: {error_msg}"
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    async fn swipe_element(&self, device_id: Option<&str>, target: &str) -> Result<()> {
        debug!("Swiping element: {}", target);

        if let Some(device_id) = device_id {
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args(["-s", device_id, "shell", "input", "swipe", target])
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::TestExecutionError(format!(
                        "Swipe failed: {error_msg}"
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    async fn type_text(&self, device_id: Option<&str>, target: &str, text: &str) -> Result<()> {
        debug!("Typing text: {} in {}", text, target);

        if let Some(device_id) = device_id {
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args(["-s", device_id, "shell", "input", "text", text])
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::TestExecutionError(format!(
                        "Type failed: {error_msg}"
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    async fn assert_element_exists(&self, device_id: Option<&str>, target: &str) -> Result<()> {
        debug!("Asserting element exists: {}", target);

        if let Some(device_id) = device_id {
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args(["-s", device_id, "shell", "dumpsys", "window", "windows"])
                    .output()?;

                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    if !output_str.contains(target) {
                        return Err(KMobileError::TestExecutionError(format!(
                            "Element not found: {target}"
                        ))
                        .into());
                    }
                }
            }
        }

        Ok(())
    }

    async fn take_screenshot(&self, device_id: Option<&str>, path: &str) -> Result<()> {
        debug!("Taking screenshot: {}", path);

        let full_path = self.test_output_dir.join(path);

        if let Some(device_id) = device_id {
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args(["-s", device_id, "exec-out", "screencap", "-p"])
                    .output()?;

                if output.status.success() {
                    fs::write(&full_path, &output.stdout)?;
                } else {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::TestExecutionError(format!(
                        "Screenshot failed: {error_msg}"
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    async fn launch_app(&self, device_id: Option<&str>, app_id: &str) -> Result<()> {
        debug!("Launching app: {}", app_id);

        if let Some(device_id) = device_id {
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args(["-s", device_id, "shell", "am", "start", "-n", app_id])
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::TestExecutionError(format!(
                        "App launch failed: {error_msg}"
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    async fn background_app(&self, device_id: Option<&str>) -> Result<()> {
        debug!("Backgrounding app");

        if let Some(device_id) = device_id {
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args([
                        "-s",
                        device_id,
                        "shell",
                        "input",
                        "keyevent",
                        "KEYCODE_HOME",
                    ])
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::TestExecutionError(format!(
                        "Background failed: {error_msg}"
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    async fn foreground_app(&self, device_id: Option<&str>) -> Result<()> {
        debug!("Foregrounding app");

        if let Some(device_id) = device_id {
            if let Some(adb_path) = &self.config.android.adb_path {
                let output = Command::new(adb_path)
                    .args([
                        "-s",
                        device_id,
                        "shell",
                        "input",
                        "keyevent",
                        "KEYCODE_APP_SWITCH",
                    ])
                    .output()?;

                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr);
                    return Err(KMobileError::TestExecutionError(format!(
                        "Foreground failed: {error_msg}"
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    fn generate_summary(&self, results: &[TestResult]) -> TestSummary {
        let mut summary = TestSummary {
            total: results.len() as u32,
            passed: 0,
            failed: 0,
            skipped: 0,
            timeout: 0,
        };

        for result in results {
            match result.status {
                TestStatus::Passed => summary.passed += 1,
                TestStatus::Failed => summary.failed += 1,
                TestStatus::Skipped => summary.skipped += 1,
                TestStatus::Timeout => summary.timeout += 1,
            }
        }

        summary
    }

    async fn save_test_report(&self, report: &TestReport) -> Result<()> {
        let report_path = self
            .test_output_dir
            .join(format!("{}_report.json", report.suite_name));
        let content = serde_json::to_string_pretty(report)?;
        fs::write(&report_path, content)?;
        Ok(())
    }

    fn print_test_summary(&self, report: &TestReport) {
        println!("{} Test Summary for '{}':", crate::utils::sym("[status]", "[status]"),  report.suite_name);
        println!("   Total: {}", report.summary.total);
        println!("   {} Passed: {}", crate::utils::sym("[ok]", "[ok]"),  report.summary.passed);
        println!("   {} Failed: {}", crate::utils::sym("[err]", "[err]"),  report.summary.failed);
        println!("   [skip]  Skipped: {}", report.summary.skipped);
        println!("   [timer]  Timeout: {}", report.summary.timeout);

        if report.summary.failed > 0 {
            println!("\n{} Failed tests:", crate::utils::sym("[err]", "[err]"));
            for result in &report.results {
                if matches!(result.status, TestStatus::Failed) {
                    println!(
                        "   - {}: {}",
                        result.test_name,
                        result.error_message.as_deref().unwrap_or("Unknown error")
                    );
                }
            }
        }
    }

    pub async fn run_device_tests(&self, device_id: &str, suite_name: Option<&str>) -> Result<()> {
        info!("Running device tests on: {}", device_id);
        self.run_tests(suite_name, Some(device_id)).await
    }

    pub async fn record_test(&self, output_path: &str) -> Result<()> {
        info!("Recording test to: {}", output_path);

        // TODO: Implement test recording functionality
        // This would involve capturing user interactions and generating test cases
        warn!("Test recording not yet implemented");

        Ok(())
    }

    pub async fn replay_test(&self, test_file: &str) -> Result<()> {
        info!("Replaying test from: {}", test_file);

        let test_path = PathBuf::from(test_file);
        if !test_path.exists() {
            return Err(KMobileError::TestFileNotFound(test_file.to_string()).into());
        }

        let content = fs::read_to_string(&test_path)?;
        let test_case: TestCase = serde_json::from_str(&content)?;

        let result = self.run_test_case(&test_case, None).await?;

        match result.status {
            TestStatus::Passed => println!("{} Test '{}' passed", crate::utils::sym("[ok]", "[ok]"),  test_case.name),
            TestStatus::Failed => println!(
                "[err] Test '{}' failed: {}",
                test_case.name,
                result.error_message.unwrap_or_default()
            ),
            _ => println!("[skip] Test '{}' skipped", test_case.name),
        }

        Ok(())
    }
}
