use anyhow::Result;
use clap::Parser;
use lazywt::cli::{Cli, Commands};
use lazywt::commands;
use lazywt::config::Config;
use lazywt::git;
use lazywt::layout;
use lazywt::output;
use lazywt::update_check;

fn main() {
    // Clean up stale navigation file from previous run
    lazywt::navigate::cleanup();

    let config = Config::load();

    // Override clap's binary name with command_name from config (D-07, D-09, FOUND-09)
    let cli = Cli::parse_from({
        let mut args: Vec<String> = std::env::args().collect();
        if !args.is_empty() {
            args[0] = config.command_name.clone();
        }
        args
    });

    if let Err(e) = run(cli, config) {
        // Per D-01: show user-friendly error, not raw backtrace
        output::error(&format!("{}", e));
        // If verbose, show the full error chain
        if std::env::args().any(|a| a == "--verbose") {
            eprintln!("\nDetails: {:?}", e);
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli, config: Config) -> Result<()> {
    let is_update = matches!(&cli.command, Commands::Update);

    let result = run_command(cli, &config);

    // Auto-update check AFTER command completes (D-30)
    // Skip if the user just ran `wt update` explicitly
    if !is_update {
        update_check::check_for_update(&config);
    }

    result
}

fn run_command(cli: Cli, config: &Config) -> Result<()> {
    match &cli.command {
        // Commands that don't need a git repo
        Commands::Version => return commands::version::run(),
        Commands::Update => return commands::update::run(),
        Commands::Clone { url, dirname, bare_dir } => {
            return commands::clone::run(url, dirname.as_deref(), bare_dir.as_deref(), config);
        }
        Commands::Alias => return commands::alias::run(config),
        Commands::Init { shell, add } => {
            return commands::init_shell::run(shell.as_deref(), *add, config);
        }
        Commands::Completions { ref shell } => {
            return commands::completions::run(shell);
        }
        Commands::Config { global, repo } => {
            // Config can work without a repo (--global), but --repo needs repo context
            // Try to find repo context optionally for --repo and scope selection
            let project_dir = if *repo {
                Some(git::repo::find_bare_repo_root(config.bare_dir.as_deref())?)
            } else {
                git::repo::find_bare_repo_root(config.bare_dir.as_deref()).ok()
            };
            return commands::config_cmd::run(*global, *repo, config, project_dir.as_deref());
        }
        Commands::Setup { dry_run, yes, bare_dir } => {
            return commands::setup::run(config, *dry_run, *yes, bare_dir.as_deref());
        }
        // Commands that need repo context -- fall through
        _ => {}
    }

    // Resolve repo root -- supports both bare and non-bare repos (FOUND-01)
    let (project_dir, _is_bare) = git::repo::find_repo_root(config.bare_dir.as_deref())?;

    // Apply per-repo config overrides (D-36 priority: env > repo > global > default)
    let config = &config.with_repo(&project_dir);

    // Detect layout and build ProjectContext (FOUND-03)
    let ctx = layout::detect_layout(&project_dir, config)?;

    // Dispatch to command
    match cli.command {
        Commands::List { short, json, no_path, no_color } => {
            commands::list::run(&ctx, config, short, json, no_path, no_color)
        }
        Commands::Switch { ref name } => {
            commands::switch::run(&ctx, config, name.as_deref())
        }
        Commands::FixFetch => commands::fix_fetch::run(&ctx),
        Commands::Add {
            name,
            branch,
            from,
        } => commands::add::run(&ctx, config, &name, branch.as_deref(), from.as_deref()),
        Commands::Remove { name, yes } => commands::remove::run(&ctx, config, &name, yes),
        Commands::RemoveAll { yes } => commands::remove_all::run(&ctx, config, yes),
        Commands::Migrate { dry_run, yes, bare_dir } => commands::migrate::run(&ctx, config, dry_run, yes, bare_dir.as_deref()),
        Commands::Pr {
            ref target,
            ref name,
        } => commands::pr::run(&ctx, config, target, name.as_deref()),
        Commands::Prune { force, yes } => commands::prune::run(&ctx, config, force, yes),
        Commands::Sync { all } => commands::sync::run(&ctx, config, all),
        Commands::Version
        | Commands::Update
        | Commands::Clone { .. }
        | Commands::Alias
        | Commands::Init { .. }
        | Commands::Completions { .. }
        | Commands::Config { .. }
        | Commands::Setup { .. } => {
            unreachable!()
        }
    }
}
