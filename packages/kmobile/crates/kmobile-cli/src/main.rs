//! KMobile CLI — entry point for the command-line interface.
//!
//! This binary wires the concrete adapter implementations in
//! `kmobile_cli::adapters` to the hex CLI defined below. The domain
//! logic stays in `kmobile_core`; this crate is purely the driving
//! adapter.

mod adapters;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use kmobile_core::config::Config;
use kmobile_core::ports::{
    DevicePort, MaterialPort, McpPort, PluginPort, ProjectPort, SerializationPort, SimulatorPort,
    TestingPort,
};

use adapters::{
    CliDeviceAdapter, CliMaterialAdapter, CliMcpAdapter, CliPluginAdapter, CliProjectAdapter,
    CliSerializationAdapter, CliSimulatorAdapter, CliTestingAdapter,
};

/// Top-level CLI arguments.
#[derive(Parser, Debug)]
#[command(name = "kmobile")]
#[command(about = "KMobile - Comprehensive mobile development and testing automation")]
#[command(version, long_about = None)]
struct Args {
    /// Path to a `kmobile.toml` configuration file.
    #[arg(long, global = true)]
    config: Option<String>,

    /// Enable verbose logging.
    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

/// Top-level subcommands. The actual command set is kept compact; the
/// adapters do the heavy lifting.
#[derive(Subcommand, Debug)]
enum Command {
    /// Initialise a new KMobile project.
    Init {
        name: String,
        #[arg(long)]
        template: Option<String>,
    },
    /// Device management commands.
    Device {
        #[command(subcommand)]
        action: DeviceCmd,
    },
    /// Simulator management commands.
    Simulator {
        #[command(subcommand)]
        action: SimulatorCmd,
    },
    /// Project build / clean / status commands.
    Project {
        #[command(subcommand)]
        action: ProjectCmd,
    },
    /// Test execution commands.
    Test {
        #[command(subcommand)]
        action: TestCmd,
    },
    /// Build-asset (icon / splash / signing-key) commands.
    Asset {
        #[command(subcommand)]
        action: AssetCmd,
    },
    /// MCP tool-registry commands.
    Mcp {
        #[command(subcommand)]
        action: McpCmd,
    },
    /// Plugin-registry commands.
    Plugin {
        #[command(subcommand)]
        action: PluginCmd,
    },
    /// Print the loaded configuration in TOML form.
    DumpConfig,
}

#[derive(Subcommand, Debug)]
enum DeviceCmd {
    /// List connected devices.
    List,
    /// Connect to a device.
    Connect { id: String },
    /// Install an app on a device.
    Install { id: String, app: String },
    /// Deploy the current project to a device.
    Deploy {
        id: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Run a test suite on a device.
    Test {
        id: String,
        #[arg(long)]
        suite: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum SimulatorCmd {
    /// List available simulators.
    List,
    /// Start a simulator.
    Start { id: String },
    /// Stop a simulator.
    Stop { id: String },
    /// Reset a simulator.
    Reset { id: String },
    /// Install an app on a simulator.
    Install { id: String, app: String },
}

#[derive(Subcommand, Debug)]
enum ProjectCmd {
    /// Build the current project.
    Build {
        #[arg(long)]
        target: Option<String>,
    },
    /// Clean the current project.
    Clean,
    /// Print project status.
    Status,
}

#[derive(Subcommand, Debug)]
enum TestCmd {
    /// Run a test suite.
    Run {
        #[arg(long)]
        suite: Option<String>,
        #[arg(long)]
        device: Option<String>,
    },
    /// Record a test (placeholder; see warnings in the adapter).
    Record { output: String },
    /// Replay a test from a JSON file.
    Replay { file: String },
}

#[derive(Subcommand, Debug)]
enum McpCmd {
    /// List registered MCP tools.
    List,
    /// Register a new MCP tool. `kind` is `device` | `simulator` |
    /// `project` | `testing` | `custom`. The tool is created in
    /// the registry without a handler — the user follows up with
    /// `register-handler` if they want to bind a CLI argv.
    Register {
        /// Stable, transport-agnostic tool id (e.g. `list_devices`).
        id: String,
        /// Human-readable tool name.
        name: String,
        /// Logical kind of the tool.
        #[arg(long, default_value = "custom")]
        kind: String,
        /// One-line description surfaced to MCP clients.
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Remove a registered MCP tool by id.
    Deregister { id: String },
    /// Show a single registered MCP tool by id.
    Get { id: String },
    /// Invoke a registered MCP tool. Arguments are passed as a
    /// JSON object on the command line.
    Invoke {
        id: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
    /// Bind a CLI handler to an MCP tool id. The first argv entry
    /// is the binary; the rest are passed as-is.
    RegisterHandler {
        id: String,
        #[arg(required = true)]
        argv: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum PluginCmd {
    /// List registered plugins.
    List,
    /// Load (register) a new plugin.
    Load { name: String },
    /// Enable a plugin by id.
    Enable { id: String },
    /// Disable a plugin by id.
    Disable { id: String },
    /// Unload (remove) a plugin by id.
    Unload { id: String },
}

#[derive(Subcommand, Debug)]
enum AssetCmd {
    /// List registered assets.
    List {
        #[arg(long)]
        project: Option<String>,
    },
    /// Register a new asset (kind defaults to `other`).
    Add {
        kind: String,
        path: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove an asset by id.
    Remove {
        id: String,
        #[arg(long)]
        project: Option<String>,
    },
    /// Show a single asset by id.
    Get {
        id: String,
        #[arg(long)]
        project: Option<String>,
    },
}

/// Parse the `kind` string from the CLI into the [`kmobile_core::ports::AssetKind`] enum.
fn parse_asset_kind(s: &str) -> kmobile_core::ports::AssetKind {
    use kmobile_core::ports::AssetKind;
    match s.to_ascii_lowercase().as_str() {
        "icon" => AssetKind::Icon,
        "splash" => AssetKind::Splash,
        "signing-key" | "signing_key" | "signingkey" => AssetKind::SigningKey,
        "push-certificate" | "push_certificate" | "pushcertificate" => AssetKind::PushCertificate,
        _ => AssetKind::Other,
    }
}

/// Parse the `kind` string from the CLI into the [`kmobile_core::ports::McpToolKind`] enum.
fn parse_mcp_kind(s: &str) -> kmobile_core::ports::McpToolKind {
    use kmobile_core::ports::McpToolKind;
    match s.to_ascii_lowercase().as_str() {
        "device" => McpToolKind::Device,
        "simulator" => McpToolKind::Simulator,
        "project" => McpToolKind::Project,
        "testing" | "test" => McpToolKind::Testing,
        _ => McpToolKind::Custom,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("warn"))
            .init();
    }

    let config = Config::load(args.config.as_deref())?;

    // Build all adapters up front so commands can dispatch into them
    // without extra setup work.
    let device_adapter = CliDeviceAdapter::new(config.clone()).await?;
    let simulator_adapter = CliSimulatorAdapter::new(config.clone()).await?;
    let project_adapter = CliProjectAdapter::new(config.clone()).await?;
    let testing_adapter = CliTestingAdapter::new(config.clone()).await?;
    let asset_root = std::env::current_dir()?.join(".kmobile").join("assets");
    let material_adapter = CliMaterialAdapter::new(&asset_root).await?;
    let mcp_root = std::env::current_dir()?
        .join(".kmobile")
        .join("mcp-tools.json");
    let mcp_adapter = CliMcpAdapter::new(&mcp_root).await?;
    let plugin_root = std::env::current_dir()?
        .join(".kmobile")
        .join("plugins.json");
    let plugin_adapter = CliPluginAdapter::new(&plugin_root).await?;
    let serialization_adapter = CliSerializationAdapter::new();

    match args.command {
        Command::Init { name, template } => {
            project_adapter
                .init_project(&name, template.as_deref())
                .await?;
            println!("Initialized project: {name}");
        }
        Command::Device { action } => match action {
            DeviceCmd::List => {
                for d in device_adapter.list_devices().await? {
                    println!("{} - {} ({})", d.id, d.name, d.platform);
                }
            }
            DeviceCmd::Connect { id } => {
                device_adapter.connect_device(&id).await?;
                println!("Connected: {id}");
            }
            DeviceCmd::Install { id, app } => {
                device_adapter.install_app(&id, &app).await?;
                println!("Installed {app} on {id}");
            }
            DeviceCmd::Deploy { id, project } => {
                device_adapter
                    .deploy_project(&id, project.as_deref())
                    .await?;
                println!("Deployed to {id}");
            }
            DeviceCmd::Test { id, suite } => {
                device_adapter
                    .run_device_tests(&id, suite.as_deref())
                    .await?;
                println!("Tests finished on {id}");
            }
        },
        Command::Simulator { action } => match action {
            SimulatorCmd::List => {
                for s in simulator_adapter.list_simulators().await? {
                    println!("{} - {} ({})", s.id, s.name, s.platform);
                }
            }
            SimulatorCmd::Start { id } => {
                simulator_adapter.start_simulator(&id).await?;
                println!("Started simulator: {id}");
            }
            SimulatorCmd::Stop { id } => {
                simulator_adapter.stop_simulator(&id).await?;
                println!("Stopped simulator: {id}");
            }
            SimulatorCmd::Reset { id } => {
                simulator_adapter.reset_simulator(&id).await?;
                println!("Reset simulator: {id}");
            }
            SimulatorCmd::Install { id, app } => {
                simulator_adapter.install_app(&id, &app).await?;
                println!("Installed {app} on simulator {id}");
            }
        },
        Command::Project { action } => match action {
            ProjectCmd::Build { target } => {
                project_adapter.build_project(target.as_deref()).await?;
                println!("Project built");
            }
            ProjectCmd::Clean => {
                project_adapter.clean_project().await?;
                println!("Project cleaned");
            }
            ProjectCmd::Status => {
                let status = project_adapter.get_project_status().await?;
                println!("{}", status.state);
            }
        },
        Command::Test { action } => match action {
            TestCmd::Run { suite, device } => {
                let r = testing_adapter
                    .run_tests(suite.as_deref(), device.as_deref())
                    .await?;
                println!(
                    "Suite {}: {} passed, {} failed",
                    r.suite, r.passed, r.failed
                );
            }
            TestCmd::Record { output } => {
                testing_adapter.record_test(&output).await?;
                println!("Recorded to {output}");
            }
            TestCmd::Replay { file } => {
                testing_adapter.replay_test(&file).await?;
            }
        },
        Command::Asset { action } => match action {
            AssetCmd::List { project } => {
                let assets = material_adapter
                    .list_assets(project.as_deref())
                    .await
                    .map_err(anyhow::Error::from)?;
                for a in assets {
                    println!(
                        "{} {:?} {} ({} bytes)",
                        a.id,
                        a.kind,
                        a.path.display(),
                        a.size_bytes
                    );
                }
            }
            AssetCmd::Add {
                kind,
                path,
                project,
            } => {
                let info = material_adapter
                    .add_asset(
                        project.as_deref(),
                        parse_asset_kind(&kind),
                        std::path::PathBuf::from(path),
                    )
                    .await
                    .map_err(anyhow::Error::from)?;
                println!("Added {} ({:?})", info.id, info.kind);
            }
            AssetCmd::Remove { id, project } => {
                let removed = material_adapter
                    .remove_asset(project.as_deref(), &id)
                    .await
                    .map_err(anyhow::Error::from)?;
                println!("Removed: {removed}");
            }
            AssetCmd::Get { id, project } => {
                let asset = material_adapter
                    .get_asset(project.as_deref(), &id)
                    .await
                    .map_err(anyhow::Error::from)?;
                match asset {
                    Some(a) => println!("{} {:?} {}", a.id, a.kind, a.path.display()),
                    None => println!("Not found: {id}"),
                }
            }
        },
        Command::Mcp { action } => match action {
            McpCmd::List => {
                for t in mcp_adapter.list_tools().await? {
                    println!("{} - {} ({:?})", t.id, t.name, t.kind);
                }
            }
            McpCmd::Register {
                id,
                name,
                kind,
                description,
            } => {
                use kmobile_core::ports::McpToolInfo;
                let tool = McpToolInfo {
                    id: id.clone(),
                    name,
                    kind: parse_mcp_kind(&kind),
                    description,
                    input_schema: None,
                };
                let stored = mcp_adapter.register_tool(tool).await?;
                println!(
                    "Registered mcp tool: {} (kind={:?})",
                    stored.id, stored.kind
                );
            }
            McpCmd::Deregister { id } => {
                let removed = mcp_adapter.deregister_tool(&id).await?;
                println!("Deregistered: {removed}");
            }
            McpCmd::Get { id } => match mcp_adapter.get_tool(&id).await? {
                Some(t) => println!("{} - {} ({:?}): {}", t.id, t.name, t.kind, t.description),
                None => println!("Not found: {id}"),
            },
            McpCmd::Invoke { id, args } => {
                let parsed: serde_json::Value =
                    serde_json::from_str(&args).unwrap_or(serde_json::json!({}));
                let r = mcp_adapter.invoke_tool(&id, parsed).await?;
                if r.success {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&r.output).unwrap_or_default()
                    );
                } else {
                    eprintln!("invocation failed: {}", r.error.unwrap_or_default());
                }
            }
            McpCmd::RegisterHandler { id, argv } => {
                mcp_adapter.register_handler(&id, argv).await?;
                println!("Handler registered for {id}");
            }
        },
        Command::Plugin { action } => match action {
            PluginCmd::List => {
                for p in plugin_adapter.list_plugins().await? {
                    println!("{} - {} (state={:?})", p.id, p.name, p.state);
                }
            }
            PluginCmd::Load { name } => {
                use kmobile_core::ports::PluginSource;
                let info = plugin_adapter
                    .load_plugin(&name, PluginSource::Registry(name.clone()))
                    .await?;
                println!("Loaded plugin: {} (id={})", info.name, info.id);
            }
            PluginCmd::Enable { id } => {
                let info = plugin_adapter.enable_plugin(&id).await?;
                println!("Enabled plugin: {} (state={:?})", info.id, info.state);
            }
            PluginCmd::Disable { id } => {
                let info = plugin_adapter.disable_plugin(&id).await?;
                println!("Disabled plugin: {} (state={:?})", info.id, info.state);
            }
            PluginCmd::Unload { id } => {
                let removed = plugin_adapter.unload_plugin(&id).await?;
                println!("Unloaded: {removed}");
            }
        },
        Command::DumpConfig => {
            let path = std::env::current_dir()?.join("kmobile.dump.toml");
            serialization_adapter.save(&config, &path).await?;
            println!("Wrote {}", path.display());
        }
    }

    Ok(())
}
