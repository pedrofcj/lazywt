use anyhow::{bail, Result};
use crate::config::Config;
use crate::error::LazywtError;
use crate::git;
use crate::layout::{self, LayoutType};
use crate::output;
use std::path::Path;

/// Extract the repository name from a URL.
///
/// Handles HTTPS, SSH, and local paths:
/// - `https://github.com/user/repo.git` -> `repo`
/// - `git@github.com:user/repo.git` -> `repo`
/// - `/path/to/repo.git` -> `repo`
/// - `https://github.com/user/repo` -> `repo`
fn extract_repo_name(url: &str) -> &str {
    let url = url.strip_suffix('/').unwrap_or(url);
    let name = url.strip_suffix(".git").unwrap_or(url);
    name.rsplit(['/', ':']).next().unwrap_or(name)
}

/// Clone a repository as a bare repo with modern layout and set up worktree structure.
///
/// Sequence (matching PowerShell New-BareRepository):
/// 1. Extract repo name or use custom dirname
/// 2. Check target directory doesn't exist
/// 3. git clone --bare into <destination>/.git
/// 4. Configure core.bare, wt.layout, fetch refspec
/// 5. Fetch all branches
/// 6. Detect default branch
/// 7. Create worktree for default branch (respecting worktree_folder config)
/// 8. Set upstream tracking and pull latest
pub fn run(url: &str, dirname: Option<&str>, bare_dir: Option<&str>, config: &Config) -> Result<()> {
    // Step 1: Determine destination directory name
    let repo_name = dirname.unwrap_or_else(|| extract_repo_name(url));

    // Step 2: Compute destination path
    let cwd = std::env::current_dir()?;
    let destination = cwd.join(repo_name);

    // Step 3: Check if directory already exists (D-12)
    if destination.exists() {
        bail!(LazywtError::DirectoryExists(
            destination.display().to_string()
        ));
    }

    // Resolve effective bare dir: flag -> config -> ".git" default (per D-02)
    let effective_bare_dir = bare_dir
        .map(|s| s.to_string())
        .or_else(|| config.bare_dir.clone())
        .unwrap_or_else(|| ".git".to_string());

    let bare_repo_path = destination.join(&effective_bare_dir);

    // Step 4: Print header
    output::header(&format!("Cloning {}", url));
    output::info(&format!("  Destination: {}", destination.display()));
    if effective_bare_dir != ".git" {
        output::info(&format!("  Bare repo: {}", effective_bare_dir));
    }

    // Step 5: Create destination directory
    std::fs::create_dir_all(&destination)?;

    // From this point, clean up on failure
    let result = clone_inner(url, &destination, &bare_repo_path, config);

    if result.is_err() {
        // Cleanup guard: remove partially created directory
        let _ = std::fs::remove_dir_all(&destination);
    }

    result
}

/// Inner clone logic that can fail -- caller handles cleanup.
fn clone_inner(
    url: &str,
    destination: &Path,
    bare_repo_path: &Path,
    config: &Config,
) -> Result<()> {
    // Step 6: Clone bare repository
    output::progress_start("Cloning repository");

    // Use git_in_cwd with absolute path since git clone creates the directory
    let bare_path_str = bare_repo_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path contains invalid UTF-8"))?;
    git::git_in_cwd(&["clone", "--bare", url, bare_path_str])?;
    output::progress_complete("Repository cloned", output::Status::Success);

    // Step 7: Ensure core.bare is explicitly set
    git::git(bare_repo_path, &["config", "core.bare", "true"])?;

    // Step 8: Set modern layout marker
    git::config::set_config(bare_repo_path, "wt.layout", "modern")?;

    // Step 8b: Store bare dir name in git config for layout auto-detection (D-04, D-07)
    let effective_bare_dir_name = bare_repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".git");
    git::config::set_config(bare_repo_path, "wt.baredir", effective_bare_dir_name)?;

    // Step 9: Fix fetch refspec (same logic as fix-fetch command)
    git::config::set_config(
        bare_repo_path,
        "remote.origin.fetch",
        "+refs/heads/*:refs/remotes/origin/*",
    )?;

    // Step 10: Fetch all branches
    output::progress_start("Fetching branches");
    match git::git(bare_repo_path, &["fetch", "--all"]) {
        Ok(_) => {
            output::progress_complete("Branches fetched", output::Status::Success);
        }
        Err(_) => {
            output::progress_complete("Fetch failed (continuing anyway)", output::Status::Warning);
        }
    }

    // Step 11: Detect default branch (allow_network=true since we just cloned)
    let default_branch = git::branch::detect_default_branch(bare_repo_path, true)?
        .ok_or(LazywtError::NoDefaultBranch)?;

    // Step 12: Compute worktree parent using layout context
    let ctx = layout::build_context(bare_repo_path, LayoutType::Modern, config);

    // Step 13: Create worktree parent directory if needed
    std::fs::create_dir_all(&ctx.worktree_parent)?;

    // Step 14: Create default branch worktree
    let main_wt_path = ctx.worktree_parent.join(&default_branch);
    let main_wt_str = main_wt_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("worktree path contains invalid UTF-8"))?;

    output::progress_start(&format!("Creating {} worktree", &default_branch));
    git::git(
        bare_repo_path,
        &["worktree", "add", main_wt_str, &default_branch],
    )?;
    output::progress_complete(
        &format!("{} worktree created", &default_branch),
        output::Status::Success,
    );

    // Step 15: Set upstream tracking (non-fatal)
    let _ = git::branch::set_upstream(&main_wt_path, &default_branch);

    // Step 16: Pull latest (non-fatal -- warn on failure per Pitfall 2)
    match git::git(&main_wt_path, &["pull"]) {
        Ok(_) => {}
        Err(_) => {
            output::warning("  Pull failed (you may need to pull manually)");
        }
    }

    // Step 17: Print success summary
    output::success(&format!("Repository ready at {}", destination.display()));
    output::info(&format!("  Main worktree: {}", main_wt_path.display()));

    // Step 18: Prompt or auto-navigate to the main worktree
    crate::navigate::maybe_navigate(&main_wt_path, config);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_repo_name_https_url() {
        assert_eq!(extract_repo_name("https://github.com/user/repo.git"), "repo");
    }

    #[test]
    fn extract_repo_name_ssh_url() {
        assert_eq!(extract_repo_name("git@github.com:user/repo.git"), "repo");
    }

    #[test]
    fn extract_repo_name_no_git_suffix() {
        assert_eq!(extract_repo_name("https://github.com/user/repo"), "repo");
    }

    #[test]
    fn extract_repo_name_local_path() {
        assert_eq!(extract_repo_name("/path/to/repo.git"), "repo");
    }

    #[test]
    fn extract_repo_name_trailing_slash() {
        assert_eq!(extract_repo_name("https://github.com/user/repo/"), "repo");
    }
}
