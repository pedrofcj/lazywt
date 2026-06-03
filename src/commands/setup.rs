use anyhow::{Result, bail};
use std::path::{Path, PathBuf};
use crate::config::Config;
use crate::git;
use crate::output;
use crate::prompt;

/// Ensure .wts is in .gitignore when using inside worktree location (D-95).
pub fn ensure_gitignore_entry(repo_root: &Path) -> Result<()> {
    let gitignore = repo_root.join(".gitignore");
    let entry = ".wts";

    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore)?;
        if content.lines().any(|l| l.trim() == entry) {
            return Ok(()); // Already present
        }
        // Append
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&gitignore)?;
        writeln!(file)?;
        writeln!(file, "# lazywt worktrees")?;
        writeln!(file, "{}", entry)?;
    } else {
        // Create new .gitignore
        std::fs::write(&gitignore, format!("# lazywt worktrees\n{}\n", entry))?;
    }
    output::info("  Added .wts to .gitignore");
    Ok(())
}

/// Convert a regular (non-bare) repository to a bare repository with worktree structure (D-97).
///
/// Uses rename-to-backup strategy instead of delete-then-move.
/// Both reviewers flagged the original delete approach
/// as having irreversible data loss risk if bare clone fails or is incomplete.
///
/// Strategy:
/// 1. Check preconditions (not bare, no uncommitted changes)
/// 2. Clone --bare into temp location
/// 3. RENAME original repo to {name}.setup-backup (preserves hooks, local config, stash)
/// 4. Move bare clone into original location
/// 5. Create worktree for current branch
/// 6. Verify worktree works
/// 7. Print message about backup location (user can delete manually when satisfied)
pub fn run(config: &Config, dry_run: bool, yes: bool, bare_dir: Option<&str>) -> Result<()> {
    // Step 1: Ensure we're in a regular git repo (not already bare)
    let is_bare = git::git_in_cwd(&["rev-parse", "--is-bare-repository"])?;
    if is_bare.trim() == "true" {
        bail!("Repository is already bare. Nothing to convert.");
    }

    let toplevel = git::git_in_cwd(&["rev-parse", "--show-toplevel"])?;
    let repo_root = dunce::canonicalize(PathBuf::from(toplevel.trim()))
        .unwrap_or_else(|_| PathBuf::from(toplevel.trim()));

    // Resolve effective bare dir: flag -> config -> ".git" default (per D-07)
    let effective_bare_dir = bare_dir
        .map(|s| s.to_string())
        .or_else(|| config.bare_dir.clone())
        .unwrap_or_else(|| ".git".to_string());

    let git_dir = repo_root.join(".git");
    if !git_dir.is_dir() {
        bail!("Expected .git directory at {}", git_dir.display());
    }

    // Check for uncommitted changes
    let status_output = git::git_in_cwd(&["status", "--porcelain"])?;
    if !status_output.trim().is_empty() {
        bail!(
            "Working directory has uncommitted changes. \
             Commit or stash them before running setup.\n\
             Run `git status` to see the changes."
        );
    }

    // Detect current branch
    let current_branch = git::git_in_cwd(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let current_branch = current_branch.trim();

    // Plan the conversion
    let parent = repo_root.parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine parent directory"))?;
    let repo_name = repo_root.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");

    // Backup path: {name}.setup-backup in the same parent directory
    let backup_dir = parent.join(format!("{}.setup-backup", repo_name));

    output::header("Setup: Convert to bare repository");
    output::info(&format!("  Repository: {}", repo_root.display()));
    output::info(&format!("  Current branch: {}", current_branch));
    output::info(&format!("  Bare repo will be at: {}", repo_root.display()));
    if effective_bare_dir != ".git" {
        output::info(&format!("  Bare repo dir: {}", effective_bare_dir));
    }
    output::info(&format!("  Backup will be at: {}", backup_dir.display()));
    output::info(&format!("  Worktree for '{}' will be created", current_branch));

    if dry_run {
        output::info("\n  [DRY RUN] No changes made.");
        return Ok(());
    }

    // Confirm (D-98)
    if !prompt::confirm("Convert this repository to bare? (original will be backed up)", yes)? {
        output::info("Cancelled.");
        return Ok(());
    }

    // Check backup dir doesn't already exist
    if backup_dir.exists() {
        bail!(
            "Backup directory already exists: {}\n\
             Remove it first if a previous setup attempt failed.",
            backup_dir.display()
        );
    }

    // Step 2: Clone as bare into a temp location
    let temp_bare = parent.join(format!(".{}-bare-temp", repo_name));
    if temp_bare.exists() {
        std::fs::remove_dir_all(&temp_bare)?;
    }

    output::progress_start("Creating bare clone");
    git::git_in_cwd(&[
        "clone",
        "--bare",
        repo_root.to_str().unwrap(),
        temp_bare.to_str().unwrap(),
    ])?;
    output::progress_complete("Bare clone created", output::Status::Success);

    // Step 3: RENAME original repo to backup (NOT delete)
    // This is the rollback-safe approach. If anything fails after this point,
    // the user can rename the backup back to the original location.

    // Release CWD lock before rename (Windows holds implicit directory handle on CWD).
    // Same pattern as migrate.rs.
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.starts_with(&repo_root) {
            let _ = std::env::set_current_dir(parent);
        }
    }

    output::progress_start("Backing up original repository");
    std::fs::rename(&repo_root, &backup_dir).map_err(|e| {
        // Clean up temp bare clone if rename fails
        let _ = std::fs::remove_dir_all(&temp_bare);
        anyhow::anyhow!(
            "Failed to rename repository to backup: {}. \
             No changes were made to your repository.",
            e
        )
    })?;
    output::progress_complete("Original backed up", output::Status::Success);

    // Step 4: Move bare clone into original location (modern layout)
    output::progress_start("Setting up bare repository");
    let move_result = (|| -> Result<()> {
        std::fs::create_dir_all(&repo_root)?;
        let final_git_dir = repo_root.join(&effective_bare_dir);
        std::fs::rename(&temp_bare, &final_git_dir)?;

        // Set wt.layout and wt.baredir config
        let _ = git::git(&final_git_dir, &["config", "wt.layout", "modern"]);
        let _ = git::git(&final_git_dir, &["config", "wt.baredir", &effective_bare_dir]);
        Ok(())
    })();

    if let Err(e) = move_result {
        // Rollback: restore the backup
        output::warning("Setup failed, rolling back...");
        let _ = std::fs::remove_dir_all(&repo_root);
        std::fs::rename(&backup_dir, &repo_root).ok();
        let _ = std::fs::remove_dir_all(&temp_bare);
        bail!("Failed to set up bare repository: {}. Original repo restored.", e);
    }
    output::progress_complete("Bare repo in place", output::Status::Success);

    // Step 5: Create worktree for the current branch
    let final_git_dir = repo_root.join(&effective_bare_dir);
    let wt_path = repo_root.join(current_branch);
    output::progress_start(&format!("Creating worktree for '{}'", current_branch));
    let wt_result = git::git(
        &final_git_dir,
        &["worktree", "add", wt_path.to_str().unwrap(), current_branch],
    );

    if let Err(e) = wt_result {
        output::warning(
            "Worktree creation failed. Your bare repo is in place \
             but you may need to create worktrees manually.",
        );
        output::warning(&format!("  Error: {}", e));
        output::info(&format!("  Backup at: {}", backup_dir.display()));
        return Ok(());
    }
    output::progress_complete("Worktree created", output::Status::Success);

    output::success(&format!(
        "Repository converted to bare. Worktree '{}' ready at {}",
        current_branch,
        wt_path.display()
    ));
    output::info(&format!(
        "\n  Backup of original repo at: {}\n  \
         You can delete it once you've verified everything works:\n  \
         rm -rf \"{}\"",
        backup_dir.display(),
        backup_dir.display()
    ));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ensure_gitignore_creates_new_file() {
        let dir = TempDir::new().unwrap();
        ensure_gitignore_entry(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains(".wts"), "should contain .wts entry");
        assert!(content.contains("lazywt"), "should contain lazywt comment");
    }

    #[test]
    fn ensure_gitignore_appends_to_existing() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "node_modules\n").unwrap();
        ensure_gitignore_entry(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains("node_modules"), "should preserve existing entries");
        assert!(content.contains(".wts"), "should add .wts entry");
    }

    #[test]
    fn ensure_gitignore_skips_if_already_present() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), ".wts\n").unwrap();
        ensure_gitignore_entry(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        // Should not duplicate
        assert_eq!(content.matches(".wts").count(), 1, "should not duplicate .wts entry");
    }
}
