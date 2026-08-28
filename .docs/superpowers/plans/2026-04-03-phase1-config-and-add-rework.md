# Phase 1: Config System + `add` Rework — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the INI config system with a two-tier JSON config (global + per-repo), rework the `add` command to decouple worktree name from branch name via config-driven prefix, and add an interactive `config` command.

**Architecture:** The new `Config` struct uses `serde_json` for deserialization. Global config lives at `~/.config/junktree/config.json`, per-repo at `.junktree.json` in the repo root. Resolution: env vars > per-repo > global > hardcoded defaults. The `add` command drops its positional `type` argument and gets a `--branch`/`-b` flag instead. A new `config` command provides interactive editing.

**Tech Stack:** Rust, serde + serde_json (already in Cargo.toml), clap derive, dialoguer (already in Cargo.toml for interactive prompts).

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Rewrite | `src/config.rs` | JSON config loading, two-tier merge, env overrides |
| Modify | `src/cli.rs` | Update `Add` variant (drop `type`, add `--branch`), add `Config` subcommand |
| Modify | `src/commands/add.rs` | Use `branch_prefix` from config, support `--branch` flag |
| Create | `src/commands/config_cmd.rs` | Interactive config editor |
| Modify | `src/commands/mod.rs` | Register `config_cmd` module |
| Modify | `src/main.rs` | Wire up `Config` command dispatch |
| Modify | `src/layout.rs` | Accept new Config struct (field names changed) |
| Modify | `src/navigate.rs` | Accept new Config struct |
| Modify | `src/commands/alias.rs` | Accept new Config struct |
| Modify | `src/commands/init_shell.rs` | Accept new Config struct |
| Modify | `src/commands/list/mod.rs` | Accept new Config struct |
| Modify | `src/commands/remove.rs` | Accept new Config struct |
| Modify | `src/commands/remove_all.rs` | Accept new Config struct |
| Modify | `src/commands/switch.rs` | Accept new Config struct |
| Modify | `src/commands/clone.rs` | Accept new Config struct |
| Modify | `src/commands/migrate.rs` | Accept new Config struct |
| Modify | `tests/test_add.rs` | Update tests for new add behavior |

---

### Task 1: Rewrite Config struct with JSON + serde

**Files:**
- Rewrite: `src/config.rs`

- [ ] **Step 1: Write failing test for JSON parsing**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_full_config() {
        let json = r#"{
            "branch_prefix": "feature/",
            "auto_update": false,
            "auto_navigate": true,
            "command_name": "tree"
        }"#;
        let config = Config::parse_json(json).unwrap();
        assert_eq!(config.branch_prefix, "feature/");
        assert!(!config.auto_update);
        assert!(config.auto_navigate);
        assert_eq!(config.command_name, "tree");
    }

    #[test]
    fn parse_json_empty_object_uses_defaults() {
        let config = Config::parse_json("{}").unwrap();
        assert_eq!(config.branch_prefix, "");
        assert!(config.auto_update);
        assert!(!config.auto_navigate);
        assert_eq!(config.command_name, "wt");
    }

    #[test]
    fn parse_json_partial_config() {
        let json = r#"{ "branch_prefix": "bug/" }"#;
        let config = Config::parse_json(json).unwrap();
        assert_eq!(config.branch_prefix, "bug/");
        assert_eq!(config.command_name, "wt"); // default
    }

    #[test]
    fn parse_json_unknown_keys_ignored() {
        let json = r#"{ "unknown_key": "value", "command_name": "wt2" }"#;
        let config = Config::parse_json(json).unwrap();
        assert_eq!(config.command_name, "wt2");
    }

    #[test]
    fn parse_json_invalid_json_returns_error() {
        let result = Config::parse_json("not json");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib config::tests -- --nocapture`
Expected: FAIL — `parse_json` method doesn't exist, `branch_prefix` field doesn't exist.

- [ ] **Step 3: Implement new Config struct**

Replace the entire `Config` struct and its impl with:

```rust
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

/// Configuration for junktree, loaded from JSON files and environment variables.
///
/// Resolution order (highest priority wins):
/// 1. Environment variables (WT_*)
/// 2. Per-repo `.junktree.json`
/// 3. Global `~/.config/junktree/config.json`
/// 4. Hardcoded defaults
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Prefix for branch names when creating worktrees (default: "" empty)
    pub branch_prefix: String,
    /// Whether to check for updates automatically (default: true)
    pub auto_update: bool,
    /// Whether to auto-navigate to new worktrees without prompting (default: false)
    pub auto_navigate: bool,
    /// The command name used in user-facing messages (default: "wt")
    pub command_name: String,
    /// Glob patterns for files to copy into new worktrees (per-repo only)
    pub copy_files: Vec<String>,
    /// Where worktrees are stored for non-bare repos: "inside" or "sibling"
    pub worktree_location: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            branch_prefix: String::new(),
            auto_update: true,
            auto_navigate: false,
            command_name: "wt".to_string(),
            copy_files: Vec::new(),
            worktree_location: "inside".to_string(),
        }
    }
}

impl Config {
    /// Path to the global config file.
    pub fn global_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("junktree").join("config.json"))
    }

    /// Load configuration from global file, per-repo file, and env overrides.
    /// `repo_root` is the bare repo root or regular repo root (if known).
    pub fn load(repo_root: Option<&Path>) -> Self {
        let mut config = Self::default();

        // Layer 1: Global config
        if let Some(global_path) = Self::global_config_path() {
            if let Ok(content) = fs::read_to_string(&global_path) {
                if let Ok(parsed) = Self::parse_json(&content) {
                    config = parsed;
                }
            }
        }

        // Layer 2: Per-repo config
        if let Some(root) = repo_root {
            let repo_config_path = root.join(".junktree.json");
            if let Ok(content) = fs::read_to_string(&repo_config_path) {
                if let Ok(repo_config) = Self::parse_json(&content) {
                    config.merge_from(&repo_config);
                }
            }
        }

        // Layer 3: Environment variable overrides (highest priority)
        config.apply_env();

        config
    }

    /// Parse a JSON string into a Config.
    pub fn parse_json(json: &str) -> Result<Self> {
        let config: Config = serde_json::from_str(json)?;
        Ok(config)
    }

    /// Merge non-default values from another config into this one.
    /// Only overrides fields that were explicitly set (non-default) in `other`.
    fn merge_from(&mut self, other: &Config) {
        let defaults = Config::default();
        if other.branch_prefix != defaults.branch_prefix {
            self.branch_prefix = other.branch_prefix.clone();
        }
        if other.auto_update != defaults.auto_update {
            self.auto_update = other.auto_update;
        }
        if other.auto_navigate != defaults.auto_navigate {
            self.auto_navigate = other.auto_navigate;
        }
        if other.command_name != defaults.command_name {
            self.command_name = other.command_name.clone();
        }
        if other.copy_files != defaults.copy_files {
            self.copy_files = other.copy_files.clone();
        }
        if other.worktree_location != defaults.worktree_location {
            self.worktree_location = other.worktree_location.clone();
        }
    }

    /// Apply environment variable overrides.
    fn apply_env(&mut self) {
        if let Ok(val) = env::var("WT_RENAME") {
            self.command_name = val;
        }
        if let Ok(val) = env::var("WT_AUTO_UPDATE") {
            self.auto_update = val != "false";
        }
        if let Ok(val) = env::var("WT_AUTO_NAVIGATE") {
            self.auto_navigate = val == "true";
        }
        if let Ok(val) = env::var("WT_BRANCH_PREFIX") {
            self.branch_prefix = val;
        }
    }

    /// Serialize this config to a pretty-printed JSON string.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib config::tests -- --nocapture`
Expected: All 5 new tests PASS.

- [ ] **Step 5: Write tests for merge behavior**

Add to the test module in `src/config.rs`:

```rust
#[test]
fn merge_repo_overrides_global() {
    let global = Config::parse_json(r#"{ "branch_prefix": "feature/", "command_name": "wt" }"#).unwrap();
    let repo = Config::parse_json(r#"{ "branch_prefix": "bug/" }"#).unwrap();
    let mut merged = global;
    merged.merge_from(&repo);
    assert_eq!(merged.branch_prefix, "bug/");
    assert_eq!(merged.command_name, "wt"); // not overridden
}

#[test]
fn merge_does_not_override_with_defaults() {
    let global = Config::parse_json(r#"{ "branch_prefix": "feature/" }"#).unwrap();
    let repo = Config::parse_json("{}").unwrap();
    let mut merged = global;
    merged.merge_from(&repo);
    assert_eq!(merged.branch_prefix, "feature/"); // unchanged
}

#[test]
fn env_override_branch_prefix() {
    let _lock = ENV_MUTEX.lock().unwrap();
    env::remove_var("WT_RENAME");
    env::remove_var("WT_AUTO_UPDATE");
    env::remove_var("WT_AUTO_NAVIGATE");
    env::set_var("WT_BRANCH_PREFIX", "hotfix/");
    let mut config = Config::default();
    config.apply_env();
    assert_eq!(config.branch_prefix, "hotfix/");
    env::remove_var("WT_BRANCH_PREFIX");
}

#[test]
fn env_override_command_name() {
    let _lock = ENV_MUTEX.lock().unwrap();
    env::remove_var("WT_AUTO_UPDATE");
    env::remove_var("WT_AUTO_NAVIGATE");
    env::remove_var("WT_BRANCH_PREFIX");
    env::set_var("WT_RENAME", "tree");
    let mut config = Config::default();
    config.apply_env();
    assert_eq!(config.command_name, "tree");
    env::remove_var("WT_RENAME");
}

#[test]
fn load_with_no_files_returns_defaults() {
    let _lock = ENV_MUTEX.lock().unwrap();
    env::remove_var("WT_RENAME");
    env::remove_var("WT_AUTO_UPDATE");
    env::remove_var("WT_AUTO_NAVIGATE");
    env::remove_var("WT_BRANCH_PREFIX");
    let config = Config::load(Some(Path::new("/nonexistent/path")));
    assert_eq!(config.command_name, "wt");
    assert_eq!(config.branch_prefix, "");
}
```

- [ ] **Step 6: Run all config tests**

Run: `cargo test --lib config::tests -- --nocapture`
Expected: All PASS.

- [ ] **Step 7: Commit**

```bash
git add src/config.rs
git commit -m "feat: replace INI config with JSON two-tier config system"
```

---

### Task 2: Update layout.rs to work with new Config

**Files:**
- Modify: `src/layout.rs`

The `Config` struct no longer has `worktree_folder: Option<String>`. The layout module needs to derive worktree folder from the config differently. For bare repos, keep the existing behavior (classic=`trees`, modern=root-level). The `worktree_location` field is only used for non-bare repos (Phase 4).

- [ ] **Step 1: Update build_context to use new Config fields**

In `src/layout.rs`, the `build_context` function accesses `config.worktree_folder`. Replace that with the equivalent logic: for bare repos, `worktree_folder` is always derived from layout type (no config override for now — the old `worktree_folder` config key is removed).

```rust
// Replace the worktree_folder computation block:
let worktree_folder = match layout {
    LayoutType::Classic => "trees".to_string(),
    LayoutType::Modern => String::new(),
};
```

- [ ] **Step 2: Update layout tests**

Remove `config_with_folder` helper and tests that relied on `worktree_folder: Option<String>`. Update `default_config()` to match the new `Config` struct.

```rust
fn default_config() -> Config {
    Config::default()
}
```

Remove tests: `config_worktree_folder_overrides_default`, `empty_worktree_folder_means_project_root`.

- [ ] **Step 3: Run layout tests**

Run: `cargo test --lib layout::tests -- --nocapture`
Expected: All remaining tests PASS.

- [ ] **Step 4: Commit**

```bash
git add src/layout.rs
git commit -m "refactor: update layout to use new JSON config struct"
```

---

### Task 3: Update CLI — rework `Add`, add `Config` subcommand

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Update Add variant in Commands enum**

Replace the existing `Add` variant:

```rust
/// Create a new worktree
Add {
    /// Worktree name (directory name for the worktree)
    name: String,
    /// Full branch name (overrides branch_prefix from config)
    #[arg(short = 'b', long = "branch")]
    branch: Option<String>,
    /// Base worktree to branch from
    #[arg(long)]
    from: Option<String>,
},
```

- [ ] **Step 2: Add Config variant to Commands enum**

```rust
/// Configure junktree settings interactively
Config {
    /// Edit global config only
    #[arg(long)]
    global: bool,
    /// Edit per-repo config only
    #[arg(long)]
    repo: bool,
},
```

- [ ] **Step 3: Update CLI tests for add**

Replace all `parse_add_*` tests to reflect the new argument structure:

```rust
#[test]
fn parse_add_command() {
    let cli = Cli::parse_from(["junktree", "add", "auth"]);
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
fn parse_add_with_branch() {
    let cli = Cli::parse_from(["junktree", "add", "fixing-naming", "-b", "bug/TTR-3022/fix-naming"]);
    assert_eq!(
        cli.command,
        Commands::Add {
            name: "fixing-naming".to_string(),
            branch: Some("bug/TTR-3022/fix-naming".to_string()),
            from: None,
        }
    );
}

#[test]
fn parse_add_with_long_branch() {
    let cli = Cli::parse_from(["junktree", "add", "fix", "--branch", "hotfix/v2"]);
    assert_eq!(
        cli.command,
        Commands::Add {
            name: "fix".to_string(),
            branch: Some("hotfix/v2".to_string()),
            from: None,
        }
    );
}

#[test]
fn parse_add_with_from() {
    let cli = Cli::parse_from(["junktree", "add", "auth", "--from", "dev"]);
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
fn parse_add_with_branch_and_from() {
    let cli = Cli::parse_from(["junktree", "add", "auth", "-b", "feature/auth", "--from", "dev"]);
    assert_eq!(
        cli.command,
        Commands::Add {
            name: "auth".to_string(),
            branch: Some("feature/auth".to_string()),
            from: Some("dev".to_string()),
        }
    );
}
```

Remove the old `parse_add_with_type` test.

- [ ] **Step 4: Add CLI test for config command**

```rust
#[test]
fn parse_config_command() {
    let cli = Cli::parse_from(["junktree", "config"]);
    assert_eq!(cli.command, Commands::Config { global: false, repo: false });
}

#[test]
fn parse_config_global() {
    let cli = Cli::parse_from(["junktree", "config", "--global"]);
    assert_eq!(cli.command, Commands::Config { global: true, repo: false });
}

#[test]
fn parse_config_repo() {
    let cli = Cli::parse_from(["junktree", "config", "--repo"]);
    assert_eq!(cli.command, Commands::Config { global: false, repo: true });
}
```

- [ ] **Step 5: Run CLI tests**

Run: `cargo test --lib cli::tests -- --nocapture`
Expected: All PASS.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs
git commit -m "feat: rework add CLI args (drop type, add --branch), add config command"
```

---

### Task 4: Rework add command implementation

**Files:**
- Modify: `src/commands/add.rs`

- [ ] **Step 1: Write unit test for branch name resolution**

Add a helper function `resolve_branch_name` and test it:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_branch_with_explicit_branch_flag() {
        let result = resolve_branch_name("fixing", Some("bug/TTR-3022/fix"), "feature/");
        assert_eq!(result, "bug/TTR-3022/fix");
    }

    #[test]
    fn resolve_branch_with_prefix() {
        let result = resolve_branch_name("auth", None, "feature/");
        assert_eq!(result, "feature/auth");
    }

    #[test]
    fn resolve_branch_without_prefix() {
        let result = resolve_branch_name("auth", None, "");
        assert_eq!(result, "auth");
    }

    // ... keep existing validate_worktree_name tests
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib commands::add::tests -- --nocapture`
Expected: FAIL — `resolve_branch_name` doesn't exist.

- [ ] **Step 3: Implement resolve_branch_name and update run()**

Add the helper:

```rust
/// Resolve the git branch name for a new worktree.
/// If `explicit_branch` is Some, use it directly (ignoring prefix).
/// Otherwise, prepend `branch_prefix` to the worktree name.
fn resolve_branch_name(worktree_name: &str, explicit_branch: Option<&str>, branch_prefix: &str) -> String {
    if let Some(branch) = explicit_branch {
        return branch.to_string();
    }
    if branch_prefix.is_empty() {
        worktree_name.to_string()
    } else {
        format!("{}{}", branch_prefix, worktree_name)
    }
}
```

Update the `run` function signature:

```rust
pub fn run(
    ctx: &ProjectContext,
    config: &Config,
    name: &str,
    branch: Option<&str>,
    from: Option<&str>,
) -> Result<()> {
```

Replace the branch name computation (old step 4):

```rust
// 4. Compute branch name
let branch_name = resolve_branch_name(name, branch, &config.branch_prefix);
```

Remove the old `branch_type` parameter and `branch_type.unwrap_or("feature")` logic.

- [ ] **Step 4: Run tests**

Run: `cargo test --lib commands::add::tests -- --nocapture`
Expected: All PASS.

- [ ] **Step 5: Commit**

```bash
git add src/commands/add.rs
git commit -m "feat: rework add command — branch prefix from config, --branch flag"
```

---

### Task 5: Create config command

**Files:**
- Create: `src/commands/config_cmd.rs`
- Modify: `src/commands/mod.rs`

- [ ] **Step 1: Write the config command module**

```rust
use std::fs;
use std::io::IsTerminal;

use anyhow::{Result, bail};
use dialoguer::{Input, Select};

use crate::config::Config;
use crate::layout::ProjectContext;
use crate::output;

/// Which config scope to edit.
enum Scope {
    Global,
    Repo,
}

/// Run the interactive config editor.
/// - No flags: prompt for scope.
/// - --global: edit global config only.
/// - --repo: edit per-repo config only.
pub fn run(ctx: Option<&ProjectContext>, global: bool, repo: bool) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("config command requires an interactive terminal");
    }

    let scope = if global {
        Scope::Global
    } else if repo {
        Scope::Repo
    } else {
        let items = vec!["Global (~/.config/junktree/config.json)", "Repo (.junktree.json)"];
        let selection = Select::new()
            .with_prompt("Which config to edit?")
            .items(&items)
            .default(0)
            .interact()?;
        if selection == 0 { Scope::Global } else { Scope::Repo }
    };

    match scope {
        Scope::Global => edit_global()?,
        Scope::Repo => {
            let ctx = ctx.ok_or_else(|| anyhow::anyhow!(
                "Not inside a git repository — cannot edit repo config"
            ))?;
            edit_repo(&ctx.project_root)?;
        }
    }

    Ok(())
}

fn edit_global() -> Result<()> {
    let path = Config::global_config_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    // Load existing or use defaults
    let existing = if path.exists() {
        let content = fs::read_to_string(&path)?;
        Config::parse_json(&content).unwrap_or_default()
    } else {
        Config::default()
    };

    let command_name: String = Input::new()
        .with_prompt("Command name")
        .default(existing.command_name)
        .interact_text()?;

    let branch_prefix: String = Input::new()
        .with_prompt("Branch prefix (e.g. 'feature/', empty for none)")
        .default(existing.branch_prefix)
        .allow_empty(true)
        .interact_text()?;

    let auto_update = Select::new()
        .with_prompt("Auto-check for updates?")
        .items(&["yes", "no"])
        .default(if existing.auto_update { 0 } else { 1 })
        .interact()? == 0;

    let auto_navigate = Select::new()
        .with_prompt("Auto-navigate to new worktrees?")
        .items(&["no", "yes"])
        .default(if existing.auto_navigate { 1 } else { 0 })
        .interact()? == 1;

    let config = Config {
        command_name,
        branch_prefix,
        auto_update,
        auto_navigate,
        ..Config::default()
    };

    // Write
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, config.to_json()?)?;
    output::success(&format!("Global config saved to {}", path.display()));

    Ok(())
}

fn edit_repo(repo_root: &std::path::Path) -> Result<()> {
    let path = repo_root.join(".junktree.json");

    let existing = if path.exists() {
        let content = fs::read_to_string(&path)?;
        Config::parse_json(&content).unwrap_or_default()
    } else {
        Config::default()
    };

    let branch_prefix: String = Input::new()
        .with_prompt("Branch prefix (e.g. 'bug/', empty for none)")
        .default(existing.branch_prefix)
        .allow_empty(true)
        .interact_text()?;

    let copy_files_str: String = Input::new()
        .with_prompt("Copy files patterns (comma-separated, e.g. '.env*,config/*.local')")
        .default(existing.copy_files.join(","))
        .allow_empty(true)
        .interact_text()?;

    let copy_files: Vec<String> = if copy_files_str.is_empty() {
        Vec::new()
    } else {
        copy_files_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };

    let worktree_location = Select::new()
        .with_prompt("Worktree location for non-bare repos")
        .items(&["inside (.wts/)", "sibling (-wts/)"])
        .default(if existing.worktree_location == "sibling" { 1 } else { 0 })
        .interact()?;
    let worktree_location = if worktree_location == 0 { "inside" } else { "sibling" }.to_string();

    // Build a repo-specific config (only repo-relevant fields)
    let config = Config {
        branch_prefix,
        copy_files,
        worktree_location,
        ..Config::default()
    };

    fs::write(&path, config.to_json()?)?;
    output::success(&format!("Repo config saved to {}", path.display()));

    Ok(())
}
```

- [ ] **Step 2: Register module in mod.rs**

Add to `src/commands/mod.rs`:

```rust
pub mod config_cmd;
```

- [ ] **Step 3: Run compilation check**

Run: `cargo check`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/commands/config_cmd.rs src/commands/mod.rs
git commit -m "feat: add interactive config command"
```

---

### Task 6: Wire everything together in main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update Config::load call**

The old `Config::load()` takes no args. The new one takes `Option<&Path>`. Update `main.rs` so the config is loaded in two phases: first without repo context (for commands that don't need a repo), then reloaded with repo context once the repo root is known.

```rust
fn main() {
    junktree::navigate::cleanup();

    // Phase 1: load config without repo context (for non-repo commands)
    let config = Config::load(None);

    let cli = Cli::parse_from({
        let mut args: Vec<String> = std::env::args().collect();
        if !args.is_empty() {
            args[0] = config.command_name.clone();
        }
        args
    });

    if let Err(e) = run(cli, config) {
        output::error(&format!("{}", e));
        if std::env::args().any(|a| a == "--verbose") {
            eprintln!("\nDetails: {:?}", e);
        }
        std::process::exit(1);
    }
}
```

- [ ] **Step 2: Update run_command to reload config with repo context**

```rust
fn run_command(cli: Cli, config: &Config) -> Result<()> {
    match &cli.command {
        // Commands that don't need a git repo
        Commands::Version => return commands::version::run(),
        Commands::Update => return commands::update::run(),
        Commands::Clone { url, dirname } => {
            return commands::clone::run(url, dirname.as_deref(), config);
        }
        Commands::Alias => return commands::alias::run(config),
        Commands::Init { shell } => return commands::init_shell::run(shell, config),
        Commands::Completions { ref shell } => {
            return commands::completions::run(shell);
        }
        // Config command: may or may not need repo context
        Commands::Config { global, repo } => {
            let ctx = git::repo::find_bare_repo_root()
                .ok()
                .and_then(|dir| layout::detect_layout(&dir, config).ok());
            return commands::config_cmd::run(ctx.as_ref(), *global, *repo);
        }
        _ => {}
    }

    // Resolve bare repo root
    let project_dir = git::repo::find_bare_repo_root()?;

    // Reload config with repo context for per-repo overrides
    let config = Config::load(Some(&project_dir));

    let ctx = layout::detect_layout(&project_dir, &config)?;

    match cli.command {
        Commands::List { short, json, no_path, no_color } => {
            commands::list::run(&ctx, &config, short, json, no_path, no_color)
        }
        Commands::Switch => commands::switch::run(&ctx, &config),
        Commands::FixFetch => commands::fix_fetch::run(&ctx),
        Commands::Add { name, branch, from } => {
            commands::add::run(&ctx, &config, &name, branch.as_deref(), from.as_deref())
        }
        Commands::Remove { name, yes } => commands::remove::run(&ctx, &config, &name, yes),
        Commands::RemoveAll { yes } => commands::remove_all::run(&ctx, &config, yes),
        Commands::Migrate { dry_run, yes } => commands::migrate::run(&ctx, &config, dry_run, yes),
        Commands::Version
        | Commands::Update
        | Commands::Clone { .. }
        | Commands::Alias
        | Commands::Init { .. }
        | Commands::Completions { .. }
        | Commands::Config { .. } => unreachable!(),
    }
}
```

- [ ] **Step 3: Compile and run all tests**

Run: `cargo test`
Expected: All tests PASS. Any compile errors from signature mismatches in other modules must be fixed.

- [ ] **Step 4: Fix any remaining signature mismatches**

Walk through each command module and ensure it compiles with the new `Config` struct. The key fields that changed:
- `command_name` — same name, same type. No change needed.
- `worktree_folder: Option<String>` — removed. Only `layout.rs` used it (already fixed in Task 2).
- `auto_update` — same. No change.
- `auto_navigate` — same. No change.

If `navigate.rs` or other modules accessed `config.worktree_folder`, remove those references.

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/commands/add.rs src/layout.rs src/config.rs
git commit -m "feat: wire new config system into main dispatch"
```

---

### Task 7: Update integration tests

**Files:**
- Modify: `tests/test_add.rs`

- [ ] **Step 1: Review and update add integration tests**

The integration tests in `tests/test_add.rs` likely pass arguments like `add name type`. Update them to use the new `add name` or `add name -b branch` syntax. Remove any tests that pass a positional branch type.

Run: `cargo test --test test_add -- --nocapture`

Fix any failing tests to match the new CLI signature.

- [ ] **Step 2: Run full integration test suite**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/
git commit -m "test: update integration tests for new add command and config"
```

---

### Task 8: Delete old INI config file reference

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Verify no remaining references to .wtconfig**

Run: `grep -r "wtconfig" src/ tests/`

If any references remain, update or remove them. The old `~/.wtconfig` is no longer read.

- [ ] **Step 2: Verify no remaining references to worktree_folder**

Run: `grep -r "worktree_folder" src/ tests/`

Clean up any remaining references.

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: remove all references to old INI config"
```
