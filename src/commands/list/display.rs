use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;

use crate::git::main_relationship::MainRelationship;
use crate::git::operations::ActiveOperation;
use crate::output;

use super::collect::WorktreeInfo;

/// Print the rich terminal output for all worktrees.
pub fn print_rich(
    infos: &[WorktreeInfo],
    project_name: &str,
    short: bool,
    no_path: bool,
    no_color: bool,
) {
    if no_color {
        owo_colors::set_override(false);
    }

    output::header(&format!("Git Worktrees for {}", project_name));
    println!();

    // Print bare entry
    for info in infos {
        if info.worktree.is_bare {
            println!(
                "[bare repository] -> {}",
                info.worktree
                    .path
                    .display()
                    .to_string()
                    .if_supports_color(Stdout, |t| t.cyan())
            );
            println!();
            break;
        }
    }

    println!(
        "{}",
        "Worktrees:".if_supports_color(Stdout, |t| t.cyan())
    );

    let non_bare: Vec<&WorktreeInfo> = infos.iter().filter(|i| !i.worktree.is_bare).collect();

    // Calculate name column width for alignment
    let max_name_width = non_bare
        .iter()
        .map(|i| {
            let branch = i.worktree.branch.as_deref().unwrap_or("(detached)");
            if i.display_name == branch {
                i.display_name.len()
            } else {
                i.display_name.len() + 4 + branch.len() // " -> branch"
            }
        })
        .max()
        .unwrap_or(10);

    for info in &non_bare {
        if short {
            print_short_line(info, max_name_width);
        } else {
            print_rich_line(info, max_name_width);
        }

        if !no_path {
            println!("       {}", info.worktree.path.display());
        }
    }
}

fn print_short_line(info: &WorktreeInfo, _pad_width: usize) {
    let branch = info.worktree.branch.as_deref().unwrap_or("(detached)");

    let status_suffix = if info.status.is_clean() {
        String::new()
    } else {
        format!(
            " {}",
            info.status
                .symbols()
                .if_supports_color(Stdout, |t| t.yellow())
        )
    };

    let orphan_merged = format_orphan_merged(info);

    if info.is_current {
        let star = output::STAR.if_supports_color(Stdout, |t| t.cyan());
        let tag = "[current]".if_supports_color(Stdout, |t| t.cyan());
        let arrow = output::ARROW.if_supports_color(Stdout, |t| t.cyan());
        let name_col = info
            .display_name
            .if_supports_color(Stdout, |t| t.cyan())
            .to_string();
        if info.display_name == branch {
            println!("{} {} {} {}{}{}", star, tag, arrow, name_col, status_suffix, orphan_merged);
        } else {
            let branch_col = branch.if_supports_color(Stdout, |t| t.green());
            println!(
                "{} {} {} {} -> {}{}{}",
                star, tag, arrow, name_col, branch_col, status_suffix, orphan_merged
            );
        }
    } else {
        let arrow = output::ARROW.if_supports_color(Stdout, |t| t.cyan());
        if info.display_name == branch {
            println!("  {} {}{}{}", arrow, info.display_name, status_suffix, orphan_merged);
        } else {
            let branch_col = branch.if_supports_color(Stdout, |t| t.green());
            println!(
                "  {} {} -> {}{}{}",
                arrow, info.display_name, branch_col, status_suffix, orphan_merged
            );
        }
    }
}

fn print_rich_line(info: &WorktreeInfo, pad_width: usize) {
    let branch = info.worktree.branch.as_deref().unwrap_or("(detached)");

    // Build name+branch string for padding calculation (uncolored length)
    let name_branch_len = if info.display_name == branch {
        info.display_name.len()
    } else {
        info.display_name.len() + 4 + branch.len()
    };
    let padding = " ".repeat(pad_width.saturating_sub(name_branch_len));

    // Special indicators
    let special = format_special_indicators(info);

    // Dirty column
    let dirty_col = format_dirty(&info.status);

    // Upstream column
    let upstream_col = format_upstream(&info.status);

    // Main relationship column
    let main_col = format_main_rel(&info.main_rel);

    // Age column
    let age_col = info
        .branch_info
        .as_ref()
        .map(|bi| format_age(bi.commit_age_secs))
        .unwrap_or_default();

    // Message column
    let message_col = info
        .branch_info
        .as_ref()
        .map(|bi| truncate(&bi.commit_message, 40))
        .unwrap_or_default();

    // Build the gutter + name portion
    let gutter_name = if info.is_current {
        let star = output::STAR.if_supports_color(Stdout, |t| t.cyan());
        let tag = "[current]".if_supports_color(Stdout, |t| t.cyan());
        let arrow = output::ARROW.if_supports_color(Stdout, |t| t.cyan());
        let name_col = info
            .display_name
            .if_supports_color(Stdout, |t| t.cyan())
            .to_string();
        if info.display_name == branch {
            format!("{} {} {} {}", star, tag, arrow, name_col)
        } else {
            let branch_col = branch.if_supports_color(Stdout, |t| t.green());
            format!("{} {} {} {} -> {}", star, tag, arrow, name_col, branch_col)
        }
    } else {
        let arrow = output::ARROW.if_supports_color(Stdout, |t| t.cyan());
        if info.display_name == branch {
            format!("  {} {}", arrow, info.display_name)
        } else {
            let branch_col = branch.if_supports_color(Stdout, |t| t.green());
            format!("  {} {} -> {}", arrow, info.display_name, branch_col)
        }
    };

    // Orphan/merged indicators
    let orphan_merged = format_orphan_merged(info);

    let age_dim = age_col.if_supports_color(Stdout, |t| t.dimmed());
    let msg_dim = message_col.if_supports_color(Stdout, |t| t.dimmed());

    println!(
        "{}{}{}{}  {}  {}  {}  {}  {}",
        gutter_name, padding, special, orphan_merged, dirty_col, upstream_col, main_col, age_dim, msg_dim
    );
}

fn format_orphan_merged(info: &WorktreeInfo) -> String {
    let mut parts = Vec::new();
    if info.is_orphan {
        parts.push(
            "(orphan)"
                .if_supports_color(Stdout, |t| t.red())
                .to_string(),
        );
    }
    if info.is_merged {
        parts.push(
            "(merged)"
                .if_supports_color(Stdout, |t| t.dimmed())
                .to_string(),
        );
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

fn format_special_indicators(info: &WorktreeInfo) -> String {
    let mut parts = Vec::new();

    match info.operation {
        ActiveOperation::Rebase => {
            parts.push(output::OP_REBASE.if_supports_color(Stdout, |t| t.yellow()).to_string());
        }
        ActiveOperation::Merge => {
            parts.push(output::OP_MERGE.if_supports_color(Stdout, |t| t.yellow()).to_string());
        }
        ActiveOperation::None => {}
    }

    if info.worktree.is_locked {
        parts.push(output::LOCKED.if_supports_color(Stdout, |t| t.yellow()).to_string());
    }
    if info.worktree.is_prunable {
        parts.push(output::PRUNABLE.if_supports_color(Stdout, |t| t.red()).to_string());
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(""))
    }
}

fn format_dirty(status: &crate::git::status::WorktreeStatus) -> String {
    if status.is_clean() {
        "clean"
            .if_supports_color(Stdout, |t| t.dimmed())
            .to_string()
    } else {
        format!("{:>5}", status.symbols())
            .if_supports_color(Stdout, |t| t.yellow())
            .to_string()
    }
}

fn format_upstream(status: &crate::git::status::WorktreeStatus) -> String {
    if !status.has_upstream {
        return output::UPSTREAM_NONE
            .if_supports_color(Stdout, |t| t.dimmed())
            .to_string();
    }

    match (status.ahead, status.behind) {
        (0, 0) => output::UPSTREAM_SYNC
            .if_supports_color(Stdout, |t| t.dimmed())
            .to_string(),
        (a, 0) => format!("{}{}", output::UPSTREAM_AHEAD, a)
            .if_supports_color(Stdout, |t| t.yellow())
            .to_string(),
        (0, b) => format!("{}{}", output::UPSTREAM_BEHIND, b)
            .if_supports_color(Stdout, |t| t.red())
            .to_string(),
        (_, _) => output::UPSTREAM_DIVERGE
            .if_supports_color(Stdout, |t| t.red())
            .to_string(),
    }
}

fn format_main_rel(rel: &MainRelationship) -> String {
    match rel {
        MainRelationship::IsDefault => output::MAIN_IS_DEFAULT
            .if_supports_color(Stdout, |t| t.cyan())
            .to_string(),
        MainRelationship::Orphan => output::MAIN_ORPHAN
            .if_supports_color(Stdout, |t| t.dimmed())
            .to_string(),
        MainRelationship::SameCommit => output::MAIN_SAME_COMMIT
            .if_supports_color(Stdout, |t| t.dimmed())
            .to_string(),
        MainRelationship::Integrated => output::MAIN_INTEGRATED
            .if_supports_color(Stdout, |t| t.green())
            .to_string(),
        MainRelationship::WouldConflict => output::MAIN_CONFLICT
            .if_supports_color(Stdout, |t| t.red())
            .to_string(),
        MainRelationship::Diverged { .. } => output::MAIN_DIVERGE
            .if_supports_color(Stdout, |t| t.yellow())
            .to_string(),
        MainRelationship::Ahead(n) => format!("{}{}", output::MAIN_AHEAD, n)
            .if_supports_color(Stdout, |t| t.yellow())
            .to_string(),
        MainRelationship::Behind(n) => format!("{}{}", output::MAIN_BEHIND, n)
            .if_supports_color(Stdout, |t| t.dimmed())
            .to_string(),
    }
}

/// Format an age-in-seconds value as relative age (e.g., "3d ago").
///
/// `age_secs` is the number of seconds since the commit was made
/// (already computed as `now - committer_timestamp` in BranchInfo).
fn format_age(age_secs: i64) -> String {
    if age_secs < 0 {
        return "now".to_string();
    }

    let minutes = age_secs / 60;
    let hours = age_secs / 3600;
    let days = age_secs / 86400;
    let weeks = age_secs / 604800;
    let months = age_secs / 2592000;

    if minutes < 1 {
        "now".to_string()
    } else if hours < 1 {
        format!("{}m ago", minutes)
    } else if days < 1 {
        format!("{}h ago", hours)
    } else if weeks < 1 {
        format!("{}d ago", days)
    } else if months < 1 {
        format!("{}w ago", weeks)
    } else {
        format!("{}mo ago", months)
    }
}

/// Truncate a string to max_len characters, adding "..." if truncated.
/// Uses char_indices for Unicode safety.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max_len.saturating_sub(3))
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_age_now() {
        assert_eq!(format_age(10), "now"); // less than 60 seconds
    }

    #[test]
    fn format_age_minutes() {
        assert_eq!(format_age(300), "5m ago");
    }

    #[test]
    fn format_age_hours() {
        assert_eq!(format_age(7200), "2h ago");
    }

    #[test]
    fn format_age_days() {
        assert_eq!(format_age(259200), "3d ago");
    }

    #[test]
    fn format_age_weeks() {
        assert_eq!(format_age(1209600), "2w ago");
    }

    #[test]
    fn format_age_months() {
        assert_eq!(format_age(5184000), "2mo ago");
    }

    #[test]
    fn format_age_negative() {
        assert_eq!(format_age(-1000), "now");
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world this is long", 10), "hello w...");
    }

    #[test]
    fn format_dirty_clean() {
        let status = crate::git::status::WorktreeStatus::default();
        // With NO_COLOR, should contain "clean"
        owo_colors::set_override(false);
        let result = format_dirty(&status);
        assert!(result.contains("clean"));
        owo_colors::unset_override();
    }

    #[test]
    fn format_dirty_all_flags() {
        let status = crate::git::status::WorktreeStatus {
            has_staged: true,
            has_modified: true,
            has_untracked: true,
            ..Default::default()
        };
        owo_colors::set_override(false);
        let result = format_dirty(&status);
        assert!(result.contains("+!?"));
        owo_colors::unset_override();
    }

    #[test]
    fn format_upstream_no_upstream() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: false,
            ..Default::default()
        };
        owo_colors::set_override(false);
        let result = format_upstream(&status);
        assert!(result.contains("\u{2014}")); // em dash
        owo_colors::unset_override();
    }

    #[test]
    fn format_upstream_in_sync() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: true,
            ahead: 0,
            behind: 0,
            ..Default::default()
        };
        owo_colors::set_override(false);
        let result = format_upstream(&status);
        assert!(result.contains("|"));
        owo_colors::unset_override();
    }

    #[test]
    fn format_upstream_ahead() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: true,
            ahead: 3,
            behind: 0,
            ..Default::default()
        };
        owo_colors::set_override(false);
        let result = format_upstream(&status);
        assert!(result.contains("3"));
        owo_colors::unset_override();
    }

    #[test]
    fn format_main_rel_is_default() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::IsDefault);
        assert!(result.contains("^"));
        owo_colors::unset_override();
    }

    #[test]
    fn format_main_rel_ahead() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::Ahead(5));
        assert!(result.contains("5"));
        owo_colors::unset_override();
    }

    #[test]
    fn format_special_no_indicators() {
        let info = make_test_info(ActiveOperation::None, false, false);
        assert_eq!(format_special_indicators(&info), "");
    }

    #[test]
    fn format_special_rebase() {
        let info = make_test_info(ActiveOperation::Rebase, false, false);
        owo_colors::set_override(false);
        let result = format_special_indicators(&info);
        assert!(result.contains("\u{2934}")); // OP_REBASE
        owo_colors::unset_override();
    }

    #[test]
    fn format_special_locked() {
        let info = make_test_info(ActiveOperation::None, true, false);
        owo_colors::set_override(false);
        let result = format_special_indicators(&info);
        assert!(result.contains("\u{229E}")); // LOCKED
        owo_colors::unset_override();
    }

    #[test]
    fn format_special_prunable() {
        let info = make_test_info(ActiveOperation::None, false, true);
        owo_colors::set_override(false);
        let result = format_special_indicators(&info);
        assert!(result.contains("\u{229F}")); // PRUNABLE
        owo_colors::unset_override();
    }

    #[test]
    fn format_orphan_merged_orphan() {
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_orphan = true;
        owo_colors::set_override(false);
        let result = format_orphan_merged(&info);
        assert!(result.contains("(orphan)"), "Should contain (orphan), got: {}", result);
        owo_colors::unset_override();
    }

    #[test]
    fn format_orphan_merged_merged() {
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_merged = true;
        owo_colors::set_override(false);
        let result = format_orphan_merged(&info);
        assert!(result.contains("(merged)"), "Should contain (merged), got: {}", result);
        owo_colors::unset_override();
    }

    #[test]
    fn format_orphan_merged_both_false_returns_empty() {
        let info = make_test_info(ActiveOperation::None, false, false);
        let result = format_orphan_merged(&info);
        assert!(result.is_empty(), "Should be empty when both false, got: '{}'", result);
    }

    #[test]
    fn format_orphan_merged_both_true() {
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_orphan = true;
        info.is_merged = true;
        owo_colors::set_override(false);
        let result = format_orphan_merged(&info);
        assert!(result.contains("(orphan)"), "Should contain (orphan)");
        assert!(result.contains("(merged)"), "Should contain (merged)");
        owo_colors::unset_override();
    }

    // --- print_short_line tests ---

    #[test]
    fn print_short_line_current_same_branch() {
        owo_colors::set_override(false);
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_current = true;
        info.display_name = "main".to_string();
        info.worktree.branch = Some("main".to_string());
        // Just exercise the code path — no panic = pass
        print_short_line(&info, 20);
        owo_colors::unset_override();
    }

    #[test]
    fn print_short_line_current_different_branch() {
        owo_colors::set_override(false);
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_current = true;
        info.display_name = "feature".to_string();
        info.worktree.branch = Some("feat/x".to_string());
        print_short_line(&info, 20);
        owo_colors::unset_override();
    }

    #[test]
    fn print_short_line_non_current_different_branch() {
        owo_colors::set_override(false);
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_current = false;
        info.display_name = "feature".to_string();
        info.worktree.branch = Some("feat/x".to_string());
        print_short_line(&info, 20);
        owo_colors::unset_override();
    }

    #[test]
    fn print_short_line_dirty_status() {
        owo_colors::set_override(false);
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.status.has_modified = true;
        print_short_line(&info, 20);
        owo_colors::unset_override();
    }

    // --- print_rich_line tests ---

    #[test]
    fn print_rich_line_current_different_branch() {
        owo_colors::set_override(false);
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_current = true;
        info.display_name = "feature".to_string();
        info.worktree.branch = Some("feat/x".to_string());
        print_rich_line(&info, 30);
        owo_colors::unset_override();
    }

    #[test]
    fn print_rich_line_non_current_same_branch() {
        owo_colors::set_override(false);
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.is_current = false;
        info.display_name = "main".to_string();
        info.worktree.branch = Some("main".to_string());
        print_rich_line(&info, 30);
        owo_colors::unset_override();
    }

    #[test]
    fn print_rich_line_with_branch_info() {
        owo_colors::set_override(false);
        let mut info = make_test_info(ActiveOperation::None, false, false);
        info.branch_info = Some(crate::git::branch_info::BranchInfo {
            upstream: None,
            commit_age_secs: 3600,
            commit_message: "fix bug".to_string(),
        });
        print_rich_line(&info, 30);
        owo_colors::unset_override();
    }

    // --- format_upstream variant tests ---

    #[test]
    fn format_upstream_behind() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: true,
            ahead: 0,
            behind: 3,
            ..Default::default()
        };
        owo_colors::set_override(false);
        let result = format_upstream(&status);
        assert!(result.contains("3"), "Should contain behind count, got: {}", result);
        owo_colors::unset_override();
    }

    #[test]
    fn format_upstream_diverged() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: true,
            ahead: 2,
            behind: 3,
            ..Default::default()
        };
        owo_colors::set_override(false);
        let result = format_upstream(&status);
        // Should use UPSTREAM_DIVERGE symbol
        assert!(!result.is_empty(), "Should produce non-empty result");
        owo_colors::unset_override();
    }

    // --- format_main_rel variant tests ---

    #[test]
    fn format_main_rel_orphan() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::Orphan);
        assert!(!result.is_empty());
        owo_colors::unset_override();
    }

    #[test]
    fn format_main_rel_same_commit() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::SameCommit);
        assert!(result.contains("_"));
        owo_colors::unset_override();
    }

    #[test]
    fn format_main_rel_integrated() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::Integrated);
        assert!(!result.is_empty());
        owo_colors::unset_override();
    }

    #[test]
    fn format_main_rel_conflict() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::WouldConflict);
        assert!(!result.is_empty());
        owo_colors::unset_override();
    }

    #[test]
    fn format_main_rel_diverged() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::Diverged { ahead: 2, behind: 3 });
        assert!(!result.is_empty());
        owo_colors::unset_override();
    }

    #[test]
    fn format_main_rel_behind() {
        owo_colors::set_override(false);
        let result = format_main_rel(&MainRelationship::Behind(4));
        assert!(result.contains("4"));
        owo_colors::unset_override();
    }

    // Helper to create a minimal WorktreeInfo for display tests
    fn make_test_info(
        op: ActiveOperation,
        locked: bool,
        prunable: bool,
    ) -> WorktreeInfo {
        use crate::git::worktree::Worktree;
        use std::path::PathBuf;

        WorktreeInfo {
            worktree: Worktree {
                path: PathBuf::from("/test/wt"),
                head: Some("abc123".to_string()),
                branch: Some("main".to_string()),
                is_bare: false,
                is_detached: false,
                is_locked: locked,
                lock_reason: None,
                is_prunable: prunable,
            },
            status: crate::git::status::WorktreeStatus::default(),
            main_rel: MainRelationship::IsDefault,
            operation: op,
            branch_info: None,
            line_diff_head: None,
            line_diff_main: None,
            is_current: false,
            display_name: "main".to_string(),
            is_orphan: false,
            is_merged: false,
        }
    }
}
