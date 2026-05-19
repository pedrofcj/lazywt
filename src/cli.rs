use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "lazywt",
    version,
    about = "Git worktree manager for bare repositories"
)]
pub struct Cli {
    /// Enable verbose output (show raw git errors)
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum Commands {
    /// List all worktrees
    List {
        /// Minimal output: name + branch + dirty only
        #[arg(long)]
        short: bool,

        /// Output as JSON for scripting
        #[arg(long)]
        json: bool,

        /// Hide worktree path lines
        #[arg(long)]
        no_path: bool,

        /// Disable colored output
        #[arg(long)]
        no_color: bool,
    },

    /// Switch to a worktree (interactive picker, direct name, or previous with -)
    Switch {
        /// Worktree name to switch to (use - for previous worktree)
        name: Option<String>,
    },

    /// Fix fetch refspec configuration for bare repos
    #[command(name = "fix-fetch")]
    FixFetch,

    /// Show current version
    Version,

    /// Clone a repo as bare and set up worktree structure
    Clone {
        /// Repository URL
        url: String,
        /// Optional custom directory name (default: derived from URL)
        dirname: Option<String>,
        /// Custom bare repo directory name (default: from config or .git)
        #[arg(long = "bare-dir")]
        bare_dir: Option<String>,
    },

    /// Create a new worktree
    Add {
        /// Worktree name (directory name for the new worktree)
        name: String,
        /// Explicit branch name (overrides branch_prefix config)
        #[arg(long, short)]
        branch: Option<String>,
        /// Base branch or worktree to branch from
        #[arg(long)]
        from: Option<String>,
    },

    /// Remove a worktree and its branch
    Remove {
        /// Worktree name to remove
        name: String,
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Remove all non-default worktrees
    #[command(name = "remove-all")]
    RemoveAll {
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Migrate classic layout to modern layout
    Migrate {
        /// Show what would happen without making changes
        #[arg(long)]
        dry_run: bool,
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
        /// Custom bare repo directory name (default: from config or .git)
        #[arg(long = "bare-dir")]
        bare_dir: Option<String>,
    },

    /// Interactively edit configuration
    Config {
        /// Edit global config directly
        #[arg(long, conflicts_with = "repo")]
        global: bool,
        /// Edit per-repo config directly
        #[arg(long, conflicts_with = "global")]
        repo: bool,
    },

    /// Check for updates
    Update,

    /// Checkout a GitHub pull request into a worktree
    Pr {
        /// PR number or GitHub URL (e.g., 42 or https://github.com/owner/repo/pull/42)
        target: String,
        /// Custom worktree name (default: pr-<number>)
        name: Option<String>,
    },

    /// Remove merged and orphaned worktrees
    Prune {
        /// Also remove dirty worktrees
        #[arg(long)]
        force: bool,
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
    },

    /// Fetch remotes and fast-forward branches
    Sync {
        /// Fast-forward all worktree branches (not just default)
        #[arg(long)]
        all: bool,
    },

    /// Set up shell alias for lazywt
    #[command(hide = true)]
    Alias,

    /// Output shell function for eval (e.g., eval "$(lazywt init)")
    Init {
        /// Shell name: bash, zsh, powershell, nushell (auto-detected if omitted)
        shell: Option<String>,
        /// Append eval line to shell profile file
        #[arg(long)]
        add: bool,
    },

    /// Generate shell completions
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for: bash, zsh, powershell, fish
        shell: String,
    },

    /// Convert a regular repo to bare repository with worktree structure
    Setup {
        /// Show what would happen without making changes
        #[arg(long)]
        dry_run: bool,
        /// Skip confirmation prompt
        #[arg(long, short)]
        yes: bool,
        /// Custom bare repo directory name (default: from config or .git)
        #[arg(long = "bare-dir")]
        bare_dir: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_list_command() {
        let cli = Cli::parse_from(["lazywt", "list"]);
        match cli.command {
            Commands::List { short, json, no_path, no_color } => {
                assert!(!short);
                assert!(!json);
                assert!(!no_path);
                assert!(!no_color);
            }
            _ => panic!("Expected List command"),
        }
        assert!(!cli.verbose);
    }

    #[test]
    fn parse_fix_fetch_command() {
        let cli = Cli::parse_from(["lazywt", "fix-fetch"]);
        assert_eq!(cli.command, Commands::FixFetch);
    }

    #[test]
    fn parse_version_command() {
        let cli = Cli::parse_from(["lazywt", "version"]);
        assert_eq!(cli.command, Commands::Version);
    }

    #[test]
    fn parse_verbose_flag_before_subcommand() {
        let cli = Cli::parse_from(["lazywt", "--verbose", "list"]);
        assert!(cli.verbose);
        assert!(matches!(cli.command, Commands::List { .. }));
    }

    #[test]
    fn parse_verbose_flag_after_subcommand() {
        let cli = Cli::parse_from(["lazywt", "list", "--verbose"]);
        assert!(cli.verbose);
        assert!(matches!(cli.command, Commands::List { .. }));
    }

    #[test]
    fn parse_clone_command() {
        let cli = Cli::parse_from(["lazywt", "clone", "https://github.com/user/repo.git"]);
        assert_eq!(
            cli.command,
            Commands::Clone {
                url: "https://github.com/user/repo.git".to_string(),
                dirname: None,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_clone_with_dirname() {
        let cli = Cli::parse_from([
            "lazywt",
            "clone",
            "https://github.com/user/repo.git",
            "mydir",
        ]);
        assert_eq!(
            cli.command,
            Commands::Clone {
                url: "https://github.com/user/repo.git".to_string(),
                dirname: Some("mydir".to_string()),
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_clone_with_bare_dir() {
        let cli = Cli::parse_from([
            "lazywt",
            "clone",
            "https://github.com/user/repo.git",
            "--bare-dir",
            ".repo",
        ]);
        assert_eq!(
            cli.command,
            Commands::Clone {
                url: "https://github.com/user/repo.git".to_string(),
                dirname: None,
                bare_dir: Some(".repo".to_string()),
            }
        );
    }

    #[test]
    fn parse_clone_with_dirname_and_bare_dir() {
        let cli = Cli::parse_from([
            "lazywt",
            "clone",
            "https://github.com/user/repo.git",
            "mydir",
            "--bare-dir",
            ".repo",
        ]);
        assert_eq!(
            cli.command,
            Commands::Clone {
                url: "https://github.com/user/repo.git".to_string(),
                dirname: Some("mydir".to_string()),
                bare_dir: Some(".repo".to_string()),
            }
        );
    }

    #[test]
    fn parse_add_command() {
        let cli = Cli::parse_from(["lazywt", "add", "auth"]);
        assert_eq!(
            cli.command,
            Commands::Add {
                name: "auth".to_string(),
                branch: None,
                from: None,
            }
        );
    }

    #[test]
    fn parse_add_with_from() {
        let cli = Cli::parse_from(["lazywt", "add", "auth", "--from", "dev"]);
        assert_eq!(
            cli.command,
            Commands::Add {
                name: "auth".to_string(),
                branch: None,
                from: Some("dev".to_string()),
            }
        );
    }

    #[test]
    fn parse_add_with_branch() {
        let cli = Cli::parse_from(["lazywt", "add", "auth", "--branch", "custom-branch"]);
        assert_eq!(
            cli.command,
            Commands::Add {
                name: "auth".to_string(),
                branch: Some("custom-branch".to_string()),
                from: None,
            }
        );
    }

    #[test]
    fn parse_add_with_branch_short() {
        let cli = Cli::parse_from(["lazywt", "add", "auth", "-b", "custom"]);
        assert_eq!(
            cli.command,
            Commands::Add {
                name: "auth".to_string(),
                branch: Some("custom".to_string()),
                from: None,
            }
        );
    }

    #[test]
    fn parse_add_with_branch_and_from() {
        let cli = Cli::parse_from(["lazywt", "add", "auth", "--branch", "custom", "--from", "dev"]);
        assert_eq!(
            cli.command,
            Commands::Add {
                name: "auth".to_string(),
                branch: Some("custom".to_string()),
                from: Some("dev".to_string()),
            }
        );
    }

    #[test]
    fn parse_add_rejects_extra_positional() {
        let result = Cli::try_parse_from(["lazywt", "add", "auth", "bugfix"]);
        assert!(result.is_err(), "Extra positional arg should be rejected");
    }

    #[test]
    fn parse_remove_command() {
        let cli = Cli::parse_from(["lazywt", "remove", "auth"]);
        assert_eq!(
            cli.command,
            Commands::Remove {
                name: "auth".to_string(),
                yes: false,
            }
        );
    }

    #[test]
    fn parse_remove_with_yes() {
        let cli = Cli::parse_from(["lazywt", "remove", "auth", "--yes"]);
        assert_eq!(
            cli.command,
            Commands::Remove {
                name: "auth".to_string(),
                yes: true,
            }
        );
    }

    #[test]
    fn parse_remove_all_command() {
        let cli = Cli::parse_from(["lazywt", "remove-all"]);
        assert_eq!(cli.command, Commands::RemoveAll { yes: false });
    }

    #[test]
    fn parse_remove_all_with_y() {
        let cli = Cli::parse_from(["lazywt", "remove-all", "-y"]);
        assert_eq!(cli.command, Commands::RemoveAll { yes: true });
    }

    #[test]
    fn parse_migrate_command() {
        let cli = Cli::parse_from(["lazywt", "migrate"]);
        assert_eq!(
            cli.command,
            Commands::Migrate {
                dry_run: false,
                yes: false,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_migrate_dry_run() {
        let cli = Cli::parse_from(["lazywt", "migrate", "--dry-run"]);
        assert_eq!(
            cli.command,
            Commands::Migrate {
                dry_run: true,
                yes: false,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_migrate_yes() {
        let cli = Cli::parse_from(["lazywt", "migrate", "--yes"]);
        assert_eq!(
            cli.command,
            Commands::Migrate {
                dry_run: false,
                yes: true,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_migrate_dry_run_and_yes() {
        let cli = Cli::parse_from(["lazywt", "migrate", "--dry-run", "--yes"]);
        assert_eq!(
            cli.command,
            Commands::Migrate {
                dry_run: true,
                yes: true,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_migrate_with_bare_dir() {
        let cli = Cli::parse_from(["lazywt", "migrate", "--bare-dir", ".repo", "--yes"]);
        assert_eq!(
            cli.command,
            Commands::Migrate {
                dry_run: false,
                yes: true,
                bare_dir: Some(".repo".to_string()),
            }
        );
    }

    #[test]
    fn parse_update_command() {
        let cli = Cli::parse_from(["lazywt", "update"]);
        assert_eq!(cli.command, Commands::Update);
    }

    #[test]
    fn parse_alias_command() {
        let cli = Cli::parse_from(["lazywt", "alias"]);
        assert_eq!(cli.command, Commands::Alias);
    }

    #[test]
    fn parse_init_command() {
        let cli = Cli::parse_from(["lazywt", "init", "bash"]);
        assert_eq!(
            cli.command,
            Commands::Init {
                shell: Some("bash".to_string()),
                add: false,
            }
        );
    }

    #[test]
    fn parse_init_powershell() {
        let cli = Cli::parse_from(["lazywt", "init", "powershell"]);
        assert_eq!(
            cli.command,
            Commands::Init {
                shell: Some("powershell".to_string()),
                add: false,
            }
        );
    }

    #[test]
    fn parse_init_no_args() {
        let cli = Cli::parse_from(["lazywt", "init"]);
        assert_eq!(
            cli.command,
            Commands::Init {
                shell: None,
                add: false,
            }
        );
    }

    #[test]
    fn parse_init_add_flag() {
        let cli = Cli::parse_from(["lazywt", "init", "--add"]);
        assert_eq!(
            cli.command,
            Commands::Init {
                shell: None,
                add: true,
            }
        );
    }

    #[test]
    fn parse_init_bash_add() {
        let cli = Cli::parse_from(["lazywt", "init", "bash", "--add"]);
        assert_eq!(
            cli.command,
            Commands::Init {
                shell: Some("bash".to_string()),
                add: true,
            }
        );
    }

    #[test]
    fn parse_completions_command() {
        let cli = Cli::parse_from(["lazywt", "completions", "bash"]);
        assert_eq!(
            cli.command,
            Commands::Completions {
                shell: "bash".to_string(),
            }
        );
    }

    #[test]
    fn parse_completions_zsh() {
        let cli = Cli::parse_from(["lazywt", "completions", "zsh"]);
        assert_eq!(
            cli.command,
            Commands::Completions {
                shell: "zsh".to_string(),
            }
        );
    }

    #[test]
    fn parse_switch_no_args() {
        let cli = Cli::parse_from(["lazywt", "switch"]);
        assert_eq!(
            cli.command,
            Commands::Switch { name: None }
        );
    }

    #[test]
    fn parse_switch_with_name() {
        let cli = Cli::parse_from(["lazywt", "switch", "auth"]);
        assert_eq!(
            cli.command,
            Commands::Switch {
                name: Some("auth".to_string()),
            }
        );
    }

    #[test]
    fn parse_switch_dash() {
        let cli = Cli::parse_from(["lazywt", "switch", "-"]);
        assert_eq!(
            cli.command,
            Commands::Switch {
                name: Some("-".to_string()),
            }
        );
    }

    #[test]
    fn parse_list_with_short_flag() {
        let cli = Cli::parse_from(["lazywt", "list", "--short"]);
        match cli.command {
            Commands::List { short, json, no_path, no_color } => {
                assert!(short);
                assert!(!json);
                assert!(!no_path);
                assert!(!no_color);
            }
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn parse_list_with_json_flag() {
        let cli = Cli::parse_from(["lazywt", "list", "--json"]);
        match cli.command {
            Commands::List { json, .. } => assert!(json),
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn parse_list_with_no_path_flag() {
        let cli = Cli::parse_from(["lazywt", "list", "--no-path"]);
        match cli.command {
            Commands::List { no_path, .. } => assert!(no_path),
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn parse_list_with_no_color_flag() {
        let cli = Cli::parse_from(["lazywt", "list", "--no-color"]);
        match cli.command {
            Commands::List { no_color, .. } => assert!(no_color),
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn parse_config_command() {
        let cli = Cli::parse_from(["lazywt", "config"]);
        assert_eq!(
            cli.command,
            Commands::Config {
                global: false,
                repo: false
            }
        );
    }

    #[test]
    fn parse_config_global() {
        let cli = Cli::parse_from(["lazywt", "config", "--global"]);
        assert_eq!(
            cli.command,
            Commands::Config {
                global: true,
                repo: false
            }
        );
    }

    #[test]
    fn parse_config_repo() {
        let cli = Cli::parse_from(["lazywt", "config", "--repo"]);
        assert_eq!(
            cli.command,
            Commands::Config {
                global: false,
                repo: true
            }
        );
    }

    #[test]
    fn parse_pr_number_only() {
        let cli = Cli::parse_from(["lazywt", "pr", "42"]);
        assert_eq!(
            cli.command,
            Commands::Pr {
                target: "42".to_string(),
                name: None,
            }
        );
    }

    #[test]
    fn parse_pr_with_name() {
        let cli = Cli::parse_from(["lazywt", "pr", "42", "review"]);
        assert_eq!(
            cli.command,
            Commands::Pr {
                target: "42".to_string(),
                name: Some("review".to_string()),
            }
        );
    }

    #[test]
    fn parse_pr_url() {
        let cli =
            Cli::parse_from(["lazywt", "pr", "https://github.com/owner/repo/pull/123"]);
        assert_eq!(
            cli.command,
            Commands::Pr {
                target: "https://github.com/owner/repo/pull/123".to_string(),
                name: None,
            }
        );
    }

    #[test]
    fn parse_list_with_all_flags() {
        let cli = Cli::parse_from(["lazywt", "list", "--short", "--no-path", "--no-color"]);
        match cli.command {
            Commands::List { short, no_path, no_color, .. } => {
                assert!(short);
                assert!(no_path);
                assert!(no_color);
            }
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn parse_prune_no_flags() {
        let cli = Cli::parse_from(["lazywt", "prune"]);
        assert_eq!(
            cli.command,
            Commands::Prune {
                force: false,
                yes: false,
            }
        );
    }

    #[test]
    fn parse_prune_force_yes() {
        let cli = Cli::parse_from(["lazywt", "prune", "--force", "--yes"]);
        assert_eq!(
            cli.command,
            Commands::Prune {
                force: true,
                yes: true,
            }
        );
    }

    #[test]
    fn parse_sync_no_flags() {
        let cli = Cli::parse_from(["lazywt", "sync"]);
        assert_eq!(cli.command, Commands::Sync { all: false });
    }

    #[test]
    fn parse_sync_all() {
        let cli = Cli::parse_from(["lazywt", "sync", "--all"]);
        assert_eq!(cli.command, Commands::Sync { all: true });
    }

    #[test]
    fn parse_list_no_flags_defaults_to_false() {
        let cli = Cli::parse_from(["lazywt", "list"]);
        match cli.command {
            Commands::List { short, json, no_path, no_color } => {
                assert!(!short);
                assert!(!json);
                assert!(!no_path);
                assert!(!no_color);
            }
            _ => panic!("Expected List command"),
        }
    }

    #[test]
    fn parse_setup_command() {
        let cli = Cli::parse_from(["lazywt", "setup"]);
        assert_eq!(
            cli.command,
            Commands::Setup {
                dry_run: false,
                yes: false,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_setup_dry_run_yes() {
        let cli = Cli::parse_from(["lazywt", "setup", "--dry-run", "--yes"]);
        assert_eq!(
            cli.command,
            Commands::Setup {
                dry_run: true,
                yes: true,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_setup_short_yes() {
        let cli = Cli::parse_from(["lazywt", "setup", "-y"]);
        assert_eq!(
            cli.command,
            Commands::Setup {
                dry_run: false,
                yes: true,
                bare_dir: None,
            }
        );
    }

    #[test]
    fn parse_setup_with_bare_dir() {
        let cli = Cli::parse_from(["lazywt", "setup", "--bare-dir", ".repo", "--yes"]);
        assert_eq!(
            cli.command,
            Commands::Setup {
                dry_run: false,
                yes: true,
                bare_dir: Some(".repo".to_string()),
            }
        );
    }
}
