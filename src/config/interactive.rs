use anyhow::Result;
use dialoguer::{Confirm, Input, Select};

use super::schema::{GlobalConfig, RepoConfig};
use super::Config;

/// Interactively edit global config settings using dialoguer widgets.
/// Pre-fills prompts with current effective values (D-45, D-46).
pub fn edit_global_config(current: &GlobalConfig) -> Result<GlobalConfig> {
    let command_name: String = Input::new()
        .with_prompt("Command name")
        .default(current.command_name.clone())
        .interact_text()?;

    let worktree_folder_input: String = Input::new()
        .with_prompt("Worktree folder (empty for default)")
        .default(current.worktree_folder.clone().unwrap_or_default())
        .allow_empty(true)
        .interact_text()?;
    let worktree_folder = if worktree_folder_input.is_empty() {
        None
    } else {
        Some(worktree_folder_input)
    };

    let auto_update = Confirm::new()
        .with_prompt("Auto-check for updates?")
        .default(current.auto_update)
        .interact()?;

    let auto_navigate = Confirm::new()
        .with_prompt("Auto-navigate to new worktrees?")
        .default(current.auto_navigate)
        .interact()?;

    let branch_prefix: String = Input::new()
        .with_prompt("Branch prefix (empty for none)")
        .default(current.branch_prefix.clone())
        .allow_empty(true)
        .interact_text()?;

    // copy_files and worktree_location are per-repo settings; preserve current values
    let copy_files = current.copy_files.clone();
    let worktree_location = current.worktree_location.clone();

    Ok(GlobalConfig {
        command_name,
        worktree_folder,
        bare_dir: current.bare_dir.clone(),
        auto_update,
        auto_navigate,
        branch_prefix,
        copy_files,
        worktree_location,
    })
}

/// Interactively edit per-repo config settings using dialoguer widgets.
/// Pre-fills prompts with effective values (D-45). Only writes fields
/// that differ from the global default (D-39: None = inherit).
pub fn edit_repo_config(
    current_effective: &Config,
    existing_repo: Option<&RepoConfig>,
) -> Result<RepoConfig> {
    let global = super::schema::load_global();

    let branch_prefix: String = Input::new()
        .with_prompt("Branch prefix (empty for none)")
        .default(current_effective.branch_prefix.clone())
        .allow_empty(true)
        .interact_text()?;

    let copy_files_input: String = Input::new()
        .with_prompt("Files to copy (comma-separated, empty for none)")
        .default(current_effective.copy_files.join(", "))
        .allow_empty(true)
        .interact_text()?;
    let copy_files_parsed: Vec<String> = if copy_files_input.is_empty() {
        Vec::new()
    } else {
        copy_files_input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };

    let location_options = vec!["inside", "sibling"];
    let default_location_idx = location_options
        .iter()
        .position(|&s| s == current_effective.worktree_location)
        .unwrap_or(0);
    let location_idx = Select::new()
        .with_prompt("Worktree location")
        .items(&location_options)
        .default(default_location_idx)
        .interact()?;
    let worktree_location = location_options[location_idx].to_string();

    let command_name: String = Input::new()
        .with_prompt("Command name")
        .default(current_effective.command_name.clone())
        .interact_text()?;

    let worktree_folder_input: String = Input::new()
        .with_prompt("Worktree folder (empty for default)")
        .default(
            current_effective
                .worktree_folder
                .clone()
                .unwrap_or_default(),
        )
        .allow_empty(true)
        .interact_text()?;
    let worktree_folder_parsed = if worktree_folder_input.is_empty() {
        None
    } else {
        Some(worktree_folder_input)
    };

    let auto_update = Confirm::new()
        .with_prompt("Auto-check for updates?")
        .default(current_effective.auto_update)
        .interact()?;

    let auto_navigate = Confirm::new()
        .with_prompt("Auto-navigate to new worktrees?")
        .default(current_effective.auto_navigate)
        .interact()?;

    // Compare each value to the global default. If same as global AND
    // the existing repo config had None, keep it as None (inherit).
    let repo_command_name = if command_name != global.command_name
        || existing_repo.and_then(|r| r.command_name.as_ref()).is_some()
            && command_name != global.command_name
    {
        Some(command_name)
    } else {
        None
    };

    let repo_worktree_folder = if worktree_folder_parsed != global.worktree_folder {
        worktree_folder_parsed
    } else if existing_repo
        .and_then(|r| r.worktree_folder.as_ref())
        .is_some()
    {
        // User kept the same value but repo previously had an override
        worktree_folder_parsed
    } else {
        None
    };

    let repo_auto_update = if auto_update != global.auto_update {
        Some(auto_update)
    } else {
        None
    };

    let repo_auto_navigate = if auto_navigate != global.auto_navigate {
        Some(auto_navigate)
    } else {
        None
    };

    let repo_branch_prefix = if branch_prefix != global.branch_prefix {
        Some(branch_prefix)
    } else {
        None
    };

    let repo_copy_files = if copy_files_parsed != global.copy_files {
        Some(copy_files_parsed)
    } else {
        None
    };

    let repo_worktree_location = if worktree_location != global.worktree_location {
        Some(worktree_location)
    } else {
        None
    };

    Ok(RepoConfig {
        command_name: repo_command_name,
        worktree_folder: repo_worktree_folder,
        bare_dir: None,
        auto_update: repo_auto_update,
        auto_navigate: repo_auto_navigate,
        branch_prefix: repo_branch_prefix,
        copy_files: repo_copy_files,
        worktree_location: repo_worktree_location,
    })
}
