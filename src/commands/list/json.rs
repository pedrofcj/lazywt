use anyhow::Result;
use serde::Serialize;

use crate::git::main_relationship::MainRelationship;
use crate::git::operations::ActiveOperation;

use super::collect::WorktreeInfo;

#[derive(Serialize)]
struct JsonWorktree {
    name: String,
    branch: Option<String>,
    path: String,
    is_current: bool,
    dirty: JsonDirty,
    upstream: Option<JsonUpstream>,
    main_relationship: String,
    operation: Option<String>,
    commit: Option<JsonCommit>,
    line_diff_head: Option<JsonLineDiff>,
    line_diff_main: Option<JsonLineDiff>,
    is_locked: bool,
    is_prunable: bool,
    is_orphan: bool,
    is_merged: bool,
}

#[derive(Serialize)]
struct JsonDirty {
    staged: bool,
    modified: bool,
    untracked: bool,
}

#[derive(Serialize)]
struct JsonUpstream {
    ahead: u32,
    behind: u32,
    tracking: Option<String>,
}

#[derive(Serialize)]
struct JsonCommit {
    age_secs: i64,
    message: String,
}

#[derive(Serialize)]
struct JsonLineDiff {
    insertions: u32,
    deletions: u32,
}

fn main_rel_string(rel: &MainRelationship) -> String {
    match rel {
        MainRelationship::IsDefault => "is_default".to_string(),
        MainRelationship::Orphan => "orphan".to_string(),
        MainRelationship::SameCommit => "same_commit".to_string(),
        MainRelationship::Integrated => "integrated".to_string(),
        MainRelationship::WouldConflict => "would_conflict".to_string(),
        MainRelationship::Diverged { ahead, behind } => {
            format!("diverged_{}_{}", ahead, behind)
        }
        MainRelationship::Ahead(n) => format!("ahead_{}", n),
        MainRelationship::Behind(n) => format!("behind_{}", n),
    }
}

fn operation_string(op: &ActiveOperation) -> Option<String> {
    match op {
        ActiveOperation::None => None,
        ActiveOperation::Rebase => Some("rebase".to_string()),
        ActiveOperation::Merge => Some("merge".to_string()),
    }
}

fn to_json_worktree(info: &WorktreeInfo) -> JsonWorktree {
    JsonWorktree {
        name: info.display_name.clone(),
        branch: info.worktree.branch.clone(),
        path: info.worktree.path.display().to_string(),
        is_current: info.is_current,
        dirty: JsonDirty {
            staged: info.status.has_staged,
            modified: info.status.has_modified,
            untracked: info.status.has_untracked,
        },
        upstream: if info.status.has_upstream {
            Some(JsonUpstream {
                ahead: info.status.ahead,
                behind: info.status.behind,
                tracking: info
                    .branch_info
                    .as_ref()
                    .and_then(|bi| bi.upstream.clone()),
            })
        } else {
            None
        },
        main_relationship: main_rel_string(&info.main_rel),
        operation: operation_string(&info.operation),
        commit: info.branch_info.as_ref().map(|bi| JsonCommit {
            age_secs: bi.commit_age_secs,
            message: bi.commit_message.clone(),
        }),
        line_diff_head: info.line_diff_head.as_ref().map(|ld| JsonLineDiff {
            insertions: ld.insertions,
            deletions: ld.deletions,
        }),
        line_diff_main: info.line_diff_main.as_ref().map(|ld| JsonLineDiff {
            insertions: ld.insertions,
            deletions: ld.deletions,
        }),
        is_locked: info.worktree.is_locked,
        is_prunable: info.worktree.is_prunable,
        is_orphan: info.is_orphan,
        is_merged: info.is_merged,
    }
}

/// Print all worktrees as a JSON array to stdout.
pub fn print_json(infos: &[WorktreeInfo], _project_name: &str) -> Result<()> {
    let json_items: Vec<JsonWorktree> = infos
        .iter()
        .filter(|i| !i.worktree.is_bare)
        .map(to_json_worktree)
        .collect();

    let json_str = serde_json::to_string_pretty(&json_items)?;
    println!("{}", json_str);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::main_relationship::MainRelationship;
    use crate::git::operations::ActiveOperation;
    use crate::git::status::WorktreeStatus;
    use crate::git::worktree::Worktree;
    use std::path::PathBuf;

    fn make_info() -> WorktreeInfo {
        WorktreeInfo {
            worktree: Worktree {
                path: PathBuf::from("/test/main"),
                head: Some("abc123".to_string()),
                branch: Some("main".to_string()),
                is_bare: false,
                is_detached: false,
                is_locked: false,
                lock_reason: None,
                is_prunable: false,
            },
            status: WorktreeStatus {
                has_staged: true,
                has_modified: false,
                has_untracked: true,
                ahead: 2,
                behind: 0,
                has_upstream: true,
            },
            main_rel: MainRelationship::IsDefault,
            operation: ActiveOperation::None,
            branch_info: Some(crate::git::branch_info::BranchInfo {
                upstream: Some("origin/main".to_string()),
                commit_age_secs: 1711929600,
                commit_message: "fix: auth bug".to_string(),
            }),
            line_diff_head: Some(crate::git::line_diff::LineDiff {
                insertions: 10,
                deletions: 3,
            }),
            line_diff_main: None,
            is_current: true,
            display_name: "main".to_string(),
            is_orphan: false,
            is_merged: false,
        }
    }

    #[test]
    fn json_serialization_roundtrip() {
        let info = make_info();
        let json_wt = to_json_worktree(&info);
        let json_str = serde_json::to_string(&json_wt).unwrap();
        assert!(json_str.contains("\"name\":\"main\""));
        assert!(json_str.contains("\"is_current\":true"));
        assert!(json_str.contains("\"staged\":true"));
        assert!(json_str.contains("\"ahead\":2"));
    }

    #[test]
    fn json_main_rel_strings() {
        assert_eq!(main_rel_string(&MainRelationship::IsDefault), "is_default");
        assert_eq!(main_rel_string(&MainRelationship::Orphan), "orphan");
        assert_eq!(main_rel_string(&MainRelationship::Ahead(5)), "ahead_5");
        assert_eq!(main_rel_string(&MainRelationship::Behind(3)), "behind_3");
        assert_eq!(
            main_rel_string(&MainRelationship::Diverged {
                ahead: 2,
                behind: 1
            }),
            "diverged_2_1"
        );
    }

    #[test]
    fn json_operation_strings() {
        assert_eq!(operation_string(&ActiveOperation::None), None);
        assert_eq!(
            operation_string(&ActiveOperation::Rebase),
            Some("rebase".to_string())
        );
        assert_eq!(
            operation_string(&ActiveOperation::Merge),
            Some("merge".to_string())
        );
    }

    #[test]
    fn json_no_upstream_is_null() {
        let mut info = make_info();
        info.status.has_upstream = false;
        let json_wt = to_json_worktree(&info);
        assert!(json_wt.upstream.is_none());
    }

    #[test]
    fn json_orphan_merged_fields_default() {
        let info = make_info();
        let json_wt = to_json_worktree(&info);
        let json_str = serde_json::to_string(&json_wt).unwrap();
        assert!(json_str.contains("\"is_orphan\":false"), "Default info should have is_orphan=false");
        assert!(json_str.contains("\"is_merged\":false"), "Default info should have is_merged=false");
    }

    #[test]
    fn json_orphan_true() {
        let mut info = make_info();
        info.is_orphan = true;
        let json_wt = to_json_worktree(&info);
        let json_str = serde_json::to_string(&json_wt).unwrap();
        assert!(json_str.contains("\"is_orphan\":true"), "Orphan info should have is_orphan=true");
    }

    #[test]
    fn json_merged_true() {
        let mut info = make_info();
        info.is_merged = true;
        let json_wt = to_json_worktree(&info);
        let json_str = serde_json::to_string(&json_wt).unwrap();
        assert!(json_str.contains("\"is_merged\":true"), "Merged info should have is_merged=true");
    }

    #[test]
    fn json_bare_worktrees_filtered() {
        let bare = WorktreeInfo {
            worktree: Worktree {
                path: PathBuf::from("/test/.bare"),
                head: None,
                branch: None,
                is_bare: true,
                is_detached: false,
                is_locked: false,
                lock_reason: None,
                is_prunable: false,
            },
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
        let infos = vec![bare, make_info()];
        let filtered: Vec<JsonWorktree> = infos
            .iter()
            .filter(|i| !i.worktree.is_bare)
            .map(to_json_worktree)
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "main");
    }
}
