use std::path::Path;

use anyhow::Result;

use crate::git::branch_info::{batch_branch_info, BranchInfo};
use crate::git::line_diff::{line_diff_head, line_diff_refs, LineDiff};
use crate::git::main_relationship::{
    check_merge_tree_support, detect_main_relationship, MainRelationship,
};
use crate::git::operations::{detect_operation, ActiveOperation};
use crate::git::status::{extended_worktree_status, worktree_status, WorktreeStatus};
use crate::git::worktree::{list_worktrees, Worktree};

/// All collected information about a single worktree.
pub struct WorktreeInfo {
    pub worktree: Worktree,
    pub status: WorktreeStatus,
    pub main_rel: MainRelationship,
    pub operation: ActiveOperation,
    pub branch_info: Option<BranchInfo>,
    pub line_diff_head: Option<LineDiff>,
    pub line_diff_main: Option<LineDiff>,
    pub is_current: bool,
    pub display_name: String,
    pub is_orphan: bool,
    pub is_merged: bool,
}

/// Collect full status info for all worktrees.
///
/// Two-phase collection: batch (one git call for all branches),
/// then per-worktree sequential calls.
pub fn collect_all(
    project_dir: &Path,
    current_path: Option<&Path>,
    short: bool,
) -> Result<(Vec<WorktreeInfo>, String)> {
    let worktrees = list_worktrees(project_dir)?;

    if short {
        return collect_short(worktrees, current_path);
    }

    // Batch phase
    let branch_map = batch_branch_info(project_dir).unwrap_or_default();
    let default_branch = crate::git::branch::detect_default_branch(project_dir, false)
        .ok()
        .flatten()
        .unwrap_or_else(|| "main".to_string());
    let supports_merge_tree = check_merge_tree_support(project_dir);

    // Collect merged branches for (merged) indicator (D-82)
    let merged_branches: std::collections::HashSet<String> = crate::git::git(
        project_dir,
        &["branch", "--merged", &default_branch, "--format=%(refname:short)"],
    )
    .map(|output| output.lines().map(|l| l.trim().to_string()).collect())
    .unwrap_or_default();

    // Get default branch HEAD for main_relationship detection
    let default_head = crate::git::git(project_dir, &["rev-parse", &default_branch])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let mut infos = Vec::new();

    for wt in worktrees {
        if wt.is_bare {
            infos.push(WorktreeInfo {
                display_name: String::new(),
                is_current: false,
                status: WorktreeStatus::default(),
                main_rel: MainRelationship::IsDefault,
                operation: ActiveOperation::None,
                branch_info: None,
                line_diff_head: None,
                line_diff_main: None,
                is_orphan: false,
                is_merged: false,
                worktree: wt,
            });
            continue;
        }

        let display_name = wt
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let is_current = current_path
            .and_then(|cwd| {
                dunce::canonicalize(&wt.path)
                    .ok()
                    .map(|canon| cwd.starts_with(&canon))
            })
            .unwrap_or(false);

        // Per-worktree: skip git calls for prunable worktrees (directory missing)
        if wt.is_prunable {
            infos.push(WorktreeInfo {
                display_name,
                is_current,
                status: WorktreeStatus::default(),
                main_rel: MainRelationship::Orphan,
                operation: ActiveOperation::None,
                branch_info: wt.branch.as_ref().and_then(|b| branch_map.get(b).cloned()),
                line_diff_head: None,
                line_diff_main: None,
                is_orphan: true,
                is_merged: false,
                worktree: wt,
            });
            continue;
        }

        let status = extended_worktree_status(&wt.path).unwrap_or_default();

        let branch_name = wt.branch.as_deref().unwrap_or("");
        let branch_head = wt.head.as_deref().unwrap_or("");

        let main_rel = if branch_head.is_empty() || default_head.is_empty() {
            MainRelationship::Orphan
        } else {
            detect_main_relationship(
                project_dir,
                branch_name,
                branch_head,
                &default_branch,
                &default_head,
                supports_merge_tree,
            )
        };

        let operation = detect_operation(&wt.path);

        let ldh = line_diff_head(&wt.path).ok();

        let ldm = if branch_name != default_branch && !default_head.is_empty() {
            let range = format!("{}...{}", default_branch, branch_head);
            line_diff_refs(project_dir, &range).ok()
        } else {
            None
        };

        let branch_info = wt.branch.as_ref().and_then(|b| branch_map.get(b).cloned());

        let is_orphan = !wt.path.exists();
        let is_merged = {
            let bname = wt.branch.as_deref().unwrap_or("");
            if bname.is_empty() || bname == default_branch || !merged_branches.contains(bname) {
                false
            } else {
                // Exclude false-positive: fresh branches at the exact same
                // commit as default (no work done yet). Compare commit SHAs
                // directly instead of rev-list --count, because after a real
                // merge all feature commits become reachable from default,
                // making rev-list --count 0 for both fresh and merged branches.
                let branch_sha = crate::git::git(project_dir, &["rev-parse", bname])
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                // default_head is already computed above
                branch_sha != default_head
            }
        };

        infos.push(WorktreeInfo {
            display_name,
            is_current,
            status,
            main_rel,
            operation,
            branch_info,
            line_diff_head: ldh,
            line_diff_main: ldm,
            is_orphan,
            is_merged,
            worktree: wt,
        });
    }

    Ok((infos, default_branch))
}

/// Short mode: only collect dirty status (fast path).
fn collect_short(
    worktrees: Vec<Worktree>,
    current_path: Option<&Path>,
) -> Result<(Vec<WorktreeInfo>, String)> {
    let mut infos = Vec::new();

    for wt in worktrees {
        let display_name = wt
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let is_current = if wt.is_bare {
            false
        } else {
            current_path
                .and_then(|cwd| {
                    dunce::canonicalize(&wt.path)
                        .ok()
                        .map(|canon| cwd.starts_with(&canon))
                })
                .unwrap_or(false)
        };

        let status = if wt.is_bare || wt.is_prunable {
            WorktreeStatus::default()
        } else {
            worktree_status(&wt.path).unwrap_or_default()
        };

        let is_orphan = wt.is_prunable || (!wt.is_bare && !wt.path.exists());

        infos.push(WorktreeInfo {
            display_name,
            is_current,
            status,
            main_rel: MainRelationship::IsDefault,
            operation: ActiveOperation::None,
            branch_info: None,
            line_diff_head: None,
            line_diff_main: None,
            is_orphan,
            is_merged: false,
            worktree: wt,
        });
    }

    Ok((infos, String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_worktree(path: PathBuf, branch: Option<&str>, is_bare: bool, is_prunable: bool) -> Worktree {
        Worktree {
            path,
            head: Some("abc123".to_string()),
            branch: branch.map(|s| s.to_string()),
            is_bare,
            is_detached: false,
            is_locked: false,
            lock_reason: None,
            is_prunable,
        }
    }

    #[test]
    fn worktree_info_has_is_orphan_field() {
        let info = WorktreeInfo {
            worktree: make_worktree(PathBuf::from("/nonexistent/path"), Some("feat"), false, false),
            status: WorktreeStatus::default(),
            main_rel: MainRelationship::IsDefault,
            operation: ActiveOperation::None,
            branch_info: None,
            line_diff_head: None,
            line_diff_main: None,
            is_current: false,
            display_name: "feat".to_string(),
            is_orphan: true,
            is_merged: false,
        };
        assert!(info.is_orphan, "WorktreeInfo with missing path should have is_orphan = true");
    }

    #[test]
    fn worktree_info_has_is_merged_field() {
        let info = WorktreeInfo {
            worktree: make_worktree(PathBuf::from("/test"), Some("feat"), false, false),
            status: WorktreeStatus::default(),
            main_rel: MainRelationship::IsDefault,
            operation: ActiveOperation::None,
            branch_info: None,
            line_diff_head: None,
            line_diff_main: None,
            is_current: false,
            display_name: "feat".to_string(),
            is_orphan: false,
            is_merged: true,
        };
        assert!(info.is_merged, "WorktreeInfo with merged branch should have is_merged = true");
    }

    #[test]
    fn bare_worktree_has_orphan_false_merged_false() {
        let info = WorktreeInfo {
            worktree: make_worktree(PathBuf::from("/bare"), None, true, false),
            status: WorktreeStatus::default(),
            main_rel: MainRelationship::IsDefault,
            operation: ActiveOperation::None,
            branch_info: None,
            line_diff_head: None,
            line_diff_main: None,
            is_current: false,
            display_name: String::new(),
            is_orphan: false,
            is_merged: false,
        };
        assert!(!info.is_orphan, "Bare worktree should not be orphan");
        assert!(!info.is_merged, "Bare worktree should not be merged");
    }

    #[test]
    fn collect_short_sets_orphan_for_prunable_worktree() {
        // Create a worktree that is prunable (missing directory)
        let wt = make_worktree(PathBuf::from("/nonexistent/prunable-wt"), Some("old-branch"), false, true);
        let result = collect_short(vec![wt], None).unwrap();
        assert!(result.0[0].is_orphan, "Prunable worktree in short mode should be orphan");
        assert!(!result.0[0].is_merged, "Short mode should not compute merged status");
    }

    #[test]
    fn collect_short_sets_orphan_for_missing_path() {
        // Non-prunable but path doesn't exist
        let wt = make_worktree(PathBuf::from("/nonexistent/missing-dir"), Some("feature"), false, false);
        let result = collect_short(vec![wt], None).unwrap();
        assert!(result.0[0].is_orphan, "Worktree with missing path should be orphan in short mode");
    }

    #[test]
    fn collect_short_bare_not_orphan() {
        let wt = make_worktree(PathBuf::from("/nonexistent/bare"), None, true, false);
        let result = collect_short(vec![wt], None).unwrap();
        assert!(!result.0[0].is_orphan, "Bare worktree should not be orphan even with nonexistent path");
    }
}
