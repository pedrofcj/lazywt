//! Benchmark-only entry point: never load user config or run update/navigation code.
use anyhow::{bail, Result};
use clap::Parser;
use lazywt::cli::{Cli, Commands};
use lazywt::commands::list::{collect, json};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let short = match cli.command {
        Commands::List { short, json, .. } if json => short,
        _ => bail!("benchmark worker requires list --json [--short]"),
    };
    let cwd = dunce::canonicalize(std::env::current_dir()?)?;
    let (infos, _) = collect::collect_all(&cwd, Some(&cwd), short)?;
    json::print_json(&infos, "benchmark")
}
