use anyhow::{Result, bail};
use crate::config::Config;
use crate::error::LazywtError;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;
use std::collections::HashSet;
use std::path::Path;

/// Normalize a path string for glob matching.
/// On Windows, `display().to_string()` produces backslash paths,
/// but the glob crate requires forward slashes for pattern matching.
fn normalize_glob_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Copy files matching config.copy_files patterns from default worktree to new worktree (D-99).
fn copy_config_files(
    default_wt_path: &Path,
    new_wt_path: &Path,
    patterns: &[String],
) {
    if patterns.is_empty() {
        return;
    }

    let mut copied = 0;
    let mut seen = HashSet::new();

    for pattern in patterns {
        // Normalize path for glob matching on Windows
        let base = normalize_glob_path(default_wt_path);
        let glob_pattern = format!("{}/{}", base, pattern);
        let entries = match glob::glob(&glob_pattern) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            // Skip directories and symlinks -- only copy regular files
            if !entry.is_file() {
                continue;
            }

            // Get relative path from default worktree
            if let Ok(relative) = entry.strip_prefix(default_wt_path) {
                let relative_str = relative.to_string_lossy().to_string();

                // Skip duplicates (same file matched by multiple patterns)
                if !seen.insert(relative_str) {
                    continue;
                }

                let dest = new_wt_path.join(relative);

                // Create parent directories if needed
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                match std::fs::copy(&entry, &dest) {
                    Ok(_) => {
                        output::info(&format!(
                            "  Copied: {}",
                            relative.display()
                        ));
                        copied += 1;
                    }
                    Err(e) => {
                        output::warning(&format!(
                            "  Warning: failed to copy {}: {}",
                            relative.display(), e
                        ));
                    }
                }
            }
        }
    }
    if copied > 0 {
        output::info(&format!("  {} file(s) copied from default worktree", copied));
    }
}

/// Resolve the git branch name for a new worktree.
/// If `explicit_branch` is Some, use it directly (D-65: overrides prefix completely).
/// Otherwise, apply branch_prefix with auto-separator (D-66).
/// Empty prefix means branch = worktree name (D-68).
fn resolve_branch_name(worktree_name: &str, explicit_branch: Option<&str>, branch_prefix: &str) -> String {
    if let Some(branch) = explicit_branch {
        return branch.to_string();
    }
    if branch_prefix.is_empty() {
        worktree_name.to_string()
    } else if branch_prefix.ends_with('/') {
        format!("{}{}", branch_prefix, worktree_name)
    } else {
        // D-66: auto-add separator when prefix doesn't end with /
        format!("{}/{}", branch_prefix, worktree_name)
    }
}

/// Reserved directory names that cannot be used as worktree names.
const RESERVED_NAMES: &[&str] = &[".git", ".bare", ".."];

/// Validate that a worktree name is acceptable.
/// Rejects reserved names, path separators, and conflicts with the worktree folder.
fn validate_worktree_name(name: &str, worktree_folder: &str) -> Result<()> {
    // Check reserved names
    if RESERVED_NAMES.contains(&name) {
        bail!(LazywtError::InvalidName(format!(
            "'{}' is a reserved name",
            name
        )));
    }

    // Check for path separators
    if name.contains('/') || name.contains('\\') {
        bail!(LazywtError::InvalidName(
            "cannot contain path separators".to_string()
        ));
    }

    // Check conflict with worktree folder name
    if !worktree_folder.is_empty() && name == worktree_folder {
        bail!(LazywtError::InvalidName(format!(
            "conflicts with worktree folder name '{}'",
            worktree_folder
        )));
    }

    Ok(())
}

/// Initialize the main worktree if it does not exist.
/// Returns the name of the default branch.
fn initialize_main_worktree(ctx: &ProjectContext) -> Result<String> {
    // 1. Detect default branch (no network during add)
    let mut default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?;

    // 2. List existing worktrees
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;

    // 3. Check if any non-bare worktree already has the default branch
    if let Some(ref branch_name) = default_branch {
        for wt in &worktrees {
            if !wt.is_bare {
                if let Some(ref wt_branch) = wt.branch {
                    if wt_branch == branch_name {
                        // Main worktree already exists -- pull latest and return
                        let _ = git::git(&wt.path, &["pull"]);
                        let _ = git::git(&ctx.project_dir, &["fetch", "--all"]);
                        return Ok(branch_name.clone());
                    }
                }
            }
        }
    }

    // 4. Fallback: check for common branch names in existing worktrees
    for common_name in &["main", "master", "develop", "trunk"] {
        for wt in &worktrees {
            if !wt.is_bare {
                if let Some(ref wt_branch) = wt.branch {
                    if wt_branch == common_name {
                        let _ = git::git(&wt.path, &["pull"]);
                        let _ = git::git(&ctx.project_dir, &["fetch", "--all"]);
                        return Ok(common_name.to_string());
                    }
                }
            }
        }
    }

    // 5. No main worktree found -- need to create one
    if default_branch.is_none() {
        // 5a. Fix refspec and fetch, then retry detection
        let _ = git::config::set_config(
            &ctx.project_dir,
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        );
        let _ = git::git(&ctx.project_dir, &["fetch", "--all"]);
        default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?;
    }

    // 5b. Still no default branch? Bail.
    let default_branch = default_branch.ok_or(LazywtError::NoDefaultBranch)?;

    // 5c. Compute main worktree path
    let main_wt_path = ctx.worktree_parent.join(&default_branch);

    // 5d. Create parent directory
    std::fs::create_dir_all(&ctx.worktree_parent)?;

    // 5e. Create the worktree
    git::git(
        &ctx.project_dir,
        &[
            "worktree",
            "add",
            main_wt_path.to_str().unwrap(),
            &default_branch,
        ],
    )?;

    // 5f. Set upstream (non-fatal)
    let _ = git::branch::set_upstream(&main_wt_path, &default_branch);

    // 5g. Print info
    output::info(&format!(
        "Initialized main worktree for branch '{}'",
        default_branch
    ));

    // 6. Pull latest (non-fatal)
    let _ = git::git(&main_wt_path, &["pull"]);

    // 7. Fetch all (non-fatal)
    let _ = git::git(&ctx.project_dir, &["fetch", "--all"]);

    // 8. Return the default branch name
    Ok(default_branch)
}

/// Create a new worktree with branch naming conventions.
///
/// - `name`: the worktree directory name (e.g., "auth")
/// - `branch`: optional explicit branch name (overrides branch_prefix from config)
/// - `from`: optional source worktree name to branch from
pub fn run(
    ctx: &ProjectContext,
    config: &Config,
    name: &str,
    branch: Option<&str>,
    from: Option<&str>,
) -> Result<()> {
    // 1. Validate name
    validate_worktree_name(name, &ctx.worktree_folder)?;

    // 2. Handle --from flag: look up source worktree
    let start_point: Option<String> = if let Some(from_name) = from {
        let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
        let source_wt = worktrees
            .iter()
            .find(|wt| {
                !wt.is_bare
                    && wt
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == from_name)
                        .unwrap_or(false)
            })
            .ok_or_else(|| LazywtError::SourceWorktreeNotFound(from_name.to_string()))?;

        let branch = source_wt
            .branch
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "source worktree '{}' has a detached HEAD -- cannot branch from it",
                    from_name
                )
            })?
            .clone();

        // Fetch all (warn on failure)
        if git::git(&ctx.project_dir, &["fetch", "--all"]).is_err() {
            output::warning("  Warning: failed to fetch from remote");
        }

        // Pull in source worktree (warn on failure)
        if git::git(&source_wt.path, &["pull"]).is_err() {
            output::warning("  Warning: failed to pull in source worktree");
        }

        Some(branch)
    } else {
        // 3. No --from: initialize main worktree if needed
        initialize_main_worktree(ctx)?;
        None
    };

    // 4. Compute branch name using resolve_branch_name (D-65..D-68)
    let branch_name = resolve_branch_name(name, branch, &config.branch_prefix);

    // 5. Compute worktree path
    let worktree_path = ctx.worktree_parent.join(name);

    // 6. Check path exists
    if worktree_path.exists() {
        bail!(LazywtError::WorktreeExists(name.to_string()));
    }

    // 7. Print header
    output::header(&format!("Adding worktree '{}'", name));
    output::info(&format!("  Branch: {}", branch_name));
    if let Some(ref from_name) = from {
        output::info(&format!("  From: {}", from_name));
    }

    // 8. Create parent directory
    std::fs::create_dir_all(&ctx.worktree_parent)?;

    // 9. Check if branch exists
    let exists = git::branch::branch_exists(&ctx.project_dir, &branch_name)?;

    // 10. Create worktree based on branch existence
    let wt_path_str = worktree_path.to_str().unwrap();

    if exists && from.is_some() {
        output::warning("  Branch already exists, --from will be ignored");
    }

    if exists {
        // Checkout existing branch into new worktree
        git::git(
            &ctx.project_dir,
            &["worktree", "add", wt_path_str, &branch_name],
        )?;
    } else if let Some(ref start) = start_point {
        // Create new branch from start_point
        git::git(
            &ctx.project_dir,
            &["worktree", "add", "-b", &branch_name, wt_path_str, start],
        )?;
    } else {
        // Create new branch from HEAD
        git::git(
            &ctx.project_dir,
            &["worktree", "add", "-b", &branch_name, wt_path_str],
        )?;
    }

    // 11. Set upstream if remote branch exists
    if git::git_check(
        &ctx.project_dir,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{}", branch_name),
        ],
    )? {
        let _ = git::branch::set_upstream(&worktree_path, &branch_name);
    }

    // 11b. Auto-manage .gitignore for non-bare repos using inside mode (D-95)
    if ctx.layout_type == crate::layout::LayoutType::NonBare && ctx.worktree_folder == ".wts" {
        let _ = crate::commands::setup::ensure_gitignore_entry(&ctx.project_root);
    }

    // 11c. Copy files from default worktree if configured (D-99)
    if !config.copy_files.is_empty() {
        let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?
            .unwrap_or_else(|| "main".to_string());
        let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
        if let Some(default_wt) = worktrees.iter().find(|wt| {
            !wt.is_bare && wt.branch.as_deref() == Some(&default_branch)
        }) {
            if default_wt.path.exists() {
                copy_config_files(&default_wt.path, &worktree_path, &config.copy_files);
            }
        }
    }


    // 12. Print success
    output::success(&format!(
        "Worktree '{}' created at {}",
        name,
        worktree_path.display()
    ));

    // 13. Prompt or auto-navigate to the new worktree
    crate::navigate::maybe_navigate(&worktree_path, config);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_dot_git() {
        let result = validate_worktree_name(".git", "trees");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("reserved"), "expected 'reserved' in: {}", msg);
    }

    #[test]
    fn validate_rejects_dot_bare() {
        let result = validate_worktree_name(".bare", "trees");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("reserved") || msg.contains("invalid"),
            "expected 'reserved' or 'invalid' in: {}",
            msg
        );
    }

    #[test]
    fn validate_rejects_double_dot() {
        let result = validate_worktree_name("..", "trees");
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_path_separators() {
        let result = validate_worktree_name("foo/bar", "trees");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("path separators"),
            "expected 'path separators' in: {}",
            msg
        );
    }

    #[test]
    fn validate_rejects_worktree_folder_conflict() {
        let result = validate_worktree_name("trees", "trees");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("conflicts"),
            "expected 'conflicts' in: {}",
            msg
        );
    }

    #[test]
    fn validate_accepts_normal_name() {
        let result = validate_worktree_name("auth", "trees");
        assert!(result.is_ok());
    }

    // resolve_branch_name tests (D-65..D-68)

    #[test]
    fn resolve_branch_name_no_prefix_no_branch() {
        // D-68: no prefix, no --branch -> branch = worktree name
        assert_eq!(resolve_branch_name("auth", None, ""), "auth");
    }

    #[test]
    fn resolve_branch_name_explicit_branch_overrides_prefix() {
        // D-65: --branch overrides prefix completely
        assert_eq!(resolve_branch_name("auth", Some("custom"), "feat"), "custom");
    }

    #[test]
    fn resolve_branch_name_prefix_auto_separator() {
        // D-66: auto-add / separator when prefix doesn't end with /
        assert_eq!(resolve_branch_name("auth", None, "feat"), "feat/auth");
    }

    #[test]
    fn resolve_branch_name_prefix_trailing_slash() {
        // D-66: trailing / preserved
        assert_eq!(resolve_branch_name("auth", None, "feat/"), "feat/auth");
    }

    #[test]
    fn resolve_branch_name_explicit_branch_no_prefix() {
        // D-65: explicit branch with no prefix
        assert_eq!(resolve_branch_name("auth", Some("my-branch"), ""), "my-branch");
    }
}

#[cfg(test)]
mod copy_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn normalize_glob_path_converts_backslashes() {
        let path = Path::new("C:\\Users\\test\\project");
        let normalized = normalize_glob_path(path);
        assert!(!normalized.contains('\\'), "Should not contain backslashes: {}", normalized);
        assert!(normalized.contains('/'));
    }

    #[test]
    fn copy_config_files_empty_patterns() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        // Should not panic or error
        copy_config_files(src.path(), dst.path(), &[]);
    }

    #[test]
    fn copy_config_files_copies_matching_file() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        std::fs::write(src.path().join(".env"), "SECRET=123").unwrap();
        copy_config_files(src.path(), dst.path(), &[".env".to_string()]);
        assert!(dst.path().join(".env").exists());
        assert_eq!(
            std::fs::read_to_string(dst.path().join(".env")).unwrap(),
            "SECRET=123"
        );
    }

    #[test]
    fn copy_config_files_glob_pattern() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        std::fs::write(src.path().join("a.txt"), "a").unwrap();
        std::fs::write(src.path().join("b.txt"), "b").unwrap();
        std::fs::write(src.path().join("c.rs"), "c").unwrap();
        copy_config_files(src.path(), dst.path(), &["*.txt".to_string()]);
        assert!(dst.path().join("a.txt").exists());
        assert!(dst.path().join("b.txt").exists());
        assert!(!dst.path().join("c.rs").exists());
    }

    #[test]
    fn copy_config_files_no_match_no_error() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        // No files in src, should not panic
        copy_config_files(src.path(), dst.path(), &[".env".to_string()]);
        assert!(!dst.path().join(".env").exists());
    }

    #[test]
    fn copy_config_files_skips_directories() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        std::fs::create_dir(src.path().join("subdir")).unwrap();
        std::fs::write(src.path().join("file.txt"), "data").unwrap();
        copy_config_files(src.path(), dst.path(), &["*".to_string()]);
        assert!(dst.path().join("file.txt").exists());
        // subdir should NOT be copied (we only copy files)
    }

    #[test]
    fn copy_config_files_deduplicates_across_patterns() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        std::fs::write(src.path().join(".env"), "data").unwrap();
        // Pattern ".env" and "*" both match .env -- should only copy once
        copy_config_files(src.path(), dst.path(), &[".env".to_string(), "*".to_string()]);
        assert!(dst.path().join(".env").exists());
    }
}
