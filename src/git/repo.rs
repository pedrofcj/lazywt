use std::path::PathBuf;
use anyhow::{Result, bail};

/// Find the repository root from the current directory.
/// Works for bare repos, worktrees of bare repos, and regular (non-bare) repos.
/// `bare_dir_hint` is the config's `bare_dir` value — used as fallback when git
/// can't find the repo (e.g., project root with a custom-named bare repo).
/// Returns (path, is_bare) tuple:
/// - For bare repos: (bare repo path, true)
/// - For non-bare repos: (repo toplevel, false)
pub fn find_repo_root(bare_dir_hint: Option<&str>) -> Result<(PathBuf, bool)> {
    // First try bare repo detection (existing logic)
    match find_bare_repo_root(bare_dir_hint) {
        Ok(path) => return Ok((path, true)),
        Err(_) => {}
    }

    // Fallback: try regular repo
    let toplevel = super::git_in_cwd(&["rev-parse", "--show-toplevel"])
        .map_err(|_| anyhow::anyhow!(
            "Not in a git repository.\n   \
             Run from inside a git repository or one of its worktrees."
        ))?;
    let toplevel = dunce::canonicalize(PathBuf::from(toplevel.trim()))
        .unwrap_or_else(|_| PathBuf::from(toplevel.trim()));
    Ok((toplevel, false))
}

/// Find the bare repository root from the current directory.
/// Works from inside a bare repo or any of its worktrees.
/// `bare_dir_hint` is used as fallback when git rev-parse fails — tries
/// CWD/<bare_dir_hint> as a potential bare repo location.
/// Returns error if not in a bare repo context.
pub fn find_bare_repo_root(bare_dir_hint: Option<&str>) -> Result<PathBuf> {
    // Step 1: Get git-dir from CWD
    match super::git_in_cwd(&["rev-parse", "--git-dir"]) {
        Ok(git_dir) => {
            let git_dir = dunce::canonicalize(PathBuf::from(git_dir.trim()))
                .unwrap_or_else(|_| PathBuf::from(git_dir.trim()));

            // Step 2: Check if directly in bare repo
            let is_bare = super::git_in_cwd(&["rev-parse", "--is-bare-repository"])?;
            if is_bare.trim() == "true" {
                return Ok(git_dir);
            }

            // Step 3: Check if in worktree of bare repo via --git-common-dir
            let common_dir = super::git_in_cwd(&["rev-parse", "--git-common-dir"])?;
            let common_dir = dunce::canonicalize(PathBuf::from(common_dir.trim()))
                .unwrap_or_else(|_| PathBuf::from(common_dir.trim()));
            let is_bare_common = super::git(&common_dir, &["rev-parse", "--is-bare-repository"])?;
            if is_bare_common.trim() == "true" {
                return Ok(common_dir);
            }
        }
        Err(_) => {
            // Step 4: git rev-parse failed — we're likely at the project root
            // with a custom-named bare repo that git can't discover on its own.
            let cwd = std::env::current_dir()?;

            // Step 4a: Try config hint first (fastest)
            if let Some(hint) = bare_dir_hint {
                let candidate = cwd.join(hint);
                if candidate.is_dir() {
                    let is_bare = super::git(&candidate, &["rev-parse", "--is-bare-repository"])
                        .map(|s| s.trim() == "true")
                        .unwrap_or(false);
                    if is_bare {
                        return Ok(dunce::canonicalize(&candidate)
                            .unwrap_or(candidate));
                    }
                }
            }

            // Step 4b: Scan CWD for any directory that looks like a bare repo.
            // A bare git repo has a HEAD file — quick check before calling git.
            if let Ok(entries) = std::fs::read_dir(&cwd) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && path.join("HEAD").is_file() {
                        let is_bare = super::git(&path, &["rev-parse", "--is-bare-repository"])
                            .map(|s| s.trim() == "true")
                            .unwrap_or(false);
                        if is_bare {
                            return Ok(dunce::canonicalize(&path)
                                .unwrap_or(path));
                        }
                    }
                }
            }
        }
    }

    bail!(
        "This command requires a bare Git repository.\n   \
         Please run from a bare repository directory or one of its worktrees."
    )
}
