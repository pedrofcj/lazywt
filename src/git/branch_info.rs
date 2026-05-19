use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;

/// Per-branch metadata collected from `git for-each-ref`.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchInfo {
    /// Tracking upstream branch, e.g. "origin/main". None when no upstream is set.
    pub upstream: Option<String>,
    /// Age of the most-recent commit in seconds (Unix timestamp delta from now).
    /// Computed as `now - committerdate:unix`. Negative means clock skew.
    pub commit_age_secs: i64,
    /// Subject line of the most-recent commit.
    pub commit_message: String,
}

// ------------------------------------------------------------------
// Parser
// ------------------------------------------------------------------

/// Parse the output of `git for-each-ref` formatted with four pipe-separated fields:
///
/// ```text
/// %(refname:short)|%(upstream:short)|%(committerdate:unix)|%(subject)
/// ```
///
/// Uses `splitn(4, '|')` so pipes inside the subject do not break parsing.
/// Lines that do not produce exactly 4 fields are silently skipped (defensive).
pub fn parse_for_each_ref(output: &str) -> HashMap<String, BranchInfo> {
    let now = unix_now();
    let mut map = HashMap::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Split into at most 4 parts so pipes in the subject are preserved.
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() != 4 {
            // Malformed line — skip defensively.
            continue;
        }

        let branch_name = parts[0].trim();
        let upstream_raw = parts[1].trim();
        let timestamp_raw = parts[2].trim();
        let subject = parts[3].trim();

        if branch_name.is_empty() {
            continue;
        }

        let upstream = if upstream_raw.is_empty() {
            None
        } else {
            Some(upstream_raw.to_string())
        };

        let commit_age_secs = if let Ok(ts) = timestamp_raw.parse::<i64>() {
            now - ts
        } else {
            // Invalid timestamp — treat as very old (i64::MAX would be extreme; use 0 as a
            // neutral sentinel so callers can still display the entry).
            0
        };

        map.insert(
            branch_name.to_string(),
            BranchInfo {
                upstream,
                commit_age_secs,
                commit_message: subject.to_string(),
            },
        );
    }

    map
}

// ------------------------------------------------------------------
// Git queries
// ------------------------------------------------------------------

/// Run `git for-each-ref --format=... refs/heads` in `dir` and return a map of
/// branch name -> BranchInfo for every local branch.
pub fn batch_branch_info(dir: &Path) -> Result<HashMap<String, BranchInfo>> {
    let format = "%(refname:short)|%(upstream:short)|%(committerdate:unix)|%(subject)";
    let output = super::git(dir, &["for-each-ref", &format!("--format={}", format), "refs/heads"])?;
    Ok(parse_for_each_ref(&output))
}

/// Return the name of the current branch in `dir`.
/// Runs `git rev-parse --abbrev-ref HEAD`.
/// Falls back to `"main"` on any error (e.g. detached HEAD, empty repo).
pub fn detect_default_branch(dir: &Path) -> String {
    match super::git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(out) => {
            let trimmed = out.trim();
            if trimmed.is_empty() || trimmed == "HEAD" {
                "main".to_string()
            } else {
                trimmed.to_string()
            }
        }
        Err(_) => "main".to_string(),
    }
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

/// Return the current Unix timestamp as i64.
/// Uses std::time so there is no external dependency.
fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    // ----------------------------------------------------------------
    // parse_for_each_ref unit tests (pure, no git subprocess needed)
    // ----------------------------------------------------------------

    #[test]
    fn empty_input_returns_empty_map() {
        let result = parse_for_each_ref("");
        assert!(result.is_empty(), "empty input should produce empty map");
    }

    #[test]
    fn single_branch_with_upstream() {
        // Use a fixed timestamp far in the past so commit_age_secs is large and positive.
        let ts: i64 = 1_000_000;
        let line = format!("main|origin/main|{}|Initial commit", ts);
        let result = parse_for_each_ref(&line);

        assert_eq!(result.len(), 1);
        let info = result.get("main").expect("main key should exist");
        assert_eq!(info.upstream, Some("origin/main".to_string()));
        assert!(info.commit_age_secs > 0, "age should be positive for old timestamp");
        assert_eq!(info.commit_message, "Initial commit");
    }

    #[test]
    fn branch_with_no_upstream() {
        let ts: i64 = 1_000_000;
        let line = format!("feature||{}|Add feature", ts);
        let result = parse_for_each_ref(&line);

        assert_eq!(result.len(), 1);
        let info = result.get("feature").expect("feature key should exist");
        assert_eq!(info.upstream, None);
        assert_eq!(info.commit_message, "Add feature");
    }

    #[test]
    fn multiple_branches() {
        let ts: i64 = 1_000_000;
        let input = format!(
            "main|origin/main|{}|Initial commit\nfeature||{}|Add feature\nhotfix|origin/hotfix|{}|Fix bug",
            ts, ts, ts
        );
        let result = parse_for_each_ref(&input);

        assert_eq!(result.len(), 3);
        assert!(result.contains_key("main"));
        assert!(result.contains_key("feature"));
        assert!(result.contains_key("hotfix"));

        let hotfix = result.get("hotfix").unwrap();
        assert_eq!(hotfix.upstream, Some("origin/hotfix".to_string()));
        assert_eq!(hotfix.commit_message, "Fix bug");
    }

    #[test]
    fn commit_message_with_pipes() {
        // The subject contains a pipe character — splitn(4) must preserve it.
        let ts: i64 = 1_000_000;
        let line = format!("main|origin/main|{}|Merge: foo|bar", ts);
        let result = parse_for_each_ref(&line);

        assert_eq!(result.len(), 1);
        let info = result.get("main").unwrap();
        assert_eq!(info.commit_message, "Merge: foo|bar");
    }

    #[test]
    fn invalid_timestamp_uses_zero_age() {
        let line = "main|origin/main|not-a-number|Some commit";
        let result = parse_for_each_ref(line);

        assert_eq!(result.len(), 1);
        let info = result.get("main").unwrap();
        // Invalid timestamp falls back to age = 0.
        assert_eq!(info.commit_age_secs, 0);
    }

    #[test]
    fn malformed_line_is_skipped() {
        // Only 2 fields — not enough, should be skipped.
        let line = "main|origin/main";
        let result = parse_for_each_ref(line);
        assert!(result.is_empty(), "malformed line with < 4 fields should be skipped");
    }

    // ----------------------------------------------------------------
    // Integration tests — require a real git repo
    // ----------------------------------------------------------------

    /// Create a non-bare git repo with a single commit on "main".
    /// Returns (repo_dir TempDir). The TempDir must be kept alive.
    fn setup_repo_with_commit() -> TempDir {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        Command::new("git").args(["init", "-b", "main"]).arg(path).output().expect("git init");
        Command::new("git")
            .arg("-C").arg(path)
            .args(["config", "user.email", "test@test.com"])
            .output().expect("git config email");
        Command::new("git")
            .arg("-C").arg(path)
            .args(["config", "user.name", "Test"])
            .output().expect("git config name");
        Command::new("git")
            .arg("-C").arg(path)
            .args(["commit", "--allow-empty", "-m", "initial"])
            .output().expect("git commit");

        dir
    }

    #[test]
    fn batch_branch_info_returns_entry_for_main() {
        let dir = setup_repo_with_commit();
        let result = batch_branch_info(dir.path());
        assert!(result.is_ok(), "batch_branch_info should succeed: {:?}", result);
        let map = result.unwrap();
        assert!(map.contains_key("main"), "should have an entry for 'main'");
        let info = map.get("main").unwrap();
        assert_eq!(info.upstream, None, "no upstream set");
        assert_eq!(info.commit_message, "initial");
    }

    #[test]
    fn detect_default_branch_returns_current_branch() {
        let dir = setup_repo_with_commit();
        let branch = detect_default_branch(dir.path());
        assert_eq!(branch, "main");
    }

    #[test]
    fn detect_default_branch_falls_back_to_main_on_error() {
        // Pass a path that is not a git repo.
        let dir = TempDir::new().unwrap();
        let branch = detect_default_branch(dir.path());
        assert_eq!(branch, "main", "should fall back to 'main' on error");
    }
}
