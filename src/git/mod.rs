pub mod branch;
pub mod branch_info;
pub mod config;
pub mod line_diff;
pub mod main_relationship;
pub mod operations;
pub mod repo;
pub mod status;
pub mod worktree;

use std::path::Path;
use std::process::Command;
use anyhow::{Context, Result, bail};

/// Run a git command in the given directory, returning stdout as String.
/// Fails with descriptive error if git exits non-zero.
/// Per D-01: wraps git errors with lazywt's own messages.
pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("failed to execute git -- is git installed and on PATH?")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed (exit {}): {}",
            args.join(" "),
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("git output was not valid UTF-8")
}

/// Run a git command, returning Ok(true) if exit 0, Ok(false) if non-zero.
/// Only fails on I/O error (git not found, etc).
/// Use for boolean queries like "does branch exist?" (Pitfall 4: exit code 1 is valid "no").
pub fn git_check(dir: &Path, args: &[&str]) -> Result<bool> {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to execute git")?;
    Ok(status.success())
}

/// Run a git command using current working directory (no -C flag).
/// Used during startup before we know the repo root.
pub fn git_in_cwd(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .context("failed to execute git -- is git installed and on PATH?")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed (exit {}): {}",
            args.join(" "),
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("git output was not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .arg(dir.path())
            .output()
            .expect("failed to run git init --bare");
        dir
    }

    #[test]
    fn git_on_valid_repo_returns_ok() {
        let dir = setup_git_repo();
        let result = git(dir.path(), &["rev-parse", "--git-dir"]);
        assert!(result.is_ok(), "git() should succeed on a valid repo: {:?}", result);
    }

    #[test]
    fn git_on_invalid_command_returns_error() {
        let dir = setup_git_repo();
        let result = git(dir.path(), &["invalid-nonexistent-command"]);
        assert!(result.is_err(), "git() should fail on invalid command");
    }

    #[test]
    fn git_check_returns_true_for_success() {
        let dir = setup_git_repo();
        let result = git_check(dir.path(), &["rev-parse", "--git-dir"]);
        assert!(result.is_ok());
        assert!(result.unwrap(), "git_check() should return true for successful command");
    }

    #[test]
    fn git_check_returns_false_for_failure() {
        let dir = setup_git_repo();
        let result = git_check(dir.path(), &["rev-parse", "--verify", "refs/heads/nonexistent"]);
        assert!(result.is_ok(), "git_check() should not error on non-zero exit");
        assert!(!result.unwrap(), "git_check() should return false for failing command");
    }
}
