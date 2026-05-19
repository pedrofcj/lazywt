use anyhow::Result;
use crate::config::Config;
use crate::error::LazywtError;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;
use crate::prompt;
use std::path::Path;

/// Normalize a path string for glob matching (Windows backslash fix).
fn normalize_glob_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Check if copy-tracked files in the target worktree differ from the default worktree (D-100).
/// Returns a list of files that have differences.
fn check_copy_files_diff(
    target_path: &Path,
    default_wt_path: &Path,
    patterns: &[String],
) -> Vec<String> {
    let mut diffs = Vec::new();
    if patterns.is_empty() {
        return diffs;
    }

    for pattern in patterns {
        // Normalize path for glob matching on Windows
        let base = normalize_glob_path(target_path);
        let glob_pattern = format!("{}/{}", base, pattern);
        let entries = match glob::glob(&glob_pattern) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            // Skip non-files
            if !entry.is_file() {
                continue;
            }

            if let Ok(relative) = entry.strip_prefix(target_path) {
                let source_file = default_wt_path.join(relative);

                // Both files must exist to compare
                if !source_file.exists() || !entry.exists() {
                    continue;
                }

                // Compare file contents
                let target_content = std::fs::read(&entry).unwrap_or_default();
                let source_content = std::fs::read(&source_file).unwrap_or_default();

                if target_content != source_content {
                    diffs.push(relative.display().to_string());
                }
            }
        }
    }
    diffs
}

/// Find a worktree by its directory name (last path component).
/// Skips bare entries since they aren't real worktrees.
fn find_worktree_by_name<'a>(
    worktrees: &'a [git::worktree::Worktree],
    name: &str,
) -> Option<&'a git::worktree::Worktree> {
    worktrees.iter().find(|wt| {
        !wt.is_bare && wt.path.file_name().and_then(|n| n.to_str()) == Some(name)
    })
}

/// Remove a single worktree and its associated branch.
///
/// Steps per RESEARCH.md Remove Command Sequence:
/// 1. List worktrees and find the target by name
/// 2. Detect default branch and protect it from removal
/// 3. Show what will be removed and prompt for confirmation
/// 4. Remove worktree with --force, then delete branch with -D
pub fn run(ctx: &ProjectContext, config: &Config, name: &str, yes: bool) -> Result<()> {
    // 1. List worktrees and find target
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
    let target = find_worktree_by_name(&worktrees, name)
        .ok_or_else(|| LazywtError::WorktreeNotFound(name.to_string()))?;

    // 2. Detect default branch (no network)
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?;

    // 3. Protection check (D-17, REM-03, Pitfall 5 -- check BOTH name and branch)
    if let Some(ref default_name) = default_branch {
        if name == default_name.as_str() {
            return Err(LazywtError::CannotRemoveDefault(name.to_string()).into());
        }
        if target.branch.as_deref() == Some(default_name.as_str()) {
            return Err(LazywtError::CannotRemoveDefault(name.to_string()).into());
        }
    }

    // 3b. Check copy-tracked files for differences (D-100)
    if !config.copy_files.is_empty() {
        let default_branch_name = default_branch.clone()
            .unwrap_or_else(|| "main".to_string());
        let all_worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
        if let Some(default_wt) = all_worktrees.iter().find(|wt| {
            !wt.is_bare && wt.branch.as_deref() == Some(&default_branch_name)
        }) {
            if default_wt.path.exists() {
                let diffs = check_copy_files_diff(&target.path, &default_wt.path, &config.copy_files);
                if !diffs.is_empty() {
                    output::warning("  Warning: Copy-tracked files differ from default worktree:");
                    for f in &diffs {
                        output::warning(&format!("    - {}", f));
                    }
                    output::warning("  These changes will be lost when the worktree is removed.");
                    println!();
                }
            }
        }
    }

    // 4. Print what will be removed
    output::header(&format!("Remove worktree '{}'", name));
    output::info(&format!("  Path: {}", target.path.display()));
    if let Some(ref branch_name) = target.branch {
        output::info(&format!("  Branch: {}", branch_name));
    }

    // 5. Prompt for confirmation (D-16)
    if !prompt::confirm(&format!("Remove worktree '{}'?", name), yes)? {
        output::info("Cancelled.");
        return Ok(());
    }

    // 6. Remove worktree (D-18)
    // Change CWD to project_dir before removal. On Windows, a directory cannot
    // be deleted while any process holds it as CWD.  If the user ran lazywt
    // from inside the worktree being removed, the OS would block the delete.
    // Moving CWD to the bare repo root releases the directory lock.
    let target_path_str = target.path.to_str().unwrap_or_default();
    std::env::set_current_dir(&ctx.project_dir).ok();
    output::progress_start("Removing worktree");
    match git::git(&ctx.project_dir, &["worktree", "remove", target_path_str, "--force"]) {
        Ok(_) => {
            output::progress_complete("Worktree removed", output::Status::Success);
        }
        Err(e) => {
            output::progress_complete("Failed to remove worktree", output::Status::Error);
            return Err(anyhow::anyhow!(
                "Failed to remove worktree. Ensure no programs have files open in the worktree directory.\nDetails: {}",
                e
            ));
        }
    }

    // 7. Delete branch (D-18) -- non-fatal if it fails
    if let Some(ref branch_name) = target.branch {
        if git::git(&ctx.project_dir, &["branch", "-D", branch_name]).is_err() {
            output::warning(&format!(
                "Could not delete branch '{}' (it may have already been deleted)",
                branch_name
            ));
        }
    }

    // 8. Print success
    output::success(&format!("Worktree '{}' removed", name));

    Ok(())
}

#[cfg(test)]
mod diff_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn diff_identical_files() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        std::fs::write(src.path().join(".env"), "SAME").unwrap();
        std::fs::write(dst.path().join(".env"), "SAME").unwrap();
        let diffs = check_copy_files_diff(dst.path(), src.path(), &[".env".to_string()]);
        assert!(diffs.is_empty());
    }

    #[test]
    fn diff_different_files() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        std::fs::write(src.path().join(".env"), "OLD").unwrap();
        std::fs::write(dst.path().join(".env"), "NEW").unwrap();
        let diffs = check_copy_files_diff(dst.path(), src.path(), &[".env".to_string()]);
        assert_eq!(diffs, vec![".env"]);
    }

    #[test]
    fn diff_missing_source_no_diff() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        std::fs::write(dst.path().join(".env"), "data").unwrap();
        // Source doesn't have .env -- can't compare
        let diffs = check_copy_files_diff(dst.path(), src.path(), &[".env".to_string()]);
        assert!(diffs.is_empty());
    }

    #[test]
    fn diff_empty_patterns() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let diffs = check_copy_files_diff(dst.path(), src.path(), &[]);
        assert!(diffs.is_empty());
    }
}
