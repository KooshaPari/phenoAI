use anyhow::Result;
use clap::Parser;
use std::io::{self, BufRead, BufReader, Write};
use tracing::{debug, error, info};

use kmobile::{Config, McpRequest, McpServer};

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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(if args.debug { "debug" } else { "info" })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting KMobile MCP Server");

    // Load configuration
    let config = Config::load(args.config.as_deref())?;

    // Initialize MCP server
    let mcp_server = McpServer::new(&config, args.config.as_deref()).await?;

    // Handle stdio communication
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let reader = BufReader::new(stdin);

    info!("MCP Server ready, listening on stdio");

    for line in reader.lines() {
        match line {
            Ok(input) => {
                if input.trim().is_empty() {
                    continue;
                }

                debug!("Received input: {}", input);

                // Parse JSON-RPC request
                match serde_json::from_str::<serde_json::Value>(&input) {
                    Ok(json) => {
                        let request = McpRequest {
                            method: json
                                .get("method")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            params: json.get("params").cloned().unwrap_or(serde_json::json!({})),
                        };

                        // Handle the request
                        match mcp_server.handle_request(request).await {
                            Ok(response) => {
                                let response_json = serde_json::to_string(&response)?;
                                writeln!(stdout, "{response_json}")?;
                                stdout.flush()?;
                            }
                            Err(e) => {
                                error!("Error handling request: {}", e);
                                let error_response = serde_json::json!({
                                    "error": {
                                        "code": -32603,
                                        "message": "Internal error",
                                        "data": e.to_string()
                                    }
                                });
                                writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                                stdout.flush()?;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to parse JSON: {}", e);
                        let error_response = serde_json::json!({
                            "error": {
                                "code": -32700,
                                "message": "Parse error",
                                "data": e.to_string()
                            }
                        });
                        writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                        stdout.flush()?;
                    }
                }
            }
            Err(e) => {
                error!("Error reading from stdin: {}", e);
                break;
            }
        }
    }

    info!("MCP Server shutting down");
    Ok(())
}
