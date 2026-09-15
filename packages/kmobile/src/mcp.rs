use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::device_basic::DeviceManager;
use crate::error::KMobileError;
use crate::project::ProjectManager;
use crate::simulator_basic::SimulatorManager;
use crate::testing::TestRunner;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    pub description: String,
    pub arguments: Vec<McpPromptArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub result: Option<serde_json::Value>,
    pub error: Option<McpError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

pub struct McpServer {
    config: Config,
    #[allow(dead_code)]
    device_manager: Arc<RwLock<DeviceManager>>,
    #[allow(dead_code)]
    simulator_manager: Arc<RwLock<SimulatorManager>>,
    #[allow(dead_code)]
    project_manager: Arc<RwLock<ProjectManager>>,
    #[allow(dead_code)]
    test_runner: Arc<RwLock<TestRunner>>,
    tools: HashMap<String, McpTool>,
    resources: HashMap<String, McpResource>,
    prompts: HashMap<String, McpPrompt>,
}

#[allow(dead_code)]
impl McpServer {
    pub async fn new(config: &Config, _config_path: Option<&str>) -> Result<Self> {
        let device_manager = Arc::new(RwLock::new(DeviceManager::new(config).await?));
        let simulator_manager = Arc::new(RwLock::new(SimulatorManager::new(config).await?));
        let project_manager = Arc::new(RwLock::new(ProjectManager::new(config).await?));
        let test_runner = Arc::new(RwLock::new(TestRunner::new(config).await?));

        let mut server = Self {
            config: config.clone(),
            device_manager,
            simulator_manager,
            project_manager,
            test_runner,
            tools: HashMap::new(),
            resources: HashMap::new(),
            prompts: HashMap::new(),
        };

        server.register_tools().await?;
        server.register_resources().await?;
        server.register_prompts().await?;

        Ok(server)
    }

    async fn register_tools(&mut self) -> Result<()> {
        info!("Registering MCP tools");

        // Device management tools
        self.tools.insert(
            "device_list".to_string(),
            McpTool {
                name: "device_list".to_string(),
                description: "List all connected devices".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        );

        self.tools.insert(
            "device_connect".to_string(),
            McpTool {
                name: "device_connect".to_string(),
                description: "Connect to a specific device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device_id": {
                            "type": "string",
                            "description": "Device ID to connect to"
                        }
                    },
                    "required": ["device_id"]
                }),
            },
        );

        self.tools.insert(
            "device_install".to_string(),
            McpTool {
                name: "device_install".to_string(),
                description: "Install app on device".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device_id": {
                            "type": "string",
                            "description": "Device ID"
                        },
                        "app_path": {
                            "type": "string",
                            "description": "Path to app file"
                        }
                    },
                    "required": ["device_id", "app_path"]
                }),
            },
        );

        // Simulator management tools
        self.tools.insert(
            "simulator_list".to_string(),
            McpTool {
                name: "simulator_list".to_string(),
                description: "List all available simulators".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        );

        self.tools.insert(
            "simulator_start".to_string(),
            McpTool {
                name: "simulator_start".to_string(),
                description: "Start a simulator".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "simulator_id": {
                            "type": "string",
                            "description": "Simulator ID to start"
                        }
                    },
                    "required": ["simulator_id"]
                }),
            },
        );

        self.tools.insert(
            "simulator_stop".to_string(),
            McpTool {
                name: "simulator_stop".to_string(),
                description: "Stop a simulator".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "simulator_id": {
                            "type": "string",
                            "description": "Simulator ID to stop"
                        }
                    },
                    "required": ["simulator_id"]
                }),
            },
        );

        // Project management tools
        self.tools.insert(
            "project_build".to_string(),
            McpTool {
                name: "project_build".to_string(),
                description: "Build the current project".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target": {
                            "type": "string",
                            "description": "Build target (optional)"
                        }
                    }
                }),
            },
        );

        self.tools.insert(
            "project_status".to_string(),
            McpTool {
                name: "project_status".to_string(),
                description: "Get project status".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        );

        // Testing tools
        self.tools.insert(
            "test_run".to_string(),
            McpTool {
                name: "test_run".to_string(),
                description: "Run tests".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "suite": {
                            "type": "string",
                            "description": "Test suite name (optional)"
                        },
                        "device_id": {
                            "type": "string",
                            "description": "Device ID to run tests on (optional)"
                        }
                    }
                }),
            },
        );

        self.tools.insert(
            "test_record".to_string(),
            McpTool {
                name: "test_record".to_string(),
                description: "Record a test".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "output": {
                            "type": "string",
                            "description": "Output file path"
                        }
                    },
                    "required": ["output"]
                }),
            },
        );

        info!("Registered {} MCP tools", self.tools.len());
        Ok(())
    }

    async fn register_resources(&mut self) -> Result<()> {
        info!("Registering MCP resources");

        self.resources.insert(
            "devices".to_string(),
            McpResource {
                uri: "kmobile://devices".to_string(),
                name: "Connected Devices".to_string(),
                description: "List of currently connected devices".to_string(),
                mime_type: "application/json".to_string(),
            },
        );

        self.resources.insert(
            "simulators".to_string(),
            McpResource {
                uri: "kmobile://simulators".to_string(),
                name: "Available Simulators".to_string(),
                description: "List of available simulators".to_string(),
                mime_type: "application/json".to_string(),
            },
        );

        self.resources.insert(
            "project".to_string(),
            McpResource {
                uri: "kmobile://project".to_string(),
                name: "Current Project".to_string(),
                description: "Current project information".to_string(),
                mime_type: "application/json".to_string(),
            },
        );

        info!("Registered {} MCP resources", self.resources.len());
        Ok(())
    }

    async fn register_prompts(&mut self) -> Result<()> {
        info!("Registering MCP prompts");

        self.prompts.insert(
            "mobile_deploy".to_string(),
            McpPrompt {
                name: "mobile_deploy".to_string(),
                description: "Deploy mobile application to device or simulator".to_string(),
                arguments: vec![
                    McpPromptArgument {
                        name: "platform".to_string(),
                        description: "Target platform (android/ios)".to_string(),
                        required: true,
                    },
                    McpPromptArgument {
                        name: "target".to_string(),
                        description: "Deployment target (device/simulator)".to_string(),
                        required: true,
                    },
                    McpPromptArgument {
                        name: "app_path".to_string(),
                        description: "Path to application file".to_string(),
                        required: false,
                    },
                ],
            },
        );

        self.prompts.insert(
            "mobile_test".to_string(),
            McpPrompt {
                name: "mobile_test".to_string(),
                description: "Run mobile application tests".to_string(),
                arguments: vec![
                    McpPromptArgument {
                        name: "test_type".to_string(),
                        description: "Type of test (unit/integration/e2e)".to_string(),
                        required: true,
                    },
                    McpPromptArgument {
                        name: "device_id".to_string(),
                        description: "Device ID to run tests on".to_string(),
                        required: false,
                    },
                ],
            },
        );

        info!("Registered {} MCP prompts", self.prompts.len());
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        info!(
            "Starting MCP server on {}:{}",
            self.config.mcp.host, self.config.mcp.port
        );

        // TODO: Implement actual MCP server using stdio transport
        // For now, we'll simulate the server running
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            debug!("MCP server heartbeat");
        }
    }

    pub async fn handle_request(&self, request: McpRequest) -> Result<McpResponse> {
        debug!("Handling MCP request: {}", request.method);

        match request.method.as_str() {
            "tools/list" => self.handle_tools_list().await,
            "tools/call" => self.handle_tool_call(request.params).await,
            "resources/list" => self.handle_resources_list().await,
            "resources/read" => self.handle_resource_read(request.params).await,
            "prompts/list" => self.handle_prompts_list().await,
            "prompts/get" => self.handle_prompt_get(request.params).await,
            _ => Ok(McpResponse {
                result: None,
                error: Some(McpError {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: None,
                }),
            }),
        }
    }

    async fn handle_tools_list(&self) -> Result<McpResponse> {
        let tools: Vec<&McpTool> = self.tools.values().collect();

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "tools": tools
            })),
            error: None,
        })
    }

    async fn handle_tool_call(&self, params: serde_json::Value) -> Result<McpResponse> {
        let tool_name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Tool name not provided".to_string()))?;

        let default_args = serde_json::json!({});
        let arguments = params.get("arguments").unwrap_or(&default_args);

        match tool_name {
            "device_list" => self.handle_device_list().await,
            "device_connect" => self.handle_device_connect(arguments).await,
            "device_install" => self.handle_device_install(arguments).await,
            "simulator_list" => self.handle_simulator_list().await,
            "simulator_start" => self.handle_simulator_start(arguments).await,
            "simulator_stop" => self.handle_simulator_stop(arguments).await,
            "project_build" => self.handle_project_build(arguments).await,
            "project_status" => self.handle_project_status().await,
            "test_run" => self.handle_test_run(arguments).await,
            "test_record" => self.handle_test_record(arguments).await,
            _ => Ok(McpResponse {
                result: None,
                error: Some(McpError {
                    code: -32602,
                    message: "Unknown tool".to_string(),
                    data: None,
                }),
            }),
        }
    }

    async fn handle_device_list(&self) -> Result<McpResponse> {
        let device_manager = self.device_manager.read().await;
        let devices = device_manager
            .list_devices()
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "devices": devices
            })),
            error: None,
        })
    }

    async fn handle_device_connect(&self, arguments: &serde_json::Value) -> Result<McpResponse> {
        let device_id = arguments
            .get("device_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Device ID not provided".to_string()))?;

        let device_manager = self.device_manager.read().await;
        device_manager
            .connect_device(device_id)
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "success": true,
                "message": format!("Connected to device: {}", device_id)
            })),
            error: None,
        })
    }

    async fn handle_device_install(&self, arguments: &serde_json::Value) -> Result<McpResponse> {
        let device_id = arguments
            .get("device_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Device ID not provided".to_string()))?;

        let app_path = arguments
            .get("app_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("App path not provided".to_string()))?;

        let device_manager = self.device_manager.read().await;
        device_manager
            .install_app(device_id, app_path)
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "success": true,
                "message": format!("Installed app on device: {}", device_id)
            })),
            error: None,
        })
    }

    async fn handle_simulator_list(&self) -> Result<McpResponse> {
        let simulator_manager = self.simulator_manager.read().await;
        let simulators = simulator_manager
            .list_simulators()
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "simulators": simulators
            })),
            error: None,
        })
    }

    async fn handle_simulator_start(&self, arguments: &serde_json::Value) -> Result<McpResponse> {
        let simulator_id = arguments
            .get("simulator_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Simulator ID not provided".to_string()))?;

        let simulator_manager = self.simulator_manager.read().await;
        simulator_manager
            .start_simulator(simulator_id)
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "success": true,
                "message": format!("Started simulator: {}", simulator_id)
            })),
            error: None,
        })
    }

    async fn handle_simulator_stop(&self, arguments: &serde_json::Value) -> Result<McpResponse> {
        let simulator_id = arguments
            .get("simulator_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Simulator ID not provided".to_string()))?;

        let simulator_manager = self.simulator_manager.read().await;
        simulator_manager
            .stop_simulator(simulator_id)
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "success": true,
                "message": format!("Stopped simulator: {}", simulator_id)
            })),
            error: None,
        })
    }

    async fn handle_project_build(&self, arguments: &serde_json::Value) -> Result<McpResponse> {
        let target = arguments.get("target").and_then(|v| v.as_str());

        let project_manager = self.project_manager.read().await;
        project_manager
            .build_project(target)
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "success": true,
                "message": "Project built successfully"
            })),
            error: None,
        })
    }

    async fn handle_project_status(&self) -> Result<McpResponse> {
        let project_manager = self.project_manager.read().await;
        let status = project_manager
            .get_project_status()
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "status": status
            })),
            error: None,
        })
    }

    async fn handle_test_run(&self, arguments: &serde_json::Value) -> Result<McpResponse> {
        let suite = arguments.get("suite").and_then(|v| v.as_str());

        let device_id = arguments.get("device_id").and_then(|v| v.as_str());

        let test_runner = self.test_runner.read().await;
        test_runner
            .run_tests(suite, device_id)
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "success": true,
                "message": "Tests completed successfully"
            })),
            error: None,
        })
    }

    async fn handle_test_record(&self, arguments: &serde_json::Value) -> Result<McpResponse> {
        let output = arguments
            .get("output")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Output path not provided".to_string()))?;

        let test_runner = self.test_runner.read().await;
        test_runner
            .record_test(output)
            .await
            .map_err(|e| KMobileError::McpServerError(e.to_string()))?;

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "success": true,
                "message": format!("Test recorded to: {}", output)
            })),
            error: None,
        })
    }

    async fn handle_resources_list(&self) -> Result<McpResponse> {
        let resources: Vec<&McpResource> = self.resources.values().collect();

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "resources": resources
            })),
            error: None,
        })
    }

    async fn handle_resource_read(&self, params: serde_json::Value) -> Result<McpResponse> {
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Resource URI not provided".to_string()))?;

        // TODO: Implement actual resource reading based on URI
        warn!("Resource reading not yet implemented for URI: {}", uri);

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "contents": []
            })),
            error: None,
        })
    }

    async fn handle_prompts_list(&self) -> Result<McpResponse> {
        let prompts: Vec<&McpPrompt> = self.prompts.values().collect();

        Ok(McpResponse {
            result: Some(serde_json::json!({
                "prompts": prompts
            })),
            error: None,
        })
    }

    async fn handle_prompt_get(&self, params: serde_json::Value) -> Result<McpResponse> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KMobileError::McpServerError("Prompt name not provided".to_string()))?;

        if let Some(prompt) = self.prompts.get(name) {
            Ok(McpResponse {
                result: Some(serde_json::json!({
                    "description": prompt.description,
                    "messages": []
                })),
                error: None,
            })
        } else {
            Ok(McpResponse {
                result: None,
                error: Some(McpError {
                    code: -32602,
                    message: "Prompt not found".to_string(),
                    data: None,
                }),
            })
        }
    }
}
