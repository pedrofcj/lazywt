mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Helper: create a local bare repo that acts as a remote, clone it as a
/// modern bare layout, and set up a `main` worktree in the clone.
///
/// Returns (remote_keepalive, clone_keepalive, remote_path, clone_git_dir, clone_root, main_wt_path).
fn create_bare_repo_with_remote() -> (TempDir, TempDir, PathBuf, PathBuf, PathBuf, PathBuf) {
    // --- 1. Create the "remote" bare repo with an initial commit ---
    let remote_dir = tempfile::tempdir().unwrap();
    let remote_path = remote_dir.path().join("remote.git");

    StdCommand::new("git")
        .args(["init", "--bare"])
        .arg(&remote_path)
        .output()
        .expect("git init --bare (remote) failed");

    // Add initial commit via a temporary worktree
    let tmp_wt = remote_dir.path().join("tmp-wt");
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "add", tmp_wt.to_str().unwrap(), "-b", "main"])
        .output()
        .expect("worktree add tmp-wt failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", "initial commit"])
        .output()
        .expect("initial commit failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .output()
        .unwrap();

    // Remove temp worktree
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    // --- 2. Clone into a modern bare layout ---
    let work_dir = tempfile::tempdir().unwrap();
    let clone_root = work_dir.path().join("project");
    let clone_git_dir = clone_root.join(".git");

    StdCommand::new("git")
        .args(["clone", "--bare"])
        .arg(format!("file://{}", remote_path.to_str().unwrap()))
        .arg(clone_git_dir.to_str().unwrap())
        .output()
        .expect("git clone --bare failed");

    // Mark as modern layout
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args(["config", "wt.layout", "modern"])
        .output()
        .unwrap();

    // Fix fetch refspec (bare clones default to +refs/heads/*:refs/heads/*)
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args([
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ])
        .output()
        .unwrap();

    // Initial fetch to populate remote tracking branches
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args(["fetch", "--all"])
        .output()
        .unwrap();

    // Create main worktree in clone
    std::fs::create_dir_all(&clone_root).ok();
    let main_wt = clone_root.join("main");
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args(["worktree", "add", main_wt.to_str().unwrap(), "main"])
        .output()
        .expect("worktree add main failed");

    // Configure git user in main worktree
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Set upstream for main
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["branch", "--set-upstream-to=origin/main", "main"])
        .output()
        .unwrap();

    (remote_dir, work_dir, remote_path, clone_git_dir, clone_root, main_wt)
}

/// Push a new empty commit to the remote bare repo (via a temporary worktree).
fn push_commit_to_remote(remote_path: &Path, branch: &str, message: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let tmp_wt = tmp.path().join("push-wt");

    StdCommand::new("git")
        .arg("-C")
        .arg(remote_path)
        .args(["worktree", "add", tmp_wt.to_str().unwrap(), branch])
        .output()
        .expect("worktree add for push failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", message])
        .output()
        .expect("commit for push failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();
}

// ---- Test cases ----

#[test]
fn sync_fetches_and_fast_forwards_default_branch() {
    let (_rd, _wd, remote_path, _git_dir, _root, main_wt) = create_bare_repo_with_remote();

    // Push a new commit to remote's main branch
    push_commit_to_remote(&remote_path, "main", "remote update");

    // Run sync from main worktree
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Fast-forwarded main"));
}

#[test]
fn sync_reports_gone_branches() {
    let (_rd, _wd, remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a branch on the remote
    let tmp = tempfile::tempdir().unwrap();
    let tmp_wt = tmp.path().join("br-wt");
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "worktree",
            "add",
            tmp_wt.to_str().unwrap(),
            "-b",
            "feature-gone",
        ])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", "feature work"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    // Fetch so clone knows about the remote branch
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args(["fetch", "--all"])
        .output()
        .unwrap();

    // Create a local worktree tracking that remote branch
    let feature_wt = clone_root.join("feature-gone");
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args([
            "worktree",
            "add",
            feature_wt.to_str().unwrap(),
            "-b",
            "feature-gone",
            "origin/feature-gone",
        ])
        .output()
        .unwrap();

    // Set upstream tracking
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args([
            "branch",
            "--set-upstream-to=origin/feature-gone",
            "feature-gone",
        ])
        .output()
        .unwrap();

    // Now delete the branch on the remote
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["branch", "-D", "feature-gone"])
        .output()
        .unwrap();

    // Run sync -- should fetch (prune stale remotes) and report gone branch
    // warning() goes to stderr
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stderr(predicate::str::contains("tracking deleted remote branch").or(
            predicate::str::contains("feature-gone"),
        ));
}

#[test]
fn sync_all_fast_forwards_non_default_branches() {
    let (_rd, _wd, remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a feature branch on the remote
    let tmp = tempfile::tempdir().unwrap();
    let tmp_wt = tmp.path().join("feat-wt");
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "worktree",
            "add",
            tmp_wt.to_str().unwrap(),
            "-b",
            "feature-x",
        ])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", "feature commit 1"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    // Fetch in clone so we know about the branch
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args(["fetch", "--all"])
        .output()
        .unwrap();

    // Create local worktree tracking that remote branch
    let feature_wt = clone_root.join("feature-x");
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args([
            "worktree",
            "add",
            feature_wt.to_str().unwrap(),
            "-b",
            "feature-x",
            "origin/feature-x",
        ])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args([
            "branch",
            "--set-upstream-to=origin/feature-x",
            "feature-x",
        ])
        .output()
        .unwrap();

    // Push another commit to the remote feature branch
    push_commit_to_remote(&remote_path, "feature-x", "feature commit 2");

    // Run sync --all
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync", "--all"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("fast-forwarded"));
}

#[test]
fn sync_all_skips_dirty_worktrees() {
    let (_rd, _wd, remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a feature branch on remote
    let tmp = tempfile::tempdir().unwrap();
    let tmp_wt = tmp.path().join("dirty-wt");
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "worktree",
            "add",
            tmp_wt.to_str().unwrap(),
            "-b",
            "feature-dirty",
        ])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", "feature dirty commit"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    // Fetch in clone
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args(["fetch", "--all"])
        .output()
        .unwrap();

    // Create local worktree tracking the feature branch
    let feature_wt = clone_root.join("feature-dirty");
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args([
            "worktree",
            "add",
            feature_wt.to_str().unwrap(),
            "-b",
            "feature-dirty",
            "origin/feature-dirty",
        ])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args([
            "branch",
            "--set-upstream-to=origin/feature-dirty",
            "feature-dirty",
        ])
        .output()
        .unwrap();

    // Make the worktree dirty (create an untracked file)
    std::fs::write(feature_wt.join("dirty.txt"), "uncommitted").unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["add", "dirty.txt"])
        .output()
        .unwrap();

    // Run sync --all -- warning() goes to stderr
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync", "--all"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stderr(predicate::str::contains("skipped (dirty)"));
}

#[test]
fn sync_all_skips_branches_without_upstream() {
    let (_rd, _wd, _remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a local-only worktree (no upstream configured)
    let local_wt = clone_root.join("local-only");
    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args([
            "worktree",
            "add",
            local_wt.to_str().unwrap(),
            "-b",
            "local-only",
        ])
        .output()
        .unwrap();

    // Explicitly unset any upstream tracking to ensure no upstream
    StdCommand::new("git")
        .arg("-C")
        .arg(&local_wt)
        .args(["branch", "--unset-upstream", "local-only"])
        .output()
        .ok(); // may fail if never set, that's fine

    // Run sync --all
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync", "--all"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped (no upstream)"));
}

#[test]
fn sync_handles_ff_failure_gracefully() {
    let (_rd, _wd, remote_path, _clone_git_dir, _clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Push a new commit to remote's main so it diverges from the clone's main
    push_commit_to_remote(&remote_path, "main", "remote-only commit");

    // Also make a local commit in the clone's main worktree so it diverges
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "local-only commit"])
        .output()
        .unwrap();

    // Run sync -- should warn about ff failure but not error
    // warning() goes to stderr
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stderr(predicate::str::contains("Could not fast-forward"));
}

#[test]
fn sync_shows_header_and_completion() {
    let (_rd, _wd, _remote_path, _clone_git_dir, _clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Run sync on a clean repo
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Syncing worktrees"))
        .stdout(predicate::str::contains("Fetched all remotes"))
        .stdout(predicate::str::contains("Sync complete"));
}

// ---- PR-style upstream tracking tests (sync.rs lines 131-168) ----

/// Helper: create a worktree in a clone with PR-style tracking config.
/// Sets branch.{branch}.merge = refs/pull/{pr_num}/head and
/// branch.{branch}.remote = origin, and unsets normal upstream.
fn setup_pr_style_worktree(
    clone_git_dir: &Path,
    clone_root: &Path,
    branch: &str,
    pr_num: u64,
) -> PathBuf {
    let wt_path = clone_root.join(branch);

    // Create worktree with a new branch
    StdCommand::new("git")
        .arg("-C")
        .arg(clone_git_dir)
        .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", branch])
        .output()
        .expect("worktree add for PR branch failed");

    // Configure git user
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Unset normal upstream (so @{upstream} fails)
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["branch", "--unset-upstream", branch])
        .output()
        .ok(); // may fail if never set

    // Set PR-style tracking config
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args([
            "config",
            &format!("branch.{}.remote", branch),
            "origin",
        ])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args([
            "config",
            &format!("branch.{}.merge", branch),
            &format!("refs/pull/{}/head", pr_num),
        ])
        .output()
        .unwrap();

    wt_path
}

#[test]
fn sync_all_fast_forwards_pr_style_upstream() {
    let (_rd, _wd, remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a PR-style worktree in the clone
    let _pr_wt = setup_pr_style_worktree(&clone_git_dir, &clone_root, "pr-42", 42);

    // On the remote, create a branch with a new commit, then point refs/pull/42/head to it
    let tmp = tempfile::tempdir().unwrap();
    let tmp_wt = tmp.path().join("pr-commit-wt");
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "worktree",
            "add",
            tmp_wt.to_str().unwrap(),
            "-b",
            "pr-source-42",
        ])
        .output()
        .expect("worktree add for PR source failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", "PR commit ahead"])
        .output()
        .unwrap();

    // Get the SHA of the new commit
    let sha_output = StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let sha = String::from_utf8_lossy(&sha_output.stdout).trim().to_string();

    // Remove temp worktree
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    // Create refs/pull/42/head on the remote pointing to the new commit
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["update-ref", "refs/pull/42/head", &sha])
        .output()
        .expect("git update-ref failed");

    // Run sync --all
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync", "--all"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("fast-forwarded"));
}

#[test]
fn sync_all_pr_upstream_diverged() {
    let (_rd, _wd, remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a PR-style worktree in the clone
    let pr_wt = setup_pr_style_worktree(&clone_git_dir, &clone_root, "pr-50", 50);

    // Make a local commit on the PR branch so it diverges from the remote ref
    StdCommand::new("git")
        .arg("-C")
        .arg(&pr_wt)
        .args(["commit", "--allow-empty", "-m", "local PR commit"])
        .output()
        .expect("local commit on PR branch failed");

    // On the remote, create a DIFFERENT commit and point refs/pull/50/head to it.
    // This creates divergence: local pr-50 has one commit, remote PR ref has a different one.
    let tmp = tempfile::tempdir().unwrap();
    let tmp_wt = tmp.path().join("pr-diverge-wt");
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "worktree",
            "add",
            tmp_wt.to_str().unwrap(),
            "-b",
            "pr-source-50",
        ])
        .output()
        .unwrap();

    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", "remote PR diverge commit"])
        .output()
        .unwrap();

    let sha_output = StdCommand::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let sha = String::from_utf8_lossy(&sha_output.stdout).trim().to_string();

    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    // Create refs/pull/50/head pointing to the diverged commit
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["update-ref", "refs/pull/50/head", &sha])
        .output()
        .expect("git update-ref failed");

    // Run sync --all -- fetch succeeds but merge --ff-only fails (diverged)
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync", "--all"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date or diverged"));
}

#[test]
fn sync_all_pr_upstream_fetch_fails() {
    let (_rd, _wd, _remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a PR-style worktree pointing to a non-existent PR ref
    let _pr_wt = setup_pr_style_worktree(&clone_git_dir, &clone_root, "pr-99", 99);
    // Do NOT create refs/pull/99/head on the remote -- fetch will fail

    // Run sync --all -- warning() goes to stderr
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync", "--all"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stderr(predicate::str::contains("fetch failed for refs/pull/99/head"));
}

#[test]
fn sync_all_pr_branch_with_non_pr_merge_ref() {
    let (_rd, _wd, _remote_path, clone_git_dir, clone_root, main_wt) =
        create_bare_repo_with_remote();

    // Create a worktree with a branch that has a merge ref but NOT a refs/pull/ ref
    let branch = "custom-tracked";
    let wt_path = clone_root.join(branch);

    StdCommand::new("git")
        .arg("-C")
        .arg(&clone_git_dir)
        .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", branch])
        .output()
        .expect("worktree add failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Unset normal upstream
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args(["branch", "--unset-upstream", branch])
        .output()
        .ok();

    // Set merge config to a non-PR ref
    StdCommand::new("git")
        .arg("-C")
        .arg(&wt_path)
        .args([
            "config",
            &format!("branch.{}.merge", branch),
            "refs/heads/some-branch",
        ])
        .output()
        .unwrap();

    // Run sync --all -- should fall through to "skipped (no upstream)"
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["sync", "--all"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped (no upstream)"));
}
