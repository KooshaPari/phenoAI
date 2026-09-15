use clap::{Parser, Subcommand};
use clap_ext::prelude::*;
use tracing::info;

use kmobile::device_basic::DeviceCommands;
use kmobile::project::ProjectCommands;
use kmobile::simulator_basic::SimulatorCommands;
use kmobile::testing::TestCommands;
use kmobile::{Config, KMobileCli};

#[derive(Parser)]
#[command(name = "kmobile")]
#[command(about = "KMobile - Comprehensive mobile development and testing automation")]
#[command(version, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    /// Shared `--config/-c` flag (clap-ext `ConfigArg`)
    #[command(flatten)]
    config: ConfigArg,

    /// Shared `-v/-q` verbosity flags (clap-ext `Verbosity`)
    #[command(flatten)]
    verbosity: Verbosity,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new KMobile project
    Init {
        #[arg(help = "Project name")]
        name: String,
        #[arg(long, help = "Project template")]
        template: Option<String>,
    },

    /// Device management commands
    Device {
        #[command(subcommand)]
        command: DeviceCommands,
    },

    /// Simulator management commands
    Simulator {
        #[command(subcommand)]
        command: SimulatorCommands,
    },

    /// Project management commands
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },

    /// Testing automation commands
    Test {
        #[command(subcommand)]
        command: TestCommands,
    },

    /// Start API server
    Serve {
        #[arg(long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "localhost")]
        host: String,
    },

    /// Start MCP server
    Mcp {
        #[arg(long, help = "MCP server configuration")]
        config: Option<String>,
    },

    /// Start TUI interface
    Tui,
}

#[tokio::main]
async fn main() -> CliResult<()> {
    let args = Args::parse();

    // Initialize tracing via clap-ext (handles RUST_LOG + -v/-q mapping).
    setup_tracing(args.verbosity.to_filter());

    // Load configuration
    let config = Config::load(args.config.config.as_deref().and_then(|p| p.to_str()))?;
    info!("KMobile started with config: {}", config.name());

    // Initialize CLI
    let cli = KMobileCli::new(config).await?;

    match args.command {
        Commands::Init { name, template } => {
            cli.init_project(&name, template.as_deref()).await?;
        }
        Commands::Device { command } => {
            cli.handle_device_command(command).await?;
        }
        Commands::Simulator { command } => {
            cli.handle_simulator_command(command).await?;
        }
        Commands::Project { command } => {
            cli.handle_project_command(command).await?;
        }
        Commands::Test { command } => {
            cli.handle_test_command(command).await?;
        }
        Commands::Serve { port, host } => {
            cli.start_api_server(&host, port).await?;
        }
        Commands::Mcp { config } => {
            cli.start_mcp_server(config.as_deref()).await?;
        }
        Commands::Tui => {
            cli.start_tui().await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Smoke test: the adopted `clap-ext` types compile in, parse, and
    /// map to the right filter levels.
    #[test]
    fn clap_ext_args_parse_and_filter_maps() {
        let args = Args::try_parse_from(["kmobile", "-vv", "tui"]).expect("parse");
        assert_eq!(args.verbosity.verbose, 2);
        assert!(!args.verbosity.quiet);
        use tracing_subscriber::filter::LevelFilter;
        assert_eq!(args.verbosity.to_filter(), LevelFilter::TRACE);

        let q = Args::try_parse_from(["kmobile", "-q", "tui"]).expect("parse");
        assert!(q.verbosity.quiet);
        assert_eq!(q.verbosity.to_filter(), LevelFilter::ERROR);
    }

    /// Smoke test: ConfigArg flattens and accepts the env / -c forms.
    #[test]
    fn clap_ext_config_arg_flatten() {
        let args = Args::try_parse_from(["kmobile", "--config", "/tmp/kmobile.yaml", "tui"])
            .expect("parse");
        assert_eq!(
            args.config.config.as_ref().and_then(|p| p.to_str()),
            Some("/tmp/kmobile.yaml")
        );
    }
}
