use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use crate::config::Config;
use crate::error::LazywtError;
use crate::git;
use crate::layout::{LayoutType, ProjectContext};
use crate::output;
use crate::prompt;

/// Tracks a completed filesystem move for rollback purposes.
struct MoveRecord {
    source: PathBuf,
    dest: PathBuf,
}

/// Check if a worktree has uncommitted changes.
/// Returns the number of changed files, or 0 if clean/error.
fn count_dirty_files(worktree_path: &Path) -> usize {
    match git::git(worktree_path, &["status", "--porcelain"]) {
        Ok(output) => output.lines().count(),
        Err(_) => 0, // Treat errors as "unknown, assume clean"
    }
}

/// Reverse completed moves in LIFO order. Returns a list of rollback errors.
fn rollback(completed_moves: &[MoveRecord]) -> Vec<String> {
    let mut errors = Vec::new();
    for record in completed_moves.iter().rev() {
        if let Err(e) = std::fs::rename(&record.dest, &record.source) {
            errors.push(format!(
                "Failed to rollback {} -> {}: {}",
                record.dest.display(),
                record.source.display(),
                e
            ));
        }
    }
    errors
}

/// Migrate a classic-layout bare repo to modern layout.
///
/// Classic layout: project_dir is the bare dir (e.g., /path/container/.bare),
/// worktrees live in project_dir/trees/<name>.
///
/// Modern layout: bare dir becomes /path/container/.git,
/// worktrees move to /path/container/<name>.
pub fn run(ctx: &ProjectContext, config: &Config, dry_run: bool, yes: bool, bare_dir: Option<&str>) -> Result<()> {
    // Step 0: Precondition check (MIG-01)
    if ctx.layout_type == LayoutType::Modern {
        return Err(LazywtError::AlreadyModernLayout.into());
    }

    // Resolve effective bare dir: flag -> config -> ".git" default (per D-02)
    let effective_bare_dir = bare_dir
        .map(|s| s.to_string())
        .or_else(|| config.bare_dir.clone())
        .unwrap_or_else(|| ".git".to_string());

    // Step 1: Compute new paths
    // For classic layout: project_dir is the bare dir (e.g., /path/container/.bare)
    // new_root is its parent (e.g., /path/container)
    // new_bare_dir is new_root/<effective_bare_dir>
    let new_root = ctx.project_dir.parent()
        .ok_or_else(|| LazywtError::MigrateFailed(
            "bare repo has no parent directory".to_string()
        ))?
        .to_path_buf();
    let new_bare_dir = new_root.join(&effective_bare_dir);

    // Step 2: Collision check (D-24, MIG-02)
    if new_bare_dir.exists() {
        return Err(LazywtError::MigrateTargetExists(effective_bare_dir.clone()).into());
    }

    // Step 3: Parse worktrees (MIG-04)
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;

    let mut internal_worktrees: Vec<(PathBuf, String)> = Vec::new(); // (old_path, leaf_name)
    let mut external_worktrees: Vec<PathBuf> = Vec::new();

    for wt in &worktrees {
        if wt.is_bare {
            continue;
        }
        if wt.path.starts_with(&ctx.project_dir) {
            let leaf = wt.path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            internal_worktrees.push((wt.path.clone(), leaf));
        } else {
            external_worktrees.push(wt.path.clone());
        }
    }

    // Check worktree destination collisions before proceeding
    for (_old_path, leaf) in &internal_worktrees {
        let new_wt_path = new_root.join(leaf);
        if new_wt_path.exists() {
            return Err(LazywtError::MigrateTargetExists(leaf.clone()).into());
        }
    }

    // Step 4: Check uncommitted changes (D-23, MIG-03)
    let mut has_dirty = false;
    for wt in &worktrees {
        if wt.is_bare {
            continue;
        }
        let dirty_count = count_dirty_files(&wt.path);
        if dirty_count > 0 {
            if !has_dirty {
                output::warning("Warning: some worktrees have uncommitted changes:");
                has_dirty = true;
            }
            let name = wt.path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            output::warning(&format!("  {} has {} uncommitted change(s)", name, dirty_count));
        }
    }

    // Step 5: Show migration preview (D-22)
    output::header("Migration plan");
    output::info(&format!("  Bare repo: {} -> {}", ctx.project_dir.display(), new_bare_dir.display()));
    if effective_bare_dir != ".git" {
        output::info(&format!("  Bare repo dir: {}", effective_bare_dir));
    }
    for (old_path, leaf) in &internal_worktrees {
        let new_wt_path = new_root.join(leaf);
        output::info(&format!("  Worktree: {} -> {}", old_path.display(), new_wt_path.display()));
    }
    for ext_path in &external_worktrees {
        output::info(&format!("  External (not moved): {}", ext_path.display()));
    }

    if dry_run {
        output::info("Dry run -- no changes made.");
        return Ok(());
    }

    // Step 6: Prompt for confirmation (D-21)
    if !prompt::confirm("Proceed with migration?", yes)? {
        output::info("Cancelled.");
        return Ok(());
    }

    // Windows CWD handling (Pitfall 1): release lock if CWD is inside bare dir
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.starts_with(&ctx.project_dir) {
            if let Some(parent) = new_root.parent() {
                let _ = std::env::set_current_dir(parent);
            }
        }
    }

    // Step 7: Execute moves with rollback tracking (D-25, MIG-04, MIG-05)
    let mut completed_moves: Vec<MoveRecord> = Vec::new();

    // 7a. Move each internal worktree to new location
    for (old_path, leaf) in &internal_worktrees {
        let new_wt_path = new_root.join(leaf);
        output::progress_start(&format!("Moving worktree '{}'", leaf));

        if let Err(e) = std::fs::rename(old_path, &new_wt_path) {
            output::progress_complete(&format!("Failed to move '{}'", leaf), output::Status::Error);
            let rollback_errors = rollback(&completed_moves);
            let mut msg = format!("Failed to move {} -> {}: {}", old_path.display(), new_wt_path.display(), e);
            if !rollback_errors.is_empty() {
                msg.push_str("\nRollback also failed:\n");
                for re in &rollback_errors {
                    msg.push_str(&format!("  {}\n", re));
                }
            }
            return Err(LazywtError::MigrateFailed(msg).into());
        }

        completed_moves.push(MoveRecord {
            source: old_path.clone(),
            dest: new_wt_path,
        });
        output::progress_complete(&format!("Moved '{}'", leaf), output::Status::Success);
    }

    // 7b. Move bare repo: rename project_dir (.bare) to new_bare_dir (.git)
    output::progress_start("Moving bare repository");
    if let Err(e) = std::fs::rename(&ctx.project_dir, &new_bare_dir) {
        output::progress_complete("Failed to move bare repository", output::Status::Error);
        let rollback_errors = rollback(&completed_moves);
        let mut msg = format!(
            "Failed to move {} -> {}: {}",
            ctx.project_dir.display(),
            new_bare_dir.display(),
            e
        );
        if !rollback_errors.is_empty() {
            msg.push_str("\nRollback also failed:\n");
            for re in &rollback_errors {
                msg.push_str(&format!("  {}\n", re));
            }
        }
        return Err(LazywtError::MigrateFailed(msg).into());
    }
    completed_moves.push(MoveRecord {
        source: ctx.project_dir.clone(),
        dest: new_bare_dir.clone(),
    });
    output::progress_complete("Bare repository moved", output::Status::Success);

    // 7c. Set git config in new location
    git::config::set_config(&new_bare_dir, "core.bare", "true")
        .context("failed to set core.bare config")?;
    git::config::set_config(&new_bare_dir, "wt.layout", "modern")
        .context("failed to set wt.layout config")?;
    git::config::set_config(&new_bare_dir, "wt.baredir", &effective_bare_dir)
        .context("failed to set wt.baredir config")?;

    // 7d. Run git worktree repair with all new worktree paths (MIG-05)
    let new_wt_paths: Vec<String> = completed_moves.iter()
        .filter(|m| m.dest != new_bare_dir) // exclude bare repo itself
        .map(|m| m.dest.to_string_lossy().to_string())
        .chain(external_worktrees.iter().map(|p| p.to_string_lossy().to_string()))
        .collect();

    if !new_wt_paths.is_empty() {
        let mut repair_args: Vec<&str> = vec!["worktree", "repair"];
        for p in &new_wt_paths {
            repair_args.push(p.as_str());
        }
        git::git(&new_bare_dir, &repair_args)
            .context("git worktree repair failed")?;
    }

    // 7e. Clean up: remove old trees/ directory inside the new bare dir if empty
    let old_trees_dir = new_bare_dir.join(&ctx.worktree_folder);
    if old_trees_dir.exists() {
        let _ = std::fs::remove_dir(&old_trees_dir); // ignore error if not empty
    }

    // 7f. Verify with git worktree list
    let verify_output = git::git(&new_bare_dir, &["worktree", "list"])
        .context("verification failed: git worktree list")?;
    output::info("Verification:");
    for line in verify_output.lines() {
        output::info(&format!("  {}", line));
    }

    // Step 8: Print success summary
    let internal_count = internal_worktrees.len();
    let external_count = external_worktrees.len();
    output::success(&format!("Migration complete: {} -> modern layout", ctx.project_name));
    output::info(&format!("  {} worktree(s) moved", internal_count));
    if external_count > 0 {
        output::info(&format!("  {} external worktree(s) repaired in-place", external_count));
    }

    Ok(())
}
