# Rich Worktree Status Display — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enhance `junktree list` to show comprehensive per-worktree status (dirty indicators, upstream ahead/behind, default-branch relationship, active operations, commit age/message, line diffs) with aligned status columns, CLI flags (--short, --json, --no-path, --no-color), and TDD at 85%+ coverage.

**Architecture:** Hybrid batch + per-worktree collection. `git for-each-ref` batch-collects branch metadata (one call). `git status --porcelain=v2 --branch` collects dirty status + upstream ahead/behind per worktree (one call each). Additional per-worktree calls for merge-base relationship and line diffs. All parsing separated from execution for testability.

**Tech Stack:** Rust, clap (derive), owo-colors, serde + serde_json (for --json), tempfile + assert_cmd (testing)

**Spec:** `docs/superpowers/specs/2026-04-01-rich-worktree-status-design.md`

---

## File Structure

### New files

| File | Responsibility |
|------|---------------|
| `src/git/line_diff.rs` | `LineDiff` struct + `parse_numstat()` parser + `line_diff()` public wrapper |
| `src/git/branch_info.rs` | `BranchInfo` struct + `parse_for_each_ref()` parser + `batch_branch_info()` wrapper |
| `src/git/operations.rs` | `ActiveOperation` enum + `detect_operation()` filesystem checker |
| `src/git/main_relationship.rs` | `MainRelationship` enum + `detect_main_relationship()` multi-step logic |
| `src/commands/list/mod.rs` | Orchestrator: delegates to collect, display, or json based on flags |
| `src/commands/list/collect.rs` | `WorktreeInfo` struct + `collect_worktree_info()` orchestration |
| `src/commands/list/display.rs` | Rich terminal output: column alignment, symbols, colors |
| `src/commands/list/json.rs` | JSON serialization structs + `print_json()` |

### Modified files

| File | Change |
|------|--------|
| `Cargo.toml` | Add `serde`, `serde_json` dependencies |
| `src/git/mod.rs` | Add `pub mod line_diff; pub mod branch_info; pub mod operations; pub mod main_relationship;` |
| `src/git/status.rs` | Add `ahead`, `behind`, `has_upstream` fields. Add `parse_porcelain_v2()`. Keep existing `parse_porcelain()` as fallback. |
| `src/output.rs` | Add new Unicode symbol constants |
| `src/cli.rs` | `Commands::List` becomes struct variant with `short`, `json`, `no_path`, `no_color` fields |
| `src/main.rs` | Destructure `List { short, json, no_path, no_color }` and pass to `commands::list::run()` |
| `src/commands/mod.rs` | No change needed — `pub mod list` already declared, Rust resolves `list/mod.rs` over `list.rs` after we swap |

---

### Task 1: Add serde dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add serde and serde_json**

In `Cargo.toml`, add to `[dependencies]` after the `ureq` line:

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add serde and serde_json dependencies"
```

---

### Task 2: Add new Unicode symbol constants to output.rs

**Files:**
- Modify: `src/output.rs`

- [ ] **Step 1: Write tests for new symbol constants**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/output.rs`:

```rust
#[test]
fn upstream_ahead_symbol() {
    assert_eq!(UPSTREAM_AHEAD, "\u{21E1}");
}

#[test]
fn upstream_behind_symbol() {
    assert_eq!(UPSTREAM_BEHIND, "\u{21E3}");
}

#[test]
fn upstream_diverge_symbol() {
    assert_eq!(UPSTREAM_DIVERGE, "\u{21C5}");
}

#[test]
fn upstream_sync_symbol() {
    assert_eq!(UPSTREAM_SYNC, "|");
}

#[test]
fn upstream_none_symbol() {
    assert_eq!(UPSTREAM_NONE, "\u{2014}");
}

#[test]
fn main_is_default_symbol() {
    assert_eq!(MAIN_IS_DEFAULT, "^");
}

#[test]
fn main_ahead_symbol() {
    assert_eq!(MAIN_AHEAD, "\u{2191}");
}

#[test]
fn main_behind_symbol() {
    assert_eq!(MAIN_BEHIND, "\u{2193}");
}

#[test]
fn main_diverge_symbol() {
    assert_eq!(MAIN_DIVERGE, "\u{2195}");
}

#[test]
fn main_integrated_symbol() {
    assert_eq!(MAIN_INTEGRATED, "\u{2282}");
}

#[test]
fn main_conflict_symbol() {
    assert_eq!(MAIN_CONFLICT, "\u{2717}");
}

#[test]
fn main_same_commit_symbol() {
    assert_eq!(MAIN_SAME_COMMIT, "_");
}

#[test]
fn main_orphan_symbol() {
    assert_eq!(MAIN_ORPHAN, "\u{2205}");
}

#[test]
fn op_rebase_symbol() {
    assert_eq!(OP_REBASE, "\u{2934}");
}

#[test]
fn op_merge_symbol() {
    assert_eq!(OP_MERGE, "\u{2935}");
}

#[test]
fn locked_symbol() {
    assert_eq!(LOCKED, "\u{229E}");
}

#[test]
fn prunable_symbol() {
    assert_eq!(PRUNABLE, "\u{229F}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib output::tests`
Expected: FAIL — constants not defined

- [ ] **Step 3: Add the constants**

Add after the existing `pub const ARROW` line in `src/output.rs`:

```rust
// Upstream relationship symbols
pub const UPSTREAM_AHEAD: &str = "\u{21E1}";   // ⇡
pub const UPSTREAM_BEHIND: &str = "\u{21E3}";  // ⇣
pub const UPSTREAM_DIVERGE: &str = "\u{21C5}"; // ⇅
pub const UPSTREAM_SYNC: &str = "|";            // |
pub const UPSTREAM_NONE: &str = "\u{2014}";    // —

// Main branch relationship symbols
pub const MAIN_IS_DEFAULT: &str = "^";
pub const MAIN_AHEAD: &str = "\u{2191}";       // ↑
pub const MAIN_BEHIND: &str = "\u{2193}";      // ↓
pub const MAIN_DIVERGE: &str = "\u{2195}";     // ↕
pub const MAIN_INTEGRATED: &str = "\u{2282}";  // ⊂
pub const MAIN_CONFLICT: &str = "\u{2717}";    // ✗
pub const MAIN_SAME_COMMIT: &str = "_";
pub const MAIN_ORPHAN: &str = "\u{2205}";      // ∅

// Active operation symbols
pub const OP_REBASE: &str = "\u{2934}";        // ⤴
pub const OP_MERGE: &str = "\u{2935}";         // ⤵

// Worktree state symbols
pub const LOCKED: &str = "\u{229E}";           // ⊞
pub const PRUNABLE: &str = "\u{229F}";         // ⊟
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib output::tests`
Expected: all PASS

- [ ] **Step 5: Commit**

```bash
git add src/output.rs
git commit -m "feat: add Unicode symbol constants for rich worktree status"
```

---

### Task 3: Extend WorktreeStatus with upstream tracking (porcelain v2)

**Files:**
- Modify: `src/git/status.rs`

- [ ] **Step 1: Write tests for the v2 porcelain parser**

Add these tests to the `#[cfg(test)] mod tests` block in `src/git/status.rs`:

```rust
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
    // v2 format: "1 .M N..." for modified, "? path" for untracked
    let output = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n1 .M N... 100644 100644 100644 abc123 def456 file.rs\n? untracked.txt\n";
    let status = parse_porcelain_v2(output);
    assert!(!status.has_staged);
    assert!(status.has_modified);
    assert!(status.has_untracked);
    assert!(status.has_upstream);
}

#[test]
fn parse_v2_staged_file() {
    let output = "# branch.oid abc123\n# branch.head main\n1 M. N... 100644 100644 100644 abc123 def456 file.rs\n";
    let status = parse_porcelain_v2(output);
    assert!(status.has_staged);
    assert!(!status.has_modified);
    assert!(!status.has_untracked);
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
    // v2 rename format: "2 R. N... ... path\toriginal_path"
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib git::status::tests`
Expected: FAIL — `parse_porcelain_v2` not defined, new fields not on struct

- [ ] **Step 3: Add new fields to WorktreeStatus**

Update the `WorktreeStatus` struct in `src/git/status.rs`:

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorktreeStatus {
    pub has_staged: bool,
    pub has_modified: bool,
    pub has_untracked: bool,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
}
```

- [ ] **Step 4: Implement parse_porcelain_v2**

Add this function in `src/git/status.rs` (after the existing `parse_porcelain` function):

```rust
/// Parse `git status --porcelain=v2 --branch` output into WorktreeStatus.
///
/// Header lines start with `#`:
/// - `# branch.ab +N -M` → ahead N, behind M
/// - `# branch.upstream origin/main` → has upstream
///
/// File entry lines:
/// - Start with `1` (ordinary) or `2` (rename): XY field at chars [2..4]
///   - X = index status, Y = worktree status
///   - `.` means unchanged, letters mean changed
/// - Start with `?` → untracked file
/// - Start with `u` → unmerged (counts as both staged and modified)
fn parse_porcelain_v2(output: &str) -> WorktreeStatus {
    let mut status = WorktreeStatus::default();

    for line in output.lines() {
        if line.starts_with("# branch.ab ") {
            // Parse "# branch.ab +3 -1"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                status.ahead = parts[2]
                    .trim_start_matches('+')
                    .parse()
                    .unwrap_or(0);
                status.behind = parts[3]
                    .trim_start_matches('-')
                    .parse()
                    .unwrap_or(0);
            }
        } else if line.starts_with("# branch.upstream ") {
            status.has_upstream = true;
        } else if line.starts_with('#') {
            // Other header lines — skip
            continue;
        } else if line.starts_with('?') {
            status.has_untracked = true;
        } else if line.starts_with('1') || line.starts_with('2') {
            // Ordinary or rename entry: "1 XY ..." or "2 XY ..."
            let bytes = line.as_bytes();
            if bytes.len() >= 4 {
                let index_char = bytes[2]; // X
                let tree_char = bytes[3];  // Y
                if index_char != b'.' {
                    status.has_staged = true;
                }
                if tree_char != b'.' {
                    status.has_modified = true;
                }
            }
        } else if line.starts_with('u') {
            // Unmerged entry — both sides have changes
            status.has_staged = true;
            status.has_modified = true;
        }

        if status.has_staged && status.has_modified && status.has_untracked {
            break;
        }
    }

    status
}
```

- [ ] **Step 5: Add extended_worktree_status public function with v1 fallback**

Add below the existing `worktree_status` function:

```rust
/// Get extended status (dirty + upstream ahead/behind) using porcelain v2.
/// Falls back to v1 (existing `worktree_status`) if v2 is not supported.
pub fn extended_worktree_status(path: &Path) -> Result<WorktreeStatus> {
    match super::git(path, &["status", "--porcelain=v2", "--branch"]) {
        Ok(output) => Ok(parse_porcelain_v2(&output)),
        Err(_) => {
            // v2 not supported — fall back to v1 (no upstream info)
            worktree_status(path)
        }
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib git::status::tests`
Expected: all PASS (old and new tests)

- [ ] **Step 7: Commit**

```bash
git add src/git/status.rs
git commit -m "feat: add porcelain v2 parser with upstream ahead/behind tracking"
```

---

### Task 4: Create line_diff module

**Files:**
- Create: `src/git/line_diff.rs`
- Modify: `src/git/mod.rs`

- [ ] **Step 1: Create src/git/line_diff.rs with tests first**

```rust
use std::path::Path;

use anyhow::Result;

/// Line-level diff statistics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineDiff {
    pub insertions: u32,
    pub deletions: u32,
}

/// Parse `git diff --numstat` output into a LineDiff.
///
/// Each line: "insertions\tdeletions\tfilename"
/// Binary files show "-\t-\tfilename" — skip those.
fn parse_numstat(output: &str) -> LineDiff {
    let mut diff = LineDiff::default();

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            if let (Ok(ins), Ok(del)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                diff.insertions += ins;
                diff.deletions += del;
            }
            // Binary files have "-" which parse::<u32>() fails on — skipped naturally
        }
    }

    diff
}

/// Get line diff of working tree vs HEAD.
pub fn line_diff_head(path: &Path) -> Result<LineDiff> {
    match super::git(path, &["diff", "--numstat", "HEAD"]) {
        Ok(output) => Ok(parse_numstat(&output)),
        Err(_) => Ok(LineDiff::default()),
    }
}

/// Get line diff between two refs (e.g., default...branch).
pub fn line_diff_refs(dir: &Path, range: &str) -> Result<LineDiff> {
    match super::git(dir, &["diff", "--numstat", range]) {
        Ok(output) => Ok(parse_numstat(&output)),
        Err(_) => Ok(LineDiff::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numstat_empty() {
        let diff = parse_numstat("");
        assert_eq!(diff, LineDiff::default());
    }

    #[test]
    fn parse_numstat_single_file() {
        let diff = parse_numstat("10\t3\tsrc/main.rs\n");
        assert_eq!(diff.insertions, 10);
        assert_eq!(diff.deletions, 3);
    }

    #[test]
    fn parse_numstat_multiple_files() {
        let output = "10\t3\tsrc/main.rs\n5\t0\tsrc/lib.rs\n0\t7\tsrc/old.rs\n";
        let diff = parse_numstat(output);
        assert_eq!(diff.insertions, 15);
        assert_eq!(diff.deletions, 10);
    }

    #[test]
    fn parse_numstat_binary_file_skipped() {
        let output = "-\t-\timage.png\n5\t2\tsrc/main.rs\n";
        let diff = parse_numstat(output);
        assert_eq!(diff.insertions, 5);
        assert_eq!(diff.deletions, 2);
    }

    #[test]
    fn parse_numstat_rename() {
        // Renames with --numstat show: "0\t0\told.rs => new.rs" or with -M flag
        let output = "0\t0\t{old.rs => new.rs}\n";
        let diff = parse_numstat(output);
        assert_eq!(diff.insertions, 0);
        assert_eq!(diff.deletions, 0);
    }

    #[test]
    fn parse_numstat_no_trailing_newline() {
        let diff = parse_numstat("3\t1\tfile.rs");
        assert_eq!(diff.insertions, 3);
        assert_eq!(diff.deletions, 1);
    }
}
```

- [ ] **Step 2: Register module in src/git/mod.rs**

Add after the existing `pub mod worktree;` line:

```rust
pub mod line_diff;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib git::line_diff::tests`
Expected: all PASS

- [ ] **Step 4: Commit**

```bash
git add src/git/line_diff.rs src/git/mod.rs
git commit -m "feat: add line_diff module for git diff --numstat parsing"
```

---

### Task 5: Create branch_info module (batch collection)

**Files:**
- Create: `src/git/branch_info.rs`
- Modify: `src/git/mod.rs`

- [ ] **Step 1: Create src/git/branch_info.rs with tests first**

```rust
use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

/// Per-branch metadata collected via `git for-each-ref`.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchInfo {
    pub upstream: Option<String>,
    pub commit_age_secs: i64,
    pub commit_message: String,
}

/// Parse `git for-each-ref` output with pipe-separated fields.
///
/// Expected format per line:
/// `branch_name|upstream_name|unix_timestamp|commit_subject`
///
/// If upstream is empty, it means no tracking branch is configured.
fn parse_for_each_ref(output: &str) -> HashMap<String, BranchInfo> {
    let mut map = HashMap::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 4 {
            continue;
        }

        let branch = parts[0].to_string();
        let upstream = if parts[1].is_empty() {
            None
        } else {
            Some(parts[1].to_string())
        };
        let commit_age_secs = parts[2].parse::<i64>().unwrap_or(0);
        let commit_message = parts[3].to_string();

        map.insert(branch, BranchInfo {
            upstream,
            commit_age_secs,
            commit_message,
        });
    }

    map
}

/// Batch-collect branch metadata for all local branches via a single git call.
pub fn batch_branch_info(dir: &Path) -> Result<HashMap<String, BranchInfo>> {
    let format = "%(refname:short)|%(upstream:short)|%(committerdate:unix)|%(subject)";
    match super::git(dir, &["for-each-ref", &format!("--format={}", format), "refs/heads"]) {
        Ok(output) => Ok(parse_for_each_ref(&output)),
        Err(_) => Ok(HashMap::new()),
    }
}

/// Detect the default branch name of a bare repo.
///
/// Reads the symbolic-ref of HEAD in the bare repo directory.
/// Falls back to "main" if detection fails.
pub fn detect_default_branch(dir: &Path) -> String {
    match super::git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(branch) => branch.trim().to_string(),
        Err(_) => "main".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_for_each_ref_empty() {
        let map = parse_for_each_ref("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_for_each_ref_single_branch_with_upstream() {
        let output = "main|origin/main|1711929600|fix: resolve auth bug\n";
        let map = parse_for_each_ref(output);
        assert_eq!(map.len(), 1);
        let info = map.get("main").unwrap();
        assert_eq!(info.upstream, Some("origin/main".to_string()));
        assert_eq!(info.commit_age_secs, 1711929600);
        assert_eq!(info.commit_message, "fix: resolve auth bug");
    }

    #[test]
    fn parse_for_each_ref_no_upstream() {
        let output = "feature/auth||1711929600|wip: add oauth\n";
        let map = parse_for_each_ref(output);
        let info = map.get("feature/auth").unwrap();
        assert_eq!(info.upstream, None);
    }

    #[test]
    fn parse_for_each_ref_multiple_branches() {
        let output = "main|origin/main|1711929600|fix: auth\ndev|origin/dev|1711843200|feat: add api\n";
        let map = parse_for_each_ref(output);
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("main"));
        assert!(map.contains_key("dev"));
    }

    #[test]
    fn parse_for_each_ref_commit_message_with_pipes() {
        // Subject may contain | characters — splitn(4, '|') handles this
        let output = "main|origin/main|1711929600|fix: parse | separated output\n";
        let map = parse_for_each_ref(output);
        let info = map.get("main").unwrap();
        assert_eq!(info.commit_message, "fix: parse | separated output");
    }

    #[test]
    fn parse_for_each_ref_invalid_timestamp() {
        let output = "main|origin/main|not-a-number|some commit\n";
        let map = parse_for_each_ref(output);
        let info = map.get("main").unwrap();
        assert_eq!(info.commit_age_secs, 0);
    }

    #[test]
    fn parse_for_each_ref_malformed_line_skipped() {
        let output = "incomplete|line\nmain|origin/main|1711929600|good line\n";
        let map = parse_for_each_ref(output);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("main"));
    }
}
```

- [ ] **Step 2: Register module in src/git/mod.rs**

Add after `pub mod line_diff;`:

```rust
pub mod branch_info;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib git::branch_info::tests`
Expected: all PASS

- [ ] **Step 4: Commit**

```bash
git add src/git/branch_info.rs src/git/mod.rs
git commit -m "feat: add branch_info module for batch for-each-ref collection"
```

---

### Task 6: Create operations module (filesystem detection)

**Files:**
- Create: `src/git/operations.rs`
- Modify: `src/git/mod.rs`

- [ ] **Step 1: Create src/git/operations.rs with tests first**

```rust
use std::path::Path;

/// Active git operation detected via filesystem sentinel files.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveOperation {
    None,
    Rebase,
    Merge,
}

/// Detect if a worktree has an active rebase or merge operation.
///
/// Checks for sentinel files/directories in the worktree's `.git` directory:
/// - `rebase-merge/` or `rebase-apply/` → Rebase
/// - `MERGE_HEAD` → Merge
///
/// For linked worktrees, `.git` is a file pointing to the gitdir. We resolve
/// the actual gitdir path before checking.
pub fn detect_operation(worktree_path: &Path) -> ActiveOperation {
    let git_path = worktree_path.join(".git");

    let gitdir = if git_path.is_file() {
        // Linked worktree: .git is a file containing "gitdir: <path>"
        match std::fs::read_to_string(&git_path) {
            Ok(content) => {
                let gitdir_str = content.trim().strip_prefix("gitdir: ").unwrap_or("");
                if gitdir_str.is_empty() {
                    return ActiveOperation::None;
                }
                let resolved = Path::new(gitdir_str);
                if resolved.is_absolute() {
                    resolved.to_path_buf()
                } else {
                    worktree_path.join(resolved)
                }
            }
            Err(_) => return ActiveOperation::None,
        }
    } else if git_path.is_dir() {
        git_path
    } else {
        return ActiveOperation::None;
    };

    // Check rebase first (higher priority)
    if gitdir.join("rebase-merge").is_dir() || gitdir.join("rebase-apply").is_dir() {
        return ActiveOperation::Rebase;
    }

    if gitdir.join("MERGE_HEAD").is_file() {
        return ActiveOperation::Merge;
    }

    ActiveOperation::None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_worktree_gitdir() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let wt = dir.path().join("worktree");
        std::fs::create_dir_all(&wt).unwrap();
        let gitdir = dir.path().join("gitdirs").join("worktree");
        std::fs::create_dir_all(&gitdir).unwrap();
        // Write .git file pointing to gitdir
        let gitdir_abs = dunce::canonicalize(&gitdir).unwrap_or_else(|_| gitdir.clone());
        std::fs::write(wt.join(".git"), format!("gitdir: {}", gitdir_abs.display())).unwrap();
        (dir, wt, gitdir)
    }

    #[test]
    fn detect_no_operation() {
        let (_dir, wt, _gitdir) = setup_worktree_gitdir();
        assert_eq!(detect_operation(&wt), ActiveOperation::None);
    }

    #[test]
    fn detect_rebase_merge() {
        let (_dir, wt, gitdir) = setup_worktree_gitdir();
        std::fs::create_dir(gitdir.join("rebase-merge")).unwrap();
        assert_eq!(detect_operation(&wt), ActiveOperation::Rebase);
    }

    #[test]
    fn detect_rebase_apply() {
        let (_dir, wt, gitdir) = setup_worktree_gitdir();
        std::fs::create_dir(gitdir.join("rebase-apply")).unwrap();
        assert_eq!(detect_operation(&wt), ActiveOperation::Rebase);
    }

    #[test]
    fn detect_merge() {
        let (_dir, wt, gitdir) = setup_worktree_gitdir();
        std::fs::write(gitdir.join("MERGE_HEAD"), "abc123\n").unwrap();
        assert_eq!(detect_operation(&wt), ActiveOperation::Merge);
    }

    #[test]
    fn rebase_takes_priority_over_merge() {
        let (_dir, wt, gitdir) = setup_worktree_gitdir();
        std::fs::create_dir(gitdir.join("rebase-merge")).unwrap();
        std::fs::write(gitdir.join("MERGE_HEAD"), "abc123\n").unwrap();
        assert_eq!(detect_operation(&wt), ActiveOperation::Rebase);
    }

    #[test]
    fn detect_nonexistent_path_returns_none() {
        let path = Path::new("/tmp/nonexistent-worktree-test-12345");
        assert_eq!(detect_operation(path), ActiveOperation::None);
    }

    #[test]
    fn detect_with_real_git_dir() {
        // When .git is a directory (non-linked worktree)
        let dir = TempDir::new().unwrap();
        let wt = dir.path().join("repo");
        let git_dir = wt.join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        assert_eq!(detect_operation(&wt), ActiveOperation::None);

        // Add merge sentinel
        std::fs::write(git_dir.join("MERGE_HEAD"), "abc123\n").unwrap();
        assert_eq!(detect_operation(&wt), ActiveOperation::Merge);
    }
}
```

- [ ] **Step 2: Register module in src/git/mod.rs**

Add after `pub mod branch_info;`:

```rust
pub mod operations;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib git::operations::tests`
Expected: all PASS

- [ ] **Step 4: Commit**

```bash
git add src/git/operations.rs src/git/mod.rs
git commit -m "feat: add operations module for rebase/merge detection"
```

---

### Task 7: Create main_relationship module

**Files:**
- Create: `src/git/main_relationship.rs`
- Modify: `src/git/mod.rs`

- [ ] **Step 1: Create src/git/main_relationship.rs with tests and implementation**

```rust
use std::path::Path;

use anyhow::Result;

/// Relationship of a branch to the default branch.
///
/// States are mutually exclusive and checked in priority order
/// (see `detect_main_relationship`).
#[derive(Debug, Clone, PartialEq)]
pub enum MainRelationship {
    IsDefault,
    Orphan,
    SameCommit,
    Integrated,
    WouldConflict,
    Diverged { ahead: u32, behind: u32 },
    Ahead(u32),
    Behind(u32),
}

impl MainRelationship {
    /// Format for display column.
    pub fn symbol(&self) -> String {
        match self {
            Self::IsDefault => crate::output::MAIN_IS_DEFAULT.to_string(),
            Self::Orphan => crate::output::MAIN_ORPHAN.to_string(),
            Self::SameCommit => crate::output::MAIN_SAME_COMMIT.to_string(),
            Self::Integrated => crate::output::MAIN_INTEGRATED.to_string(),
            Self::WouldConflict => crate::output::MAIN_CONFLICT.to_string(),
            Self::Diverged { .. } => crate::output::MAIN_DIVERGE.to_string(),
            Self::Ahead(n) => format!("{}{}", crate::output::MAIN_AHEAD, n),
            Self::Behind(n) => format!("{}{}", crate::output::MAIN_BEHIND, n),
        }
    }
}

/// Detect the relationship of a branch to the default branch.
///
/// Priority order:
/// 1. IsDefault — branch name matches default
/// 2. Orphan — no common ancestor (merge-base fails)
/// 3. SameCommit — head == default_head
/// 4. Integrated — merge-base tree matches default tree
/// 5. WouldConflict — merge-tree reports conflicts (git 2.38+)
/// 6. Diverged — ahead > 0 and behind > 0
/// 7. Ahead — ahead > 0
/// 8. Behind — behind > 0
pub fn detect_main_relationship(
    dir: &Path,
    branch_name: &str,
    branch_head: &str,
    default_branch: &str,
    default_head: &str,
    supports_merge_tree: bool,
) -> MainRelationship {
    // 1. IsDefault
    if branch_name == default_branch {
        return MainRelationship::IsDefault;
    }

    // 2. Orphan — merge-base fails
    let merge_base = match super::git(dir, &["merge-base", branch_head, default_head]) {
        Ok(output) => output.trim().to_string(),
        Err(_) => return MainRelationship::Orphan,
    };

    // 3. SameCommit
    if branch_head == default_head {
        return MainRelationship::SameCommit;
    }

    // 4. Integrated — merge-base tree == default tree
    let base_tree = super::git(dir, &["rev-parse", &format!("{}^{{tree}}", merge_base)])
        .map(|s| s.trim().to_string());
    let default_tree = super::git(dir, &["rev-parse", &format!("{}^{{tree}}", default_head)])
        .map(|s| s.trim().to_string());
    if let (Ok(bt), Ok(dt)) = (&base_tree, &default_tree) {
        if bt == dt {
            return MainRelationship::Integrated;
        }
    }

    // Count ahead/behind
    let ahead = super::git(dir, &["rev-list", "--count", &format!("{}..{}", merge_base, branch_head)])
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let behind = super::git(dir, &["rev-list", "--count", &format!("{}..{}", merge_base, default_head)])
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    // 5. WouldConflict (only if merge-tree supported and branches diverge)
    if supports_merge_tree && ahead > 0 && behind > 0 {
        if let Ok(output) = super::git(dir, &["merge-tree", "--write-tree", &merge_base, branch_head, default_head]) {
            // merge-tree --write-tree outputs just a tree hash on success (clean merge)
            // and includes "CONFLICT" lines on failure
            if output.contains("CONFLICT") {
                return MainRelationship::WouldConflict;
            }
        }
        // If merge-tree itself fails, fall through to Diverged
    }

    // 6/7/8. Diverged / Ahead / Behind
    match (ahead, behind) {
        (a, b) if a > 0 && b > 0 => MainRelationship::Diverged { ahead: a, behind: b },
        (a, 0) if a > 0 => MainRelationship::Ahead(a),
        (0, b) if b > 0 => MainRelationship::Behind(b),
        _ => MainRelationship::SameCommit, // both 0 — shouldn't reach here but safe fallback
    }
}

/// Check if the current git version supports `merge-tree --write-tree` (git 2.38+).
///
/// Runs a quick probe command. Returns false if the flag is unrecognized.
pub fn check_merge_tree_support(dir: &Path) -> bool {
    // Use a known-good tree to test the flag; if git doesn't support --write-tree,
    // it will fail with an error about the unknown option
    super::git_check(dir, &["merge-tree", "--write-tree", "HEAD", "HEAD", "HEAD"])
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_is_default() {
        assert_eq!(MainRelationship::IsDefault.symbol(), "^");
    }

    #[test]
    fn symbol_orphan() {
        assert_eq!(MainRelationship::Orphan.symbol(), "\u{2205}");
    }

    #[test]
    fn symbol_same_commit() {
        assert_eq!(MainRelationship::SameCommit.symbol(), "_");
    }

    #[test]
    fn symbol_integrated() {
        assert_eq!(MainRelationship::Integrated.symbol(), "\u{2282}");
    }

    #[test]
    fn symbol_would_conflict() {
        assert_eq!(MainRelationship::WouldConflict.symbol(), "\u{2717}");
    }

    #[test]
    fn symbol_diverged() {
        assert_eq!(
            MainRelationship::Diverged { ahead: 3, behind: 2 }.symbol(),
            "\u{2195}"
        );
    }

    #[test]
    fn symbol_ahead() {
        assert_eq!(MainRelationship::Ahead(5).symbol(), "\u{2191}5");
    }

    #[test]
    fn symbol_behind() {
        assert_eq!(MainRelationship::Behind(3).symbol(), "\u{2193}3");
    }
}
```

- [ ] **Step 2: Register module in src/git/mod.rs**

Add after `pub mod operations;`:

```rust
pub mod main_relationship;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --lib git::main_relationship::tests`
Expected: all PASS

- [ ] **Step 4: Commit**

```bash
git add src/git/main_relationship.rs src/git/mod.rs
git commit -m "feat: add main_relationship module for default-branch relationship detection"
```

---

### Task 8: Update CLI to add list command flags

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write tests for the new CLI flags**

Add to the `#[cfg(test)] mod tests` block in `src/cli.rs`:

```rust
#[test]
fn parse_list_with_short_flag() {
    let cli = Cli::parse_from(["junktree", "list", "--short"]);
    match cli.command {
        Commands::List { short, json, no_path, no_color } => {
            assert!(short);
            assert!(!json);
            assert!(!no_path);
            assert!(!no_color);
        }
        _ => panic!("Expected List command"),
    }
}

#[test]
fn parse_list_with_json_flag() {
    let cli = Cli::parse_from(["junktree", "list", "--json"]);
    match cli.command {
        Commands::List { json, .. } => assert!(json),
        _ => panic!("Expected List command"),
    }
}

#[test]
fn parse_list_with_no_path_flag() {
    let cli = Cli::parse_from(["junktree", "list", "--no-path"]);
    match cli.command {
        Commands::List { no_path, .. } => assert!(no_path),
        _ => panic!("Expected List command"),
    }
}

#[test]
fn parse_list_with_no_color_flag() {
    let cli = Cli::parse_from(["junktree", "list", "--no-color"]);
    match cli.command {
        Commands::List { no_color, .. } => assert!(no_color),
        _ => panic!("Expected List command"),
    }
}

#[test]
fn parse_list_with_all_flags() {
    let cli = Cli::parse_from(["junktree", "list", "--short", "--no-path", "--no-color"]);
    match cli.command {
        Commands::List { short, no_path, no_color, .. } => {
            assert!(short);
            assert!(no_path);
            assert!(no_color);
        }
        _ => panic!("Expected List command"),
    }
}

#[test]
fn parse_list_no_flags_defaults_to_false() {
    let cli = Cli::parse_from(["junktree", "list"]);
    match cli.command {
        Commands::List { short, json, no_path, no_color } => {
            assert!(!short);
            assert!(!json);
            assert!(!no_path);
            assert!(!no_color);
        }
        _ => panic!("Expected List command"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib cli::tests`
Expected: FAIL — `Commands::List` is still a unit variant

- [ ] **Step 3: Update Commands::List to struct variant**

In `src/cli.rs`, replace:

```rust
    /// List all worktrees
    List,
```

with:

```rust
    /// List all worktrees
    List {
        /// Minimal output: name + branch + dirty only
        #[arg(long)]
        short: bool,

        /// Output as JSON for scripting
        #[arg(long)]
        json: bool,

        /// Hide worktree path lines
        #[arg(long)]
        no_path: bool,

        /// Disable colored output
        #[arg(long)]
        no_color: bool,
    },
```

- [ ] **Step 4: Fix the existing test that matches on Commands::List**

In `src/cli.rs`, update the `parse_list_command` test:

```rust
#[test]
fn parse_list_command() {
    let cli = Cli::parse_from(["junktree", "list"]);
    match cli.command {
        Commands::List { short, json, no_path, no_color } => {
            assert!(!short);
            assert!(!json);
            assert!(!no_path);
            assert!(!no_color);
        }
        _ => panic!("Expected List command"),
    }
    assert!(!cli.verbose);
}
```

Also update the `parse_verbose_flag_before_subcommand` and `parse_verbose_flag_after_subcommand` tests — they match `Commands::List` too:

```rust
#[test]
fn parse_verbose_flag_before_subcommand() {
    let cli = Cli::parse_from(["junktree", "--verbose", "list"]);
    assert!(cli.verbose);
    assert!(matches!(cli.command, Commands::List { .. }));
}

#[test]
fn parse_verbose_flag_after_subcommand() {
    let cli = Cli::parse_from(["junktree", "list", "--verbose"]);
    assert!(cli.verbose);
    assert!(matches!(cli.command, Commands::List { .. }));
}
```

- [ ] **Step 5: Update main.rs to destructure the new List variant**

In `src/main.rs`, replace:

```rust
        Commands::List => commands::list::run(&ctx, config),
```

with:

```rust
        Commands::List { short, json, no_path, no_color } => {
            commands::list::run(&ctx, config, short, json, no_path, no_color)
        }
```

- [ ] **Step 6: Temporarily update list::run signature to accept new params**

In `src/commands/list.rs`, change the function signature to accept the new params (ignore them for now — we'll use them in later tasks):

```rust
pub fn run(ctx: &ProjectContext, _config: &Config, _short: bool, _json: bool, _no_path: bool, _no_color: bool) -> Result<()> {
```

- [ ] **Step 7: Run all tests to verify everything passes**

Run: `cargo test`
Expected: all PASS (old and new)

- [ ] **Step 8: Commit**

```bash
git add src/cli.rs src/main.rs src/commands/list.rs
git commit -m "feat: add --short, --json, --no-path, --no-color flags to list command"
```

---

### Task 9: Convert list.rs to list/ module directory with collect.rs

**Files:**
- Delete: `src/commands/list.rs`
- Create: `src/commands/list/mod.rs`
- Create: `src/commands/list/collect.rs`

- [ ] **Step 1: Create src/commands/list/ directory**

```bash
mkdir -p src/commands/list
```

- [ ] **Step 2: Create src/commands/list/collect.rs with WorktreeInfo**

```rust
use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::git::branch_info::{batch_branch_info, detect_default_branch, BranchInfo};
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
    let default_branch = detect_default_branch(project_dir);
    let supports_merge_tree = check_merge_tree_support(project_dir);

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

        infos.push(WorktreeInfo {
            display_name,
            is_current,
            status,
            main_rel,
            operation,
            branch_info,
            line_diff_head: ldh,
            line_diff_main: ldm,
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

        infos.push(WorktreeInfo {
            display_name,
            is_current,
            status,
            main_rel: MainRelationship::IsDefault,
            operation: ActiveOperation::None,
            branch_info: None,
            line_diff_head: None,
            line_diff_main: None,
            worktree: wt,
        });
    }

    Ok((infos, String::new()))
}
```

- [ ] **Step 3: Create src/commands/list/mod.rs (orchestrator)**

Move the old `list.rs` logic into `mod.rs` but delegate to collect + display:

```rust
pub mod collect;
pub mod display;
pub mod json;

use anyhow::Result;

use crate::config::Config;
use crate::layout::ProjectContext;

pub fn run(
    ctx: &ProjectContext,
    _config: &Config,
    short: bool,
    json: bool,
    no_path: bool,
    no_color: bool,
) -> Result<()> {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| dunce::canonicalize(&p).ok());

    let (infos, _default_branch) = collect::collect_all(
        &ctx.project_dir,
        cwd.as_deref(),
        short,
    )?;

    if json {
        json::print_json(&infos, &ctx.project_name)?;
    } else {
        display::print_rich(&infos, &ctx.project_name, short, no_path, no_color);
    }

    Ok(())
}
```

- [ ] **Step 4: Create stub files for display.rs and json.rs**

Create `src/commands/list/display.rs`:

```rust
use super::collect::WorktreeInfo;

/// Print the rich terminal output for all worktrees.
pub fn print_rich(
    _infos: &[WorktreeInfo],
    _project_name: &str,
    _short: bool,
    _no_path: bool,
    _no_color: bool,
) {
    // Implemented in Task 10
    todo!("Rich display not yet implemented")
}
```

Create `src/commands/list/json.rs`:

```rust
use anyhow::Result;
use super::collect::WorktreeInfo;

/// Print JSON output for all worktrees.
pub fn print_json(_infos: &[WorktreeInfo], _project_name: &str) -> Result<()> {
    // Implemented in Task 11
    todo!("JSON output not yet implemented")
}
```

- [ ] **Step 5: Delete the old src/commands/list.rs**

```bash
rm src/commands/list.rs
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo check`
Expected: compiles (display and json have `todo!()` but aren't called in tests)

- [ ] **Step 7: Commit**

```bash
git add src/commands/list/ src/commands/
git rm src/commands/list.rs
git commit -m "refactor: convert list command to module directory with collect.rs"
```

---

### Task 10: Implement rich display output

**Files:**
- Modify: `src/commands/list/display.rs`

- [ ] **Step 1: Write tests for display formatting**

Replace the stub in `src/commands/list/display.rs` with the full implementation including tests. The display module formats each `WorktreeInfo` into aligned columns.

```rust
use std::time::{SystemTime, UNIX_EPOCH};

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
        // Set NO_COLOR env var so owo-colors disables itself
        std::env::set_var("NO_COLOR", "1");
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

fn print_short_line(info: &WorktreeInfo, pad_width: usize) {
    let branch = info.worktree.branch.as_deref().unwrap_or("(detached)");
    let name_branch = if info.display_name == branch {
        info.display_name.clone()
    } else {
        format!(
            "{} -> {}",
            info.display_name,
            branch.if_supports_color(Stdout, |t| t.green())
        )
    };

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

    if info.is_current {
        let star = output::STAR.if_supports_color(Stdout, |t| t.cyan());
        let tag = "[current]".if_supports_color(Stdout, |t| t.cyan());
        let arrow = output::ARROW.if_supports_color(Stdout, |t| t.cyan());
        let name_col = info
            .display_name
            .if_supports_color(Stdout, |t| t.cyan())
            .to_string();
        if info.display_name == branch {
            println!("{} {} {} {}{}", star, tag, arrow, name_col, status_suffix);
        } else {
            let branch_col = branch.if_supports_color(Stdout, |t| t.green());
            println!(
                "{} {} {} {} -> {}{}",
                star, tag, arrow, name_col, branch_col, status_suffix
            );
        }
    } else {
        let arrow = output::ARROW.if_supports_color(Stdout, |t| t.cyan());
        if info.display_name == branch {
            println!("  {} {}{}", arrow, info.display_name, status_suffix);
        } else {
            let branch_col = branch.if_supports_color(Stdout, |t| t.green());
            println!(
                "  {} {} -> {}{}",
                arrow, info.display_name, branch_col, status_suffix
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

    let age_dim = age_col.if_supports_color(Stdout, |t| t.dimmed());
    let msg_dim = message_col.if_supports_color(Stdout, |t| t.dimmed());

    println!(
        "{}{}{}  {}  {}  {}  {}  {}",
        gutter_name, padding, special, dirty_col, upstream_col, main_col, age_dim, msg_dim
    );
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

/// Format a unix timestamp as relative age (e.g., "3d ago").
fn format_age(commit_timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let diff_secs = now - commit_timestamp;
    if diff_secs < 0 {
        return "now".to_string();
    }

    let minutes = diff_secs / 60;
    let hours = diff_secs / 3600;
    let days = diff_secs / 86400;
    let weeks = diff_secs / 604800;
    let months = diff_secs / 2592000;

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
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_age_now() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_age(now), "now");
    }

    #[test]
    fn format_age_minutes() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_age(now - 300), "5m ago");
    }

    #[test]
    fn format_age_hours() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_age(now - 7200), "2h ago");
    }

    #[test]
    fn format_age_days() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_age(now - 259200), "3d ago");
    }

    #[test]
    fn format_age_weeks() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_age(now - 1209600), "2w ago");
    }

    #[test]
    fn format_age_months() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_age(now - 5184000), "2mo ago");
    }

    #[test]
    fn format_age_future_timestamp() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        assert_eq!(format_age(now + 1000), "now");
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
        std::env::set_var("NO_COLOR", "1");
        let result = format_dirty(&status);
        assert!(result.contains("clean"));
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_dirty_all_flags() {
        let status = crate::git::status::WorktreeStatus {
            has_staged: true,
            has_modified: true,
            has_untracked: true,
            ..Default::default()
        };
        std::env::set_var("NO_COLOR", "1");
        let result = format_dirty(&status);
        assert!(result.contains("+!?"));
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_upstream_no_upstream() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: false,
            ..Default::default()
        };
        std::env::set_var("NO_COLOR", "1");
        let result = format_upstream(&status);
        assert!(result.contains("\u{2014}")); // —
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_upstream_in_sync() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: true,
            ahead: 0,
            behind: 0,
            ..Default::default()
        };
        std::env::set_var("NO_COLOR", "1");
        let result = format_upstream(&status);
        assert!(result.contains("|"));
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_upstream_ahead() {
        let status = crate::git::status::WorktreeStatus {
            has_upstream: true,
            ahead: 3,
            behind: 0,
            ..Default::default()
        };
        std::env::set_var("NO_COLOR", "1");
        let result = format_upstream(&status);
        assert!(result.contains("3"));
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_main_rel_is_default() {
        std::env::set_var("NO_COLOR", "1");
        let result = format_main_rel(&MainRelationship::IsDefault);
        assert!(result.contains("^"));
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_main_rel_ahead() {
        std::env::set_var("NO_COLOR", "1");
        let result = format_main_rel(&MainRelationship::Ahead(5));
        assert!(result.contains("5"));
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_special_no_indicators() {
        let info = make_test_info(ActiveOperation::None, false, false);
        assert_eq!(format_special_indicators(&info), "");
    }

    #[test]
    fn format_special_rebase() {
        let info = make_test_info(ActiveOperation::Rebase, false, false);
        std::env::set_var("NO_COLOR", "1");
        let result = format_special_indicators(&info);
        assert!(result.contains("\u{2934}")); // ⤴
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_special_locked() {
        let info = make_test_info(ActiveOperation::None, true, false);
        std::env::set_var("NO_COLOR", "1");
        let result = format_special_indicators(&info);
        assert!(result.contains("\u{229E}")); // ⊞
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn format_special_prunable() {
        let info = make_test_info(ActiveOperation::None, false, true);
        std::env::set_var("NO_COLOR", "1");
        let result = format_special_indicators(&info);
        assert!(result.contains("\u{229F}")); // ⊟
        std::env::remove_var("NO_COLOR");
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
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib commands::list::display::tests`
Expected: all PASS

- [ ] **Step 3: Commit**

```bash
git add src/commands/list/display.rs
git commit -m "feat: implement rich display output with aligned status columns"
```

---

### Task 11: Implement JSON output

**Files:**
- Modify: `src/commands/list/json.rs`

- [ ] **Step 1: Replace the stub with full implementation and tests**

```rust
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
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib commands::list::json::tests`
Expected: all PASS

- [ ] **Step 3: Commit**

```bash
git add src/commands/list/json.rs
git commit -m "feat: implement JSON output for list command"
```

---

### Task 12: Integration tests

**Files:**
- Modify: `tests/test_list.rs`

- [ ] **Step 1: Update existing tests for new List struct variant**

The existing integration tests run `junktree list` with no flags, which now uses the rich output path. They should still pass since the output still contains the same key strings. Verify:

Run: `cargo test --test test_list`
Expected: all 4 existing tests PASS

- [ ] **Step 2: Add integration tests for flags and rich status**

Add to `tests/test_list.rs`:

```rust
#[test]
fn list_short_flag_shows_minimal_output() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("junktree")
        .unwrap()
        .args(["list", "--short"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("main"))
        .stdout(predicate::str::contains("Worktrees:"));
}

#[test]
fn list_json_flag_produces_valid_json() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    let output = Command::cargo_bin("junktree")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should be valid JSON array
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed.is_array());
    let arr = parsed.as_array().unwrap();
    assert!(!arr.is_empty());
    // First entry should be "main"
    assert_eq!(arr[0]["name"], "main");
    assert_eq!(arr[0]["is_current"], true);
}

#[test]
fn list_no_path_flag_hides_paths() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    let output = Command::cargo_bin("junktree")
        .unwrap()
        .args(["list", "--no-path"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    // Path line starts with 7 spaces — should NOT be present
    let lines: Vec<&str> = stdout.lines().collect();
    let path_lines: Vec<&&str> = lines.iter().filter(|l| l.starts_with("       /") || l.starts_with("       C:") || l.starts_with("       D:")).collect();
    assert!(path_lines.is_empty(), "No path lines should be present with --no-path");
}

#[test]
fn list_shows_dirty_status_for_modified_files() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a dirty file
    std::fs::write(main_wt.join("dirty.txt"), "dirty content").unwrap();

    Command::cargo_bin("junktree")
        .unwrap()
        .arg("list")
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("?"));
}

#[test]
fn list_json_includes_dirty_status() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    std::fs::write(main_wt.join("dirty.txt"), "dirty content").unwrap();

    let output = Command::cargo_bin("junktree")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr[0]["dirty"]["untracked"], true);
}
```

- [ ] **Step 3: Add serde_json to dev-dependencies for test assertions**

In `Cargo.toml`, add to `[dev-dependencies]`:

```toml
serde_json = "1"
```

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: all PASS

- [ ] **Step 5: Commit**

```bash
git add tests/test_list.rs Cargo.toml Cargo.lock
git commit -m "test: add integration tests for rich list output and CLI flags"
```

---

### Task 13: Final verification and cleanup

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: all tests PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Test the actual output manually**

Run: `cargo run -- list`
Verify: Rich output with status columns appears (what you see depends on your repo state)

Run: `cargo run -- list --short`
Verify: Minimal output (matches old behavior)

Run: `cargo run -- list --json`
Verify: Valid JSON array

Run: `cargo run -- list --no-path`
Verify: No path lines under worktree names

- [ ] **Step 4: Commit any cleanup**

```bash
git add -A
git commit -m "chore: final cleanup for rich worktree status feature"
```
