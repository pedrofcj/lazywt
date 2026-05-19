use std::io::IsTerminal;

use anyhow::Result;
use dialoguer::Select;

use crate::config::interactive;
use crate::config::repo_config_path;
use crate::config::schema;
use crate::config::Config;
use crate::output;

/// Run the interactive config editor.
///
/// Determines scope (global or per-repo) based on flags and prompts,
/// then walks through settings with pre-filled values (D-43, D-44, D-45, D-46).
pub fn run(
    global_flag: bool,
    repo_flag: bool,
    config: &Config,
    project_dir: Option<&std::path::Path>,
) -> Result<()> {
    // TTY guard (Pitfall 7, D-20)
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("Config editor requires a terminal");
    }

    // Determine scope
    let scope = if global_flag {
        Scope::Global
    } else if repo_flag {
        if project_dir.is_none() {
            anyhow::bail!(
                "Not inside a git repository. Run from a worktree or use --global."
            );
        }
        Scope::Repo
    } else {
        // No flag -- prompt for scope (D-43, D-46)
        if project_dir.is_some() {
            let items = vec!["Global", "Per-repo"];
            let idx = Select::new()
                .with_prompt("Which config scope?")
                .items(&items)
                .default(0)
                .interact()?;
            if idx == 0 {
                Scope::Global
            } else {
                Scope::Repo
            }
        } else {
            // Not in a repo -- global is the only option
            Scope::Global
        }
    };

    match scope {
        Scope::Global => {
            let current = schema::load_global();
            let edited = interactive::edit_global_config(&current)?;
            schema::write_global(&edited)?;
            output::success("Global config saved");
        }
        Scope::Repo => {
            let dir = project_dir.expect("repo scope requires project_dir");
            let effective = config.with_repo(dir);
            let existing = schema::load_repo(dir);
            let edited = interactive::edit_repo_config(&effective, existing.as_ref())?;
            schema::write_repo(dir, &edited)?;
            output::success(&format!(
                "Repo config saved to {}",
                dunce::simplified(&repo_config_path(dir)).display()
            ));
        }
    }

    Ok(())
}

enum Scope {
    Global,
    Repo,
}
