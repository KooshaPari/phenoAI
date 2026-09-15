//! KMobile MCP Server — Model Context Protocol entry point.
//!
//! Handles stdio JSON-RPC and delegates to the core ports.
//! Uses rmcp (Rust MCP SDK) for protocol compliance.

use anyhow::Result;
use clap::Parser;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::serve_server;
use rmcp::transport::stdio;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use kmobile::{Config, DeviceManager, ProjectManager, SimulatorManager, TestRunner};

#[derive(Parser)]
#[command(name = "kmobile-mcp")]
#[command(about = "KMobile MCP Server - Model Context Protocol server for mobile development")]
#[command(version, long_about = None)]
struct Args {
    #[arg(long, help = "Configuration file path")]
    config: Option<String>,
    #[arg(long, help = "Enable debug logging")]
    debug: bool,
}

/// Empty input for tools that require no parameters.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
struct EmptyInput {}

/// Device info returned by `list_devices` and `list_simulators`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
struct DeviceInfo {
    id: String,
    name: String,
    platform: String,
}

/// Output for `list_devices` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ListDevicesOutput {
    devices: Vec<DeviceInfo>,
}

/// Output for `list_simulators` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ListSimulatorsOutput {
    simulators: Vec<DeviceInfo>,
}

/// Output for `get_project_status` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ProjectStatusOutput {
    name: String,
    state: String,
}

/// Input for `run_tests` tool.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
struct RunTestsInput {
    suite: Option<String>,
    device: Option<String>,
}

/// Output for `run_tests` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
struct RunTestsOutput {
    suite: String,
    device: String,
    passed: usize,
    failed: usize,
}

/// KMobile MCP server handler.
/// Implements the ServerHandler trait from rmcp to expose tools for
/// device management, simulator control, and project operations.
#[derive(Debug, Clone)]
struct KMobileMcpServer {
    #[allow(dead_code)]
    config: Config,
    device_manager: DeviceManager,
    simulator_manager: SimulatorManager,
    project_manager: ProjectManager,
    test_runner: TestRunner,
}

impl KMobileMcpServer {
    async fn new(config: Config) -> Result<Self> {
        let device_manager = DeviceManager::new(&config).await?;
        let simulator_manager = SimulatorManager::new(&config).await?;
        let project_manager = ProjectManager::new(&config).await?;
        let test_runner = TestRunner::new(&config).await?;

        Ok(Self {
            config,
            device_manager,
            simulator_manager,
            project_manager,
            test_runner,
        })
    }

    fn project_state_from_status(status_json: &str) -> String {
        match serde_json::from_str::<Value>(status_json) {
            Ok(json) => json
                .get("build_status")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "ready".to_string()),
            Err(_) => "ready".to_string(),
        }
    }
}

#[tool_router(server_handler)]
impl KMobileMcpServer {
    /// List all connected devices (iOS/Android).
    #[tool(
        name = "list_devices",
        description = "List all connected iOS/Android devices"
    )]
    async fn list_devices(&self, Parameters(_): Parameters<EmptyInput>) -> Json<ListDevicesOutput> {
        let devices = match self.device_manager.list_devices().await {
            Ok(real_devices) => real_devices
                .into_iter()
                .map(|device| DeviceInfo {
                    id: device.id,
                    name: device.name,
                    platform: device.platform,
                })
                .collect(),
            Err(_) => vec![],
        };
        Json(ListDevicesOutput { devices })
    }

    /// List all available simulators.
    #[tool(
        name = "list_simulators",
        description = "List all available simulators"
    )]
    async fn list_simulators(
        &self,
        Parameters(_): Parameters<EmptyInput>,
    ) -> Json<ListSimulatorsOutput> {
        let simulators = match self.simulator_manager.list_simulators().await {
            Ok(real_simulators) => real_simulators
                .into_iter()
                .map(|simulator| DeviceInfo {
                    id: simulator.id,
                    name: simulator.name,
                    platform: simulator.platform,
                })
                .collect(),
            Err(_) => vec![],
        };
        Json(ListSimulatorsOutput { simulators })
    }

    /// Get the current project status.
    #[tool(
        name = "get_project_status",
        description = "Get the current project build status"
    )]
    async fn get_project_status(
        &self,
        Parameters(_): Parameters<EmptyInput>,
    ) -> Json<ProjectStatusOutput> {
        let project_name = self
            .config
            .projects
            .first()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "my-app".to_string());
        let status_state = self.project_manager.get_project_status().await.map_or_else(
            |_| "ready".to_string(),
            |status| Self::project_state_from_status(&status),
        );
        Json(ProjectStatusOutput {
            name: project_name,
            state: status_state,
        })
    }

    /// Run tests with optional suite and device.
    #[tool(
        name = "run_tests",
        description = "Execute test suites on target devices"
    )]
    async fn run_tests(
        &self,
        Parameters(RunTestsInput { suite, device }): Parameters<RunTestsInput>,
    ) -> Json<RunTestsOutput> {
        let suite = suite.unwrap_or_else(|| "default".to_string());
        let device = device.unwrap_or_else(|| "simulator".to_string());

        let suite_name = if suite == "default" {
            None
        } else {
            Some(suite.as_str())
        };
        let device_id = if device == "simulator" {
            None
        } else {
            Some(device.as_str())
        };

        let (passed, failed) = self
            .test_runner
            .run_tests_with_summary(suite_name, device_id)
            .await
            .map(|summary| (summary.passed as usize, summary.failed as usize))
            .unwrap_or_else(|_| (0, 1));

        Json(RunTestsOutput {
            suite,
            device,
            passed,
            failed,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = Config::load(args.config.as_deref())?;

    if args.debug {
        tracing_subscriber::fmt().with_env_filter("debug").init();
    }

    let server = KMobileMcpServer::new(config).await?;
    let transport = stdio();
    let running = serve_server(server, transport).await?;

    running.waiting().await?;
    Ok(())
}
