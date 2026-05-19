use std::path::Path;
use anyhow::Result;

/// Check whether a branch exists locally or on origin remote.
/// Returns true if found at refs/heads/<name> or refs/remotes/origin/<name>.
pub fn branch_exists(project_dir: &Path, branch_name: &str) -> Result<bool> {
    // Check local branch first
    let local = super::git_check(
        project_dir,
        &["show-ref", "--verify", "--quiet", &format!("refs/heads/{}", branch_name)],
    )?;
    if local {
        return Ok(true);
    }

    // Check remote branch
    let remote = super::git_check(
        project_dir,
        &["show-ref", "--verify", "--quiet", &format!("refs/remotes/origin/{}", branch_name)],
    )?;
    Ok(remote)
}

/// Detect the default branch for a repository.
///
/// If `allow_network` is true, runs `git remote set-head origin --auto` to refresh origin/HEAD.
/// Then tries `git rev-parse --abbrev-ref origin/HEAD` to read the symbolic ref.
/// Falls back to checking for local `main` then `master` branches.
/// Returns None if no default branch can be determined.
pub fn detect_default_branch(project_dir: &Path, allow_network: bool) -> Result<Option<String>> {
    // Optionally refresh origin/HEAD from remote
    if allow_network {
        let _ = super::git_check(project_dir, &["remote", "set-head", "origin", "--auto"]);
    }

    // Try to read origin/HEAD
    if let Ok(output) = super::git(project_dir, &["rev-parse", "--abbrev-ref", "origin/HEAD"]) {
        let trimmed = output.trim();
        if !trimmed.is_empty() && trimmed != "origin/HEAD" {
            // Strip "origin/" prefix
            let branch = trimmed.strip_prefix("origin/").unwrap_or(trimmed);
            return Ok(Some(branch.to_string()));
        }
    }

    // Fallback: check for common local branches
    if super::git_check(
        project_dir,
        &["show-ref", "--verify", "--quiet", "refs/heads/main"],
    )? {
        return Ok(Some("main".to_string()));
    }

    if super::git_check(
        project_dir,
        &["show-ref", "--verify", "--quiet", "refs/heads/master"],
    )? {
        return Ok(Some("master".to_string()));
    }

    Ok(None)
}

/// Set upstream tracking for a branch to origin/<branch_name>.
/// Failures are non-fatal -- callers should warn but not abort.
pub fn set_upstream(worktree_path: &Path, branch_name: &str) -> Result<()> {
    super::git(
        worktree_path,
        &["branch", "--set-upstream-to", &format!("origin/{}", branch_name)],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a bare repo for testing branch utilities.
    fn setup_bare_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .arg(dir.path())
            .output()
            .expect("git init --bare failed");
        dir
    }

    /// Create a bare repo with a commit on "main" branch.
    fn setup_bare_repo_with_main() -> (TempDir, TempDir) {
        let bare_dir = TempDir::new().unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .arg(bare_dir.path())
            .output()
            .expect("git init --bare failed");

        // Create a worktree to make a commit
        let wt_dir = TempDir::new().unwrap();
        let wt_path = wt_dir.path().join("main");
        Command::new("git")
            .arg("-C")
            .arg(bare_dir.path())
            .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", "main"])
            .output()
            .expect("git worktree add failed");

        // Configure git user for commits
        Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .expect("git config user.email failed");

        Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(["config", "user.name", "Test User"])
            .output()
            .expect("git config user.name failed");

        Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(["commit", "--allow-empty", "-m", "initial"])
            .output()
            .expect("git commit failed");

        // Point bare repo HEAD to main
        Command::new("git")
            .arg("-C")
            .arg(bare_dir.path())
            .args(["symbolic-ref", "HEAD", "refs/heads/main"])
            .output()
            .expect("git symbolic-ref HEAD failed");

        (bare_dir, wt_dir)
    }

    #[test]
    fn branch_exists_returns_false_for_nonexistent() {
        let dir = setup_bare_repo();
        let result = branch_exists(dir.path(), "nonexistent").unwrap();
        assert!(!result, "branch_exists should return false for nonexistent branch");
    }

    #[test]
    fn branch_exists_returns_true_for_existing_local() {
        let (bare_dir, _wt_dir) = setup_bare_repo_with_main();
        let result = branch_exists(bare_dir.path(), "main").unwrap();
        assert!(result, "branch_exists should return true for existing 'main' branch");
    }

    #[test]
    fn detect_default_branch_with_no_remote_returns_local_fallback() {
        let (bare_dir, _wt_dir) = setup_bare_repo_with_main();
        // No remote configured, so origin/HEAD won't work.
        // Should fall back to checking for local "main" branch.
        let result = detect_default_branch(bare_dir.path(), false).unwrap();
        assert_eq!(result, Some("main".to_string()));
    }

    #[test]
    fn detect_default_branch_empty_repo_returns_none() {
        let dir = setup_bare_repo();
        let result = detect_default_branch(dir.path(), false).unwrap();
        assert_eq!(result, None, "empty bare repo should return None");
    }
}
