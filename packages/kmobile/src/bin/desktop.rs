use anyhow::Result;
use clap::Parser;
use kmobile::desktop::{Args, KMobileDesktopApp};

#[derive(Parser)]
#[command(name = "kmobile-desktop")]
#[command(
    about = "KMobile Desktop - Revolutionary hardware emulation and visual control for mobile devices"
)]
struct CliArgs {
    #[arg(long, default_value = "3000")]
    pub port: u16,

    #[arg(long, default_value = "localhost")]
    pub host: String,

    #[arg(long)]
    pub device_id: Option<String>,

    #[arg(long)]
    pub fullscreen: bool,

    #[arg(long)]
    pub debug: bool,
}

impl From<CliArgs> for Args {
    fn from(cli: CliArgs) -> Self {
        Args {
            port: cli.port,
            host: cli.host,
            device_id: cli.device_id,
            fullscreen: cli.fullscreen,
            debug: cli.debug,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args = CliArgs::parse();

    // Initialize logging
    if cli_args.debug {
        std::env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    let args: Args = cli_args.into();

    // Initialize and run the desktop application
    let app = KMobileDesktopApp::new(&args).await?;
    app.run().await?;

    Ok(())
}
