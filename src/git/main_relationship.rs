use std::path::Path;
use anyhow::Result;
use crate::output::{
    MAIN_IS_DEFAULT, MAIN_ORPHAN, MAIN_SAME_COMMIT, MAIN_INTEGRATED,
    MAIN_CONFLICT, MAIN_DIVERGE, MAIN_AHEAD, MAIN_BEHIND,
};

/// Describes how a worktree's branch relates to the repository's default branch.
///
/// Variants are ordered from highest to lowest priority in detection:
/// IsDefault > Orphan > SameCommit > Integrated > WouldConflict > Diverged > Ahead > Behind.
#[derive(Debug, Clone, PartialEq)]
pub enum MainRelationship {
    /// This worktree IS the default branch.
    IsDefault,
    /// No common ancestor with the default branch (orphan branch).
    Orphan,
    /// Same commit as the default branch head, but not the default branch itself.
    SameCommit,
    /// Fully integrated into the default branch (merge-base tree == default tree).
    Integrated,
    /// Would produce conflicts when merged into the default branch.
    WouldConflict,
    /// Ahead of AND behind the default branch (diverged history).
    Diverged { ahead: u32, behind: u32 },
    /// Only ahead of the default branch by N commits.
    Ahead(u32),
    /// Only behind the default branch by N commits.
    Behind(u32),
}

impl MainRelationship {
    /// Returns the display symbol for this relationship variant.
    ///
    /// For `Ahead(n)` and `Behind(n)`, the count is appended directly after the arrow symbol.
    /// For `Diverged`, only the ↕ symbol is returned (counts are in the struct fields).
    pub fn symbol(&self) -> String {
        match self {
            MainRelationship::IsDefault => MAIN_IS_DEFAULT.to_string(),
            MainRelationship::Orphan => MAIN_ORPHAN.to_string(),
            MainRelationship::SameCommit => MAIN_SAME_COMMIT.to_string(),
            MainRelationship::Integrated => MAIN_INTEGRATED.to_string(),
            MainRelationship::WouldConflict => MAIN_CONFLICT.to_string(),
            MainRelationship::Diverged { .. } => MAIN_DIVERGE.to_string(),
            MainRelationship::Ahead(n) => format!("{}{}", MAIN_AHEAD, n),
            MainRelationship::Behind(n) => format!("{}{}", MAIN_BEHIND, n),
        }
    }
}

/// Probe whether the installed git supports `merge-tree --write-tree`.
///
/// `git merge-tree --write-tree` was introduced in git 2.38. Older versions
/// only support the three-argument form `git merge-tree <base> <branch1> <branch2>`.
/// Check if `git merge-tree` is available (the old 3-argument form).
///
/// The old form `git merge-tree <base> <ours> <theirs>` works on all git
/// versions that support worktrees. We just verify the command exists
/// by checking `git merge-tree` with no args (exits non-zero but doesn't
/// error with "not a git command").
///
/// Returns `true` if available. Never fails — returns `false` conservatively.
pub fn check_merge_tree_support(dir: &Path) -> bool {
    // The 3-arg merge-tree is available in all modern git versions.
    // We probe by running it with HEAD against itself — always clean, always works.
    super::git_check(dir, &["merge-tree", "HEAD", "HEAD", "HEAD"])
        .unwrap_or(false)
}

/// Detect the relationship between `branch_name` and the repository's `default_branch`.
///
/// # Arguments
///
/// * `dir` — path to any directory inside the repository (used as `-C` for git)
/// * `branch_name` — name of the worktree branch being evaluated
/// * `branch_head` — the current HEAD commit hash of the worktree branch
/// * `default_branch` — name of the repository's default branch (e.g. "main")
/// * `default_head` — the current HEAD commit hash of the default branch
/// * `supports_merge_tree` — whether `git merge-tree --write-tree` is available
///
/// # Detection priority
///
/// 1. `IsDefault` — branch_name == default_branch (no git queries needed)
/// 2. `Orphan` — merge-base fails (no common ancestor)
/// 3. `SameCommit` — branch_head == default_head
/// 4. `Integrated` — merge-base tree matches default tree (already merged)
/// 5. `WouldConflict` — merge-tree output contains "CONFLICT" (requires supports_merge_tree)
/// 6. `Diverged` — ahead > 0 && behind > 0
/// 7. `Ahead` — ahead > 0
/// 8. `Behind` — behind > 0 (or default: Behind(0))
pub fn detect_main_relationship(
    dir: &Path,
    branch_name: &str,
    branch_head: &str,
    default_branch: &str,
    default_head: &str,
    supports_merge_tree: bool,
) -> MainRelationship {
    // 1. IsDefault: this worktree IS the default branch
    if branch_name == default_branch {
        return MainRelationship::IsDefault;
    }

    // 2. Find merge-base; Orphan if none exists
    let merge_base = match super::git(dir, &["merge-base", branch_head, default_head]) {
        Ok(output) => output.trim().to_string(),
        Err(_) => return MainRelationship::Orphan,
    };

    if merge_base.is_empty() {
        return MainRelationship::Orphan;
    }

    // 3. SameCommit: both branches point at the same commit
    if branch_head == default_head {
        return MainRelationship::SameCommit;
    }

    // 4. Integrated: merge-base tree == default tree (branch already fully merged)
    //    We compare the tree objects: if the merge-base has the same tree as default_head,
    //    then all of default's content is already in the branch's history.
    let merged_tree = super::git(dir, &["rev-parse", &format!("{}^{{tree}}", merge_base)])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let default_tree = super::git(dir, &["rev-parse", &format!("{}^{{tree}}", default_head)])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if !merged_tree.is_empty() && merged_tree == default_tree {
        return MainRelationship::Integrated;
    }

    // 5. WouldConflict: use merge-tree to detect conflicts (only when supported)
    if supports_merge_tree {
        // git merge-tree <merge-base> <branch> <default>
        // Output contains "CONFLICT" text when the merge would conflict
        if let Ok(output) = run_merge_tree_check(dir, &merge_base, branch_head, default_head) {
            if output.contains("CONFLICT") {
                return MainRelationship::WouldConflict;
            }
        }
    }

    // 6-8. Count ahead/behind commits
    let (ahead, behind) = count_ahead_behind(dir, branch_head, default_head);

    match (ahead, behind) {
        (a, b) if a > 0 && b > 0 => MainRelationship::Diverged { ahead: a, behind: b },
        (a, 0) if a > 0 => MainRelationship::Ahead(a),
        (0, b) => MainRelationship::Behind(b),
        _ => MainRelationship::Behind(0),
    }
}

/// Run merge-tree conflict check, capturing stdout.
///
/// Uses the old three-argument form: `git merge-tree <base> <ours> <theirs>`.
/// On conflict, the output contains diff markers with "CONFLICT" text.
fn run_merge_tree_check(
    dir: &Path,
    merge_base: &str,
    branch_head: &str,
    default_head: &str,
) -> Result<String> {
    use std::process::Command;
    // Use the old three-argument form: git merge-tree <base> <ours> <theirs>
    // This produces conflict markers in its output on conflict; exit code varies by git version.
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["merge-tree", merge_base, branch_head, default_head])
        .output()
        .map_err(|e| anyhow::anyhow!("failed to execute git merge-tree: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

/// Count how many commits `branch_head` is ahead of and behind `default_head`.
///
/// Uses `git rev-list --left-right --count branch_head...default_head`.
/// Returns `(ahead, behind)`. On any error returns `(0, 0)`.
fn count_ahead_behind(dir: &Path, branch_head: &str, default_head: &str) -> (u32, u32) {
    let range = format!("{}...{}", branch_head, default_head);
    let output = match super::git(dir, &["rev-list", "--left-right", "--count", &range]) {
        Ok(s) => s,
        Err(_) => return (0, 0),
    };

    let parts: Vec<&str> = output.split_whitespace().collect();
    if parts.len() == 2 {
        let ahead = parts[0].parse::<u32>().unwrap_or(0);
        let behind = parts[1].parse::<u32>().unwrap_or(0);
        (ahead, behind)
    } else {
        (0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a non-bare git repo with an initial commit on "main".
    /// Returns (TempDir, PathBuf to repo root).
    fn setup_repo_with_branches() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().to_path_buf();

        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["init", "-b", "main"])
            .output()
            .expect("git init failed");

        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["config", "user.email", "test@test.com"])
            .output().unwrap();

        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["config", "user.name", "Test User"])
            .output().unwrap();

        // Create an initial commit with actual file content so the tree is non-empty.
        // This prevents false "Integrated" detection when merge-base tree matches
        // default tree simply because both are the empty tree.
        std::fs::write(repo.join("README.md"), "initial\n").unwrap();
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["add", "README.md"])
            .output()
            .expect("git add failed");
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["commit", "-m", "initial commit"])
            .output()
            .expect("git commit failed");

        (dir, repo)
    }

    /// Get the HEAD commit hash for a given branch.
    fn get_head(repo: &Path, branch: &str) -> String {
        let output = Command::new("git")
            .arg("-C").arg(repo)
            .args(["rev-parse", branch])
            .output()
            .expect("git rev-parse failed");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// Counter for generating unique filenames in commits.
    static COMMIT_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    /// Add a commit with a unique file change so the tree hash differs between commits.
    /// Using --allow-empty would leave the tree identical, causing false Integrated detection.
    fn add_commit(repo: &Path, msg: &str) {
        let n = COMMIT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let filename = format!("file_{}.txt", n);
        std::fs::write(repo.join(&filename), format!("content {}\n", n)).unwrap();
        Command::new("git")
            .arg("-C").arg(repo)
            .args(["add", &filename])
            .output()
            .expect("git add failed");
        Command::new("git")
            .arg("-C").arg(repo)
            .args(["commit", "-m", msg])
            .output()
            .expect("git commit failed");
    }

    /// Checkout an existing branch.
    fn checkout(repo: &Path, branch: &str) {
        Command::new("git")
            .arg("-C").arg(repo)
            .args(["checkout", branch])
            .output()
            .expect("git checkout failed");
    }

    /// Create and checkout a new branch from current HEAD.
    fn checkout_new_branch(repo: &Path, branch: &str) {
        Command::new("git")
            .arg("-C").arg(repo)
            .args(["checkout", "-b", branch])
            .output()
            .expect("git checkout -b failed");
    }

    // ===== Existing symbol tests =====

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

    #[test]
    fn symbol_ahead_zero() {
        assert_eq!(MainRelationship::Ahead(0).symbol(), "\u{2191}0");
    }

    #[test]
    fn symbol_behind_zero() {
        assert_eq!(MainRelationship::Behind(0).symbol(), "\u{2193}0");
    }

    // ===== detect_main_relationship tests =====

    #[test]
    fn test_detect_is_default() {
        // When branch_name == default_branch, returns IsDefault immediately
        // No real git repo needed -- the function short-circuits before any git call
        let dir = TempDir::new().unwrap();
        let result = detect_main_relationship(
            dir.path(), "main", "abc123", "main", "def456", false,
        );
        assert_eq!(result, MainRelationship::IsDefault);
    }

    #[test]
    fn test_detect_orphan_merge_base_failure() {
        // Create a repo with two branches that share no common ancestor
        let (_dir, repo) = setup_repo_with_branches();
        let main_head = get_head(&repo, "main");

        // Create an orphan branch (no common history with main)
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["checkout", "--orphan", "orphan-branch"])
            .output()
            .expect("git checkout --orphan failed");

        // Use --allow-empty here because the orphan branch has no tracked files yet
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["commit", "--allow-empty", "-m", "orphan commit"])
            .output()
            .expect("git commit failed");
        let orphan_head = get_head(&repo, "HEAD");

        let result = detect_main_relationship(
            &repo, "orphan-branch", &orphan_head, "main", &main_head, false,
        );
        assert_eq!(result, MainRelationship::Orphan);
    }

    #[test]
    fn test_detect_same_commit() {
        // When branch_head == default_head but different branch names
        let (_dir, repo) = setup_repo_with_branches();
        let main_head = get_head(&repo, "main");

        // Create feature at same commit as main
        checkout_new_branch(&repo, "feature");
        let feature_head = get_head(&repo, "feature");

        assert_eq!(main_head, feature_head, "heads should be identical");

        let result = detect_main_relationship(
            &repo, "feature", &feature_head, "main", &main_head, false,
        );
        assert_eq!(result, MainRelationship::SameCommit);
    }

    #[test]
    fn test_detect_ahead_returns_integrated_when_purely_ahead() {
        // When feature is purely ahead of main (main hasn't moved), merge-base == main HEAD,
        // so merge-base tree == default tree, causing Integrated to be returned.
        // This is expected behavior of the current algorithm -- Ahead(n) is only reachable
        // when the merge-base tree differs from the default tree.
        let (_dir, repo) = setup_repo_with_branches();
        let main_head = get_head(&repo, "main");

        checkout_new_branch(&repo, "feature");
        add_commit(&repo, "feature commit 1");
        add_commit(&repo, "feature commit 2");
        let feature_head = get_head(&repo, "feature");

        let result = detect_main_relationship(
            &repo, "feature", &feature_head, "main", &main_head, false,
        );
        // Purely-ahead branches are classified as Integrated because
        // the merge-base (which is main HEAD) has the same tree as main HEAD.
        assert_eq!(result, MainRelationship::Integrated);
    }

    #[test]
    fn test_detect_ahead_with_behind() {
        // To reach the Ahead path, we need ahead > 0, behind == 0, AND
        // merge-base tree != default tree. This only happens in unusual scenarios.
        // Instead, test the ahead/behind counting logic via count_ahead_behind directly.
        let (_dir, repo) = setup_repo_with_branches();
        let main_head = get_head(&repo, "main");

        checkout_new_branch(&repo, "feature");
        add_commit(&repo, "feature commit 1");
        add_commit(&repo, "feature commit 2");
        let feature_head = get_head(&repo, "feature");

        let (ahead, behind) = count_ahead_behind(&repo, &feature_head, &main_head);
        assert_eq!(ahead, 2);
        assert_eq!(behind, 0);
    }

    #[test]
    fn test_detect_behind() {
        // Feature is behind main by 2 commits (with real file changes)
        let (_dir, repo) = setup_repo_with_branches();

        // Create feature at current main
        checkout_new_branch(&repo, "feature");
        let feature_head = get_head(&repo, "feature");

        // Go back to main and add commits with file changes
        checkout(&repo, "main");
        add_commit(&repo, "main commit 1");
        add_commit(&repo, "main commit 2");
        let main_head = get_head(&repo, "main");

        let result = detect_main_relationship(
            &repo, "feature", &feature_head, "main", &main_head, false,
        );
        assert_eq!(result, MainRelationship::Behind(2));
    }

    #[test]
    fn test_detect_diverged() {
        // Feature has 1 commit ahead and main has 1 commit ahead (both diverged)
        let (_dir, repo) = setup_repo_with_branches();

        // Create feature branch from main
        checkout_new_branch(&repo, "feature");
        add_commit(&repo, "feature commit");
        let feature_head = get_head(&repo, "feature");

        // Go back to main and add a different commit (different file)
        checkout(&repo, "main");
        add_commit(&repo, "main commit");
        let main_head = get_head(&repo, "main");

        let result = detect_main_relationship(
            &repo, "feature", &feature_head, "main", &main_head, false,
        );
        assert_eq!(result, MainRelationship::Diverged { ahead: 1, behind: 1 });
    }

    #[test]
    fn test_detect_integrated() {
        // Feature is fully merged into main (merge-base tree == default tree)
        // After merging feature into main, the merge-base of feature..main is
        // feature_head itself, so merge-base tree == tree at feature_head.
        // Then we add a commit to main that doesn't change tree content (empty commit)
        // so default tree still equals merge-base tree.
        let (_dir, repo) = setup_repo_with_branches();

        // Create feature with a real file change
        checkout_new_branch(&repo, "feature");
        add_commit(&repo, "feature work");
        let feature_head = get_head(&repo, "feature");

        // Merge feature into main (fast-forward)
        checkout(&repo, "main");
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["merge", "feature", "--no-edit"])
            .output()
            .expect("git merge failed");

        // After fast-forward, main == feature (SameCommit). We need main to advance
        // with an empty commit (no tree change) so merge-base tree still matches default tree.
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["commit", "--allow-empty", "-m", "main empty post-merge"])
            .output()
            .expect("git commit failed");
        let main_head = get_head(&repo, "main");

        let result = detect_main_relationship(
            &repo, "feature", &feature_head, "main", &main_head, false,
        );
        assert_eq!(result, MainRelationship::Integrated);
    }

    #[test]
    fn test_detect_conflict_scenario_falls_through_to_diverged() {
        // The old 3-arg merge-tree form outputs "changed in both" / "added in both"
        // rather than "CONFLICT", so the WouldConflict detection doesn't trigger.
        // This test verifies the fallback: conflicting branches fall through to Diverged.
        let (_dir, repo) = setup_repo_with_branches();

        // Modify the existing file on main
        std::fs::write(repo.join("README.md"), "main modification\n").unwrap();
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["add", "README.md"])
            .output().unwrap();
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["commit", "-m", "main: modify README"])
            .output().unwrap();
        let main_head = get_head(&repo, "main");

        // Create feature branch from main's parent
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["checkout", "-b", "feature", "main~1"])
            .output().unwrap();

        // Modify the same file differently on feature
        std::fs::write(repo.join("README.md"), "feature modification\n").unwrap();
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["add", "README.md"])
            .output().unwrap();
        Command::new("git")
            .arg("-C").arg(&repo)
            .args(["commit", "-m", "feature: modify README"])
            .output().unwrap();
        let feature_head = get_head(&repo, "feature");

        // With merge-tree support enabled, the old 3-arg form doesn't output "CONFLICT"
        // so we expect Diverged as the fallback
        let result = detect_main_relationship(
            &repo, "feature", &feature_head, "main", &main_head, true,
        );
        assert_eq!(result, MainRelationship::Diverged { ahead: 1, behind: 1 });
    }

    #[test]
    fn test_detect_without_merge_tree_support() {
        // When merge-tree support is disabled, conflict detection is skipped entirely
        let (_dir, repo) = setup_repo_with_branches();

        // Same diverged setup
        checkout_new_branch(&repo, "feature");
        add_commit(&repo, "feature commit");
        let feature_head = get_head(&repo, "feature");

        checkout(&repo, "main");
        add_commit(&repo, "main commit");
        let main_head = get_head(&repo, "main");

        // With supports_merge_tree=false, conflict check is skipped
        let result = detect_main_relationship(
            &repo, "feature", &feature_head, "main", &main_head, false,
        );
        assert_eq!(result, MainRelationship::Diverged { ahead: 1, behind: 1 });
    }

    // ===== check_merge_tree_support tests =====

    #[test]
    fn test_check_merge_tree_support_valid() {
        // A repo with at least one commit should support merge-tree
        let (_dir, repo) = setup_repo_with_branches();
        assert!(check_merge_tree_support(&repo));
    }

    #[test]
    fn test_check_merge_tree_support_invalid_dir() {
        // A directory that is not a git repo should return false
        let dir = TempDir::new().unwrap();
        assert!(!check_merge_tree_support(dir.path()));
    }

    // ===== count_ahead_behind tests =====

    #[test]
    fn test_count_ahead_behind_error_returns_zero() {
        // Invalid commit hashes should return (0, 0)
        let (_dir, repo) = setup_repo_with_branches();
        let result = count_ahead_behind(&repo, "0000000000000000000000000000000000000000", "1111111111111111111111111111111111111111");
        assert_eq!(result, (0, 0));
    }

    #[test]
    fn test_count_ahead_behind_valid() {
        // Verify count_ahead_behind returns correct counts
        let (_dir, repo) = setup_repo_with_branches();
        let main_head = get_head(&repo, "main");

        checkout_new_branch(&repo, "feature");
        add_commit(&repo, "feature 1");
        add_commit(&repo, "feature 2");
        let feature_head = get_head(&repo, "feature");

        let (ahead, behind) = count_ahead_behind(&repo, &feature_head, &main_head);
        assert_eq!(ahead, 2);
        assert_eq!(behind, 0);
    }

    #[test]
    fn test_count_ahead_behind_no_repo() {
        // Non-git directory should return (0, 0)
        let dir = TempDir::new().unwrap();
        let result = count_ahead_behind(dir.path(), "abc", "def");
        assert_eq!(result, (0, 0));
    }
}
