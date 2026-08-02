//! tag-forge — universal cost-allocation tag taxonomy validator and enforcer.
//!
//! See `README.md` and `CLAUDE.md` for design rationale and `docs/` for
//! per-subcommand reference.

mod config;
mod scan;
mod taxonomy;
mod validate;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "tag-forge",
    version,
    about = "Universal cost-allocation tag taxonomy enforcer",
    long_about = None,
)]
struct Cli {
    /// Path to the tag-forge config file (taxonomy + validators + virtual allocation rules).
    #[arg(short, long, env = "TAG_FORGE_CONFIG", default_value = "tag-forge.yaml")]
    config: PathBuf,

    /// Increase logging verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate a plan of resources against the configured taxonomy.
    Validate {
        /// Path to the plan YAML.
        plan: PathBuf,

        /// Exit zero even if violations are found (useful for reporting modes).
        #[arg(long)]
        no_fail: bool,

        /// Output format: human (default), json.
        #[arg(long, default_value = "human")]
        format: String,
    },

    /// Scan a directory of resource manifests; report tag coverage per dimension.
    Scan {
        /// Root path to scan recursively.
        path: PathBuf,

        /// Output format: human (default), json.
        #[arg(long, default_value = "human")]
        format: String,
    },

    /// Inspect the configured taxonomy.
    Taxonomy {
        #[command(subcommand)]
        action: TaxonomyAction,
    },
}

#[derive(Subcommand, Debug)]
enum TaxonomyAction {
    /// Print the full taxonomy.
    Print,

    /// Describe one dimension.
    Describe {
        /// Dimension name (e.g. cost_center).
        dimension: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    let cfg = config::load(&cli.config)?;
    tracing::debug!(config_path = %cli.config.display(), "loaded config");

    match cli.command {
        Command::Validate { plan, no_fail, format } => {
            let report = validate::run(&cfg.taxonomy, &plan)?;
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&report)?),
                _ => println!("{}", report.render_human()),
            }
            if !report.violations.is_empty() && !no_fail {
                std::process::exit(1);
            }
        }
        Command::Scan { path, format } => {
            let report = scan::run(&cfg.taxonomy, &path)?;
            match format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&report)?),
                _ => println!("{}", report.render_human()),
            }
        }
        Command::Taxonomy { action } => match action {
            TaxonomyAction::Print => println!("{}", cfg.taxonomy.render()),
            TaxonomyAction::Describe { dimension } => match cfg.taxonomy.describe(&dimension) {
                Some(text) => println!("{text}"),
                None => {
                    eprintln!("unknown dimension: {dimension}");
                    std::process::exit(1);
                }
            },
        },
    }

    Ok(())
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| format!("tag_forge={level}"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
