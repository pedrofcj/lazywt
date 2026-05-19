use std::path::Path;

use anyhow::Result;

/// Flags indicating dirty state of a worktree, plus upstream tracking info.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorktreeStatus {
    pub has_staged: bool,    // '+' symbol
    pub has_modified: bool,  // '!' symbol
    pub has_untracked: bool, // '?' symbol
    pub ahead: u32,          // commits ahead of upstream
    pub behind: u32,         // commits behind upstream
    pub has_upstream: bool,  // whether tracking branch exists
}

impl WorktreeStatus {
    /// Format status flags as a compact string: "+!?" or subsets thereof.
    /// Returns empty string if clean.
    pub fn symbols(&self) -> String {
        let mut s = String::new();
        if self.has_staged {
            s.push('+');
        }
        if self.has_modified {
            s.push('!');
        }
        if self.has_untracked {
            s.push('?');
        }
        s
    }

    pub fn is_clean(&self) -> bool {
        !self.has_staged && !self.has_modified && !self.has_untracked
    }
}

/// Parse `git status --porcelain` output into a WorktreeStatus.
///
/// Porcelain format: two-character status prefix per line.
/// - First char = index (staging area) status
/// - Second char = working tree status
///
/// Rules:
/// - Line starts with "??" -> untracked file
/// - First char is not ' ' and not '?' -> has staged changes
/// - Second char is not ' ' and not '?' (and line doesn't start with "??") -> has modified files
fn parse_porcelain(output: &str) -> WorktreeStatus {
    let mut status = WorktreeStatus::default();

    for line in output.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 2 {
            continue;
        }

        let index_char = bytes[0];
        let tree_char = bytes[1];

        if index_char == b'?' && tree_char == b'?' {
            status.has_untracked = true;
        } else {
            if index_char != b' ' && index_char != b'?' {
                status.has_staged = true;
            }
            if tree_char != b' ' && tree_char != b'?' {
                status.has_modified = true;
            }
        }

        // Early exit if all flags are already set
        if status.has_staged && status.has_modified && status.has_untracked {
            break;
        }
    }

    status
}

/// Get the dirty status of a worktree by running `git status --porcelain`.
///
/// Returns a default (clean) status if the git command fails, keeping
/// the `list` command resilient to individual worktree errors.
pub fn worktree_status(path: &Path) -> Result<WorktreeStatus> {
    match super::git(path, &["status", "--porcelain"]) {
        Ok(output) => Ok(parse_porcelain(&output)),
        Err(_) => Ok(WorktreeStatus::default()),
    }
}

/// Parse `git status --porcelain=v2 --branch` output into WorktreeStatus.
///
/// Header lines start with `#`:
/// - `# branch.ab +N -M` -> ahead N, behind M
/// - `# branch.upstream ...` -> has upstream
///
/// File entry lines:
/// - Start with `1` or `2`: XY at chars [2..4], `.` = unchanged
/// - Start with `?` -> untracked
/// - Start with `u` -> unmerged (both staged and modified)
fn parse_porcelain_v2(output: &str) -> WorktreeStatus {
    let mut status = WorktreeStatus::default();

    for line in output.lines() {
        if line.starts_with("# branch.ab ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                status.ahead = parts[2].trim_start_matches('+').parse().unwrap_or(0);
                status.behind = parts[3].trim_start_matches('-').parse().unwrap_or(0);
            }
        } else if line.starts_with("# branch.upstream ") {
            status.has_upstream = true;
        } else if line.starts_with('#') {
            continue;
        } else if line.starts_with('?') {
            status.has_untracked = true;
        } else if line.starts_with('1') || line.starts_with('2') {
            let bytes = line.as_bytes();
            if bytes.len() >= 4 {
                let index_char = bytes[2];
                let tree_char = bytes[3];
                if index_char != b'.' {
                    status.has_staged = true;
                }
                if tree_char != b'.' {
                    status.has_modified = true;
                }
            }
        } else if line.starts_with('u') {
            status.has_staged = true;
            status.has_modified = true;
        }

        if status.has_staged && status.has_modified && status.has_untracked {
            break;
        }
    }

    status
}

/// Get extended status (dirty + upstream ahead/behind) using porcelain v2.
/// Falls back to v1 (existing `worktree_status`) if v2 is not supported.
pub fn extended_worktree_status(path: &Path) -> Result<WorktreeStatus> {
    match super::git(path, &["status", "--porcelain=v2", "--branch"]) {
        Ok(output) => Ok(parse_porcelain_v2(&output)),
        Err(_) => worktree_status(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_output_all_false() {
        let status = parse_porcelain("");
        assert!(!status.has_staged);
        assert!(!status.has_modified);
        assert!(!status.has_untracked);
        assert!(status.is_clean());
    }

    #[test]
    fn parse_untracked_file() {
        let status = parse_porcelain("?? file.txt\n");
        assert!(!status.has_staged);
        assert!(!status.has_modified);
        assert!(status.has_untracked);
    }

    #[test]
    fn parse_staged_file() {
        // "M " means index has modification, working tree clean
        let status = parse_porcelain("M  file.txt\n");
        assert!(status.has_staged);
        assert!(!status.has_modified);
        assert!(!status.has_untracked);
    }

    #[test]
    fn parse_modified_file() {
        // " M" means index clean, working tree has modification
        let status = parse_porcelain(" M file.txt\n");
        assert!(!status.has_staged);
        assert!(status.has_modified);
        assert!(!status.has_untracked);
    }

    #[test]
    fn parse_mixed_status_all_three() {
        // "MM" = staged + modified, "??" = untracked
        let status = parse_porcelain("MM file.txt\n?? other.txt\n");
        assert!(status.has_staged);
        assert!(status.has_modified);
        assert!(status.has_untracked);
    }

    #[test]
    fn parse_added_file() {
        // "A " means new file added to index
        let status = parse_porcelain("A  new_file.rs\n");
        assert!(status.has_staged);
        assert!(!status.has_modified);
        assert!(!status.has_untracked);
    }

    #[test]
    fn parse_deleted_in_worktree() {
        // " D" means deleted in working tree but not staged
        let status = parse_porcelain(" D deleted.rs\n");
        assert!(!status.has_staged);
        assert!(status.has_modified);
        assert!(!status.has_untracked);
    }

    #[test]
    fn parse_renamed_file() {
        // "R " means renamed in index
        let status = parse_porcelain("R  old.rs -> new.rs\n");
        assert!(status.has_staged);
        assert!(!status.has_modified);
        assert!(!status.has_untracked);
    }

    #[test]
    fn symbols_empty_for_clean() {
        let status = WorktreeStatus::default();
        assert_eq!(status.symbols(), "");
    }

    #[test]
    fn symbols_staged_only() {
        let status = WorktreeStatus {
            has_staged: true,
            has_modified: false,
            has_untracked: false,
            ..Default::default()
        };
        assert_eq!(status.symbols(), "+");
    }

    #[test]
    fn symbols_modified_only() {
        let status = WorktreeStatus {
            has_staged: false,
            has_modified: true,
            has_untracked: false,
            ..Default::default()
        };
        assert_eq!(status.symbols(), "!");
    }

    #[test]
    fn symbols_untracked_only() {
        let status = WorktreeStatus {
            has_staged: false,
            has_modified: false,
            has_untracked: true,
            ..Default::default()
        };
        assert_eq!(status.symbols(), "?");
    }

    #[test]
    fn symbols_all_flags() {
        let status = WorktreeStatus {
            has_staged: true,
            has_modified: true,
            has_untracked: true,
            ..Default::default()
        };
        assert_eq!(status.symbols(), "+!?");
    }

    #[test]
    fn symbols_staged_and_untracked() {
        let status = WorktreeStatus {
            has_staged: true,
            has_modified: false,
            has_untracked: true,
            ..Default::default()
        };
        assert_eq!(status.symbols(), "+?");
    }

    #[test]
    fn is_clean_true_for_default() {
        assert!(WorktreeStatus::default().is_clean());
    }

    #[test]
    fn is_clean_false_when_dirty() {
        let status = WorktreeStatus {
            has_staged: false,
            has_modified: true,
            has_untracked: false,
            ..Default::default()
        };
        assert!(!status.is_clean());
    }

    // ---- Porcelain v2 tests ----

    #[test]
    fn parse_v2_clean_with_upstream_in_sync() {
        let output = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        let status = parse_porcelain_v2(output);
        assert!(status.is_clean());
        assert!(status.has_upstream);
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
    }

    #[test]
    fn parse_v2_ahead_and_behind() {
        let output = "# branch.oid abc123\n# branch.head feat\n# branch.upstream origin/feat\n# branch.ab +3 -1\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.ahead, 3);
        assert_eq!(status.behind, 1);
        assert!(status.has_upstream);
    }

    #[test]
    fn parse_v2_no_upstream() {
        let output = "# branch.oid abc123\n# branch.head feat\n";
        let status = parse_porcelain_v2(output);
        assert!(!status.has_upstream);
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
    }

    #[test]
    fn parse_v2_with_dirty_files() {
        let output = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n1 .M N... 100644 100644 100644 abc123 def456 file.rs\n? untracked.txt\n";
        let status = parse_porcelain_v2(output);
        assert!(!status.has_staged);
        assert!(status.has_modified);
        assert!(status.has_untracked);
    }

    #[test]
    fn parse_v2_staged_file() {
        let output = "# branch.oid abc123\n# branch.head main\n1 M. N... 100644 100644 100644 abc123 def456 file.rs\n";
        let status = parse_porcelain_v2(output);
        assert!(status.has_staged);
        assert!(!status.has_modified);
    }

    #[test]
    fn parse_v2_staged_and_modified() {
        let output = "# branch.oid abc123\n# branch.head main\n1 MM N... 100644 100644 100644 abc123 def456 file.rs\n";
        let status = parse_porcelain_v2(output);
        assert!(status.has_staged);
        assert!(status.has_modified);
    }

    #[test]
    fn parse_v2_renamed_file() {
        let output = "# branch.oid abc123\n# branch.head main\n2 R. N... 100644 100644 100644 abc123 def456 R100 new.rs\told.rs\n";
        let status = parse_porcelain_v2(output);
        assert!(status.has_staged);
        assert!(!status.has_modified);
    }

    #[test]
    fn parse_v2_empty_output() {
        let status = parse_porcelain_v2("");
        assert!(status.is_clean());
        assert!(!status.has_upstream);
    }
}
