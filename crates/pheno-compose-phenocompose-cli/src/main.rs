use clap::{Parser, Subcommand};
use phenocompose_cli::{
    apply, down, export_provenance, load_job_provenance, load_manifest, render_plan, run_action, status, CliError,
    ErrorKind, Result,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "pheno-compose", version, about)]
struct Cli {
    #[arg(long, global = true, default_value = ".phenocompose/runs")]
    state_dir: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate and deterministically normalize a composition manifest.
    Plan { manifest: PathBuf },
    /// Apply a manifest through real providers, or render with no mutation.
    Apply {
        manifest: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Query real runtime status using persisted run state.
    Status { run_id: String },
    /// Stop containers belonging to a persisted run.
    Down { run_id: String },
    /// Run a declared action through the bounded NanoVMS evaluation boundary.
    RunAction {
        run_id: String,
        action: String,
        #[arg(long)]
        job_id: String,
    },
    /// Export provenance from an existing persisted run.
    ExportProvenance {
        run_id: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    if let Err(error) = execute(Cli::parse()) {
        let rendered = serde_json::to_string(&error.envelope()).unwrap_or_else(|_| {
            "{\"error\":{\"kind\":\"backend\",\"code\":\"error_render\",\"message\":\"failed to render error\"}}"
                .to_owned()
        });
        eprintln!("{rendered}");
        std::process::exit(exit_code(error.kind));
    }
}

fn execute(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Plan { manifest } => {
            let manifest = load_manifest(&manifest)?;
            print_json(&render_plan(&manifest)?)
        }
        Commands::Apply { manifest, dry_run } => {
            let manifest = load_manifest(&manifest)?;
            print_json(&apply(manifest, &cli.state_dir, dry_run)?)
        }
        Commands::Status { run_id } => print_json(&status(&cli.state_dir, &run_id)?),
        Commands::Down { run_id } => print_json(&down(&cli.state_dir, &run_id)?),
        Commands::RunAction { run_id, action, job_id } => match run_action(&cli.state_dir, &run_id, &action, &job_id) {
            Ok(provenance) => print_json(&provenance),
            Err(error) => {
                if let Ok(provenance) = load_job_provenance(&cli.state_dir, &run_id, &job_id) {
                    print_json(&provenance)?;
                }
                Err(error)
            }
        },
        Commands::ExportProvenance { run_id, output } => {
            let provenance = export_provenance(&cli.state_dir, &run_id)?;
            if let Some(path) = output {
                write_json(&path, &provenance)
            } else {
                print_json(&provenance)
            }
        }
    }
}

fn print_json(value: &impl Serialize) -> Result<()> {
    let output = serde_json::to_string_pretty(value).map_err(CliError::json)?;
    println!("{output}");
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let output = serde_json::to_vec_pretty(value).map_err(CliError::json)?;
    fs::write(path, output).map_err(|error| CliError::io("output_write", error))
}

fn exit_code(kind: ErrorKind) -> i32 {
    match kind {
        ErrorKind::Validation => 2,
        ErrorKind::Unsupported => 3,
        ErrorKind::NotFound => 4,
        ErrorKind::Conflict => 5,
        ErrorKind::Backend | ErrorKind::Io => 1,
    }
}
