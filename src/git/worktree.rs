use std::path::{Path, PathBuf};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    pub lock_reason: Option<String>,
    pub is_prunable: bool,
}

/// Parse `git worktree list --porcelain` output into Vec<Worktree>.
/// Records are separated by blank lines. Each record starts with "worktree <path>".
pub fn parse_porcelain(output: &str) -> Vec<Worktree> {
    let mut worktrees = Vec::new();

    // Track current worktree being built
    let mut current_path: Option<PathBuf> = None;
    let mut head: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut is_bare = false;
    let mut is_detached = false;
    let mut is_locked = false;
    let mut lock_reason: Option<String> = None;
    let mut is_prunable = false;

    let flush = |worktrees: &mut Vec<Worktree>,
                 path: &mut Option<PathBuf>,
                 head: &mut Option<String>,
                 branch: &mut Option<String>,
                 is_bare: &mut bool,
                 is_detached: &mut bool,
                 is_locked: &mut bool,
                 lock_reason: &mut Option<String>,
                 is_prunable: &mut bool| {
        if let Some(p) = path.take() {
            worktrees.push(Worktree {
                path: p,
                head: head.take(),
                branch: branch.take(),
                is_bare: *is_bare,
                is_detached: *is_detached,
                is_locked: *is_locked,
                lock_reason: lock_reason.take(),
                is_prunable: *is_prunable,
            });
            *is_bare = false;
            *is_detached = false;
            *is_locked = false;
            *is_prunable = false;
        }
    };

    for line in output.lines() {
        if line.is_empty() {
            // Blank line separates records
            flush(
                &mut worktrees,
                &mut current_path,
                &mut head,
                &mut branch,
                &mut is_bare,
                &mut is_detached,
                &mut is_locked,
                &mut lock_reason,
                &mut is_prunable,
            );
            continue;
        }

        if let Some(path_str) = line.strip_prefix("worktree ") {
            // Start of a new record -- flush previous if not yet flushed
            flush(
                &mut worktrees,
                &mut current_path,
                &mut head,
                &mut branch,
                &mut is_bare,
                &mut is_detached,
                &mut is_locked,
                &mut lock_reason,
                &mut is_prunable,
            );
            current_path = Some(PathBuf::from(path_str));
        } else if let Some(sha) = line.strip_prefix("HEAD ") {
            head = Some(sha.to_string());
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            // Strip refs/heads/ prefix if present
            let short = branch_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(branch_ref);
            branch = Some(short.to_string());
        } else if line == "bare" {
            is_bare = true;
        } else if line == "detached" {
            is_detached = true;
        } else if line == "locked" {
            is_locked = true;
        } else if let Some(reason) = line.strip_prefix("locked ") {
            is_locked = true;
            lock_reason = Some(reason.to_string());
        } else if line == "prunable" || line.starts_with("prunable ") {
            is_prunable = true;
        }
    }

    // Flush any remaining record (handles output without trailing blank line)
    flush(
        &mut worktrees,
        &mut current_path,
        &mut head,
        &mut branch,
        &mut is_bare,
        &mut is_detached,
        &mut is_locked,
        &mut lock_reason,
        &mut is_prunable,
    );

    worktrees
}

/// List worktrees by running git worktree list --porcelain.
/// Falls back to error message if --porcelain fails (D-09, D-10).
pub fn list_worktrees(project_dir: &Path) -> Result<Vec<Worktree>> {
    match super::git(project_dir, &["worktree", "list", "--porcelain"]) {
        Ok(output) => Ok(parse_porcelain(&output)),
        Err(_) => {
            anyhow::bail!(
                "Failed to list worktrees. Your git version may not support --porcelain.\n   \
                 Please upgrade git to version 2.7 or later."
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_WORKTREE_OUTPUT: &str = "\
worktree /home/user/project/.bare
HEAD abc1234567890abcdef1234567890abcdef123456
bare

worktree /home/user/project/trees/main
HEAD def4567890abcdef1234567890abcdef12345678
branch refs/heads/main

";

    #[test]
    fn parse_porcelain_two_worktrees_returns_vec_len_2() {
        let result = parse_porcelain(TWO_WORKTREE_OUTPUT);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_porcelain_first_entry_bare_has_is_bare_true() {
        let result = parse_porcelain(TWO_WORKTREE_OUTPUT);
        assert!(result[0].is_bare, "First entry with 'bare' flag should have is_bare=true");
        assert_eq!(result[0].branch, None, "Bare entry should have no branch");
    }

    #[test]
    fn parse_porcelain_second_entry_has_correct_fields() {
        let result = parse_porcelain(TWO_WORKTREE_OUTPUT);
        assert_eq!(result[1].path, PathBuf::from("/home/user/project/trees/main"));
        assert_eq!(result[1].head, Some("def4567890abcdef1234567890abcdef12345678".to_string()));
        assert_eq!(result[1].branch, Some("main".to_string()));
    }

    #[test]
    fn parse_porcelain_strips_refs_heads_from_branch() {
        let result = parse_porcelain(TWO_WORKTREE_OUTPUT);
        assert_eq!(result[1].branch, Some("main".to_string()),
            "Branch should be 'main' not 'refs/heads/main'");
    }

    #[test]
    fn parse_porcelain_detached_head() {
        let output = "\
worktree /home/user/project/trees/detached
HEAD abc1234567890abcdef1234567890abcdef123456
detached

";
        let result = parse_porcelain(output);
        assert_eq!(result.len(), 1);
        assert!(result[0].is_detached, "Detached entry should have is_detached=true");
        assert_eq!(result[0].branch, None, "Detached entry should have no branch");
    }

    #[test]
    fn parse_porcelain_locked_worktree() {
        let output = "\
worktree /home/user/project/trees/locked-wt
HEAD abc1234567890abcdef1234567890abcdef123456
branch refs/heads/feature/locked
locked

";
        let result = parse_porcelain(output);
        assert_eq!(result.len(), 1);
        assert!(result[0].is_locked, "Locked entry should have is_locked=true");
    }

    #[test]
    fn parse_porcelain_locked_with_reason() {
        let output = "\
worktree /home/user/project/trees/locked-wt
HEAD abc1234567890abcdef1234567890abcdef123456
branch refs/heads/feature/locked
locked reason text

";
        let result = parse_porcelain(output);
        assert_eq!(result.len(), 1);
        assert!(result[0].is_locked, "Locked entry should have is_locked=true");
        assert_eq!(result[0].lock_reason, Some("reason text".to_string()));
    }

    #[test]
    fn parse_porcelain_empty_input() {
        let result = parse_porcelain("");
        assert!(result.is_empty(), "Empty input should return empty vec");
    }

    #[test]
    fn parse_porcelain_no_trailing_blank_line() {
        let output = "\
worktree /home/user/project/trees/main
HEAD abc1234567890abcdef1234567890abcdef123456
branch refs/heads/main";
        let result = parse_porcelain(output);
        assert_eq!(result.len(), 1, "Should handle output without trailing blank line");
        assert_eq!(result[0].branch, Some("main".to_string()));
    }

    #[test]
    fn parse_porcelain_prunable_worktree() {
        let output = "\
worktree /home/user/project/trees/prunable-wt
HEAD abc1234567890abcdef1234567890abcdef123456
branch refs/heads/feature/old
prunable gitdir file points to non-existent location

";
        let result = parse_porcelain(output);
        assert_eq!(result.len(), 1);
        assert!(result[0].is_prunable, "Prunable entry should have is_prunable=true");
    }

    #[test]
    fn parse_porcelain_nested_branch_name() {
        let output = "\
worktree /home/user/project/trees/auth
HEAD abc1234567890abcdef1234567890abcdef123456
branch refs/heads/feature/auth/login

";
        let result = parse_porcelain(output);
        assert_eq!(result[0].branch, Some("feature/auth/login".to_string()),
            "Should strip only refs/heads/ prefix, preserving nested path");
    }
}
