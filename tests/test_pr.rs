mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Create a local bare remote with a simulated PR ref (refs/pull/N/head).
///
/// Returns:
/// - remote_dir: TempDir keepalive for remote repo
/// - work_dir: TempDir keepalive for cloned work area
/// - main_wt: PathBuf to the main worktree (run lazywt from here)
/// - project_root: PathBuf to the project root (parent of worktrees)
fn create_repo_with_pr_ref(pr_number: u64) -> (TempDir, TempDir, PathBuf, PathBuf) {
    // 1. Create remote bare repo
    let remote_dir = tempfile::tempdir().unwrap();
    let remote_path = remote_dir.path().join("remote.git");

    let out = StdCommand::new("git")
        .args(["init", "--bare"])
        .arg(&remote_path)
        .output()
        .expect("git init --bare failed");
    assert!(out.status.success());

    // Create a temp worktree to make an initial commit on main
    let tmp_wt = remote_dir.path().join("tmp-wt");
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "add", tmp_wt.to_str().unwrap(), "-b", "main"])
        .output()
        .expect("git worktree add failed");

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
        .args(["commit", "--allow-empty", "-m", "initial"])
        .output()
        .unwrap();

    // Point HEAD to main
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .output()
        .unwrap();

    // 2. Create a "PR branch" on the remote
    let pr_wt = remote_dir.path().join("pr-wt");
    let pr_branch = format!("pr-branch-{}", pr_number);
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "worktree",
            "add",
            pr_wt.to_str().unwrap(),
            "-b",
            &pr_branch,
        ])
        .output()
        .expect("git worktree add for PR branch failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&pr_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&pr_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Make a commit on the PR branch
    std::fs::write(pr_wt.join("pr-change.txt"), "PR content").unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&pr_wt)
        .args(["add", "pr-change.txt"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&pr_wt)
        .args(["commit", "-m", "PR change"])
        .output()
        .unwrap();

    // Remove temp worktrees (to avoid locks)
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", pr_wt.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    // Create refs/pull/N/head pointing to the PR branch commit
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "update-ref",
            &format!("refs/pull/{}/head", pr_number),
            &format!("refs/heads/{}", pr_branch),
        ])
        .output()
        .expect("git update-ref failed");
    assert!(
        out.status.success(),
        "git update-ref failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 3. Clone as modern bare repo using lazywt clone
    let work_dir = tempfile::tempdir().unwrap();
    let project_root = work_dir.path().join("project");

    // Use lazywt clone to set up the project correctly
    let clone_out = StdCommand::new(env!("CARGO_BIN_EXE_lazywt"))
        .args(["clone", remote_path.to_str().unwrap(), "project"])
        .current_dir(work_dir.path())
        .output()
        .expect("lazywt clone failed");
    assert!(
        clone_out.status.success(),
        "lazywt clone failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&clone_out.stdout),
        String::from_utf8_lossy(&clone_out.stderr)
    );

    let git_dir = project_root.join(".git");

    // Add PR fetch refspec so git fetch can find refs/pull/N/head
    StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args([
            "config",
            "--add",
            "remote.origin.fetch",
            "+refs/pull/*:refs/pull/*",
        ])
        .output()
        .expect("git config --add fetch refspec failed");

    let main_wt = project_root.join("main");

    (remote_dir, work_dir, main_wt, project_root)
}

#[test]
fn pr_fallback_creates_worktree_with_default_name() {
    let (_remote_dir, _work_dir, main_wt, project_root) = create_repo_with_pr_ref(42);

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "42"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Fetching refs/pull/42/head"))
        .stdout(predicate::str::contains("PR #42 checked out to 'pr-42'"));

    // Verify the pr-42 directory exists as a worktree
    let pr_dir = project_root.join("pr-42");
    assert!(pr_dir.exists(), "pr-42 worktree directory should exist");
    assert!(
        pr_dir.join(".git").is_file(),
        "pr-42 should have a .git worktree marker file"
    );
}

#[test]
fn pr_fallback_creates_worktree_with_custom_name() {
    let (_remote_dir, _work_dir, main_wt, project_root) = create_repo_with_pr_ref(42);

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "42", "my-pr"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("checked out to 'my-pr'"));

    // Verify the custom-named directory exists (not pr-42)
    let custom_dir = project_root.join("my-pr");
    assert!(custom_dir.exists(), "'my-pr' worktree directory should exist");
    assert!(
        !project_root.join("pr-42").exists(),
        "pr-42 should NOT exist when custom name is provided"
    );
}

#[test]
fn pr_fails_if_worktree_directory_exists() {
    let (_remote_dir, _work_dir, main_wt, project_root) = create_repo_with_pr_ref(42);

    // Pre-create the pr-42 directory
    std::fs::create_dir_all(project_root.join("pr-42")).unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "42"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn pr_fails_with_invalid_target() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "not-a-number"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not parse PR number"));
}

#[test]
fn pr_fails_with_non_github_url() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "https://gitlab.com/owner/repo/pull/10"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Only GitHub URLs are supported"));
}

#[test]
fn pr_accepts_github_url() {
    let (_remote_dir, _work_dir, main_wt, _project_root) = create_repo_with_pr_ref(123);

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "https://github.com/owner/repo/pull/123"])
        .current_dir(&main_wt)
        .assert()
        .success()
        // The URL was parsed to PR number 123 and the fetch was attempted
        .stdout(predicate::str::contains("Fetching refs/pull/123/head"))
        .stdout(predicate::str::contains("PR #123 checked out to 'pr-123'"));
}

#[test]
fn pr_sets_tracking_config() {
    let (_remote_dir, _work_dir, main_wt, project_root) = create_repo_with_pr_ref(42);

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "42"])
        .current_dir(&main_wt)
        .assert()
        .success();

    let pr_dir = project_root.join("pr-42");

    // Verify tracking config: branch.pr-42.remote = origin
    let remote_output = StdCommand::new("git")
        .arg("-C")
        .arg(&pr_dir)
        .args(["config", "branch.pr-42.remote"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&remote_output.stdout).trim(),
        "origin",
        "PR branch remote should be 'origin'"
    );

    // Verify tracking config: branch.pr-42.merge = refs/pull/42/head
    let merge_output = StdCommand::new("git")
        .arg("-C")
        .arg(&pr_dir)
        .args(["config", "branch.pr-42.merge"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&merge_output.stdout).trim(),
        "refs/pull/42/head",
        "PR branch merge ref should track refs/pull/42/head"
    );
}

// ============================================================
// Mock gh binary helpers and gh-available path tests
// ============================================================

/// Get the path to the compiled mock_gh binary.
/// Uses env!("CARGO_BIN_EXE_mock_gh") which is set by cargo test.
fn mock_gh_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mock_gh"))
}

/// Create a mock `gh` binary in a temp directory.
///
/// Copies the compiled `mock_gh` binary as `gh` (with platform extension).
/// The mock reads env vars MOCK_GH_BRANCH, MOCK_GH_PR_OWNER, etc. at runtime.
///
/// Returns the TempDir (keepalive) and the directory path to prepend to PATH.
fn setup_mock_gh_dir() -> (TempDir, PathBuf) {
    let mock_dir = tempfile::tempdir().unwrap();
    let mock_path = mock_dir.path().to_path_buf();

    let src = mock_gh_exe();
    let gh_name = if cfg!(windows) { "gh.exe" } else { "gh" };
    let dst = mock_path.join(gh_name);
    std::fs::copy(&src, &dst).unwrap_or_else(|e| {
        panic!(
            "Failed to copy mock_gh from {:?} to {:?}: {}",
            src, dst, e
        )
    });

    (mock_dir, mock_path)
}

/// Build a PATH string that prepends the mock directory to the system PATH.
fn path_with_mock(mock_dir: &Path) -> String {
    let current_path = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };
    format!("{}{}{}", mock_dir.display(), sep, current_path)
}

/// Create a repo with a named branch (not just PR ref) on the remote,
/// so the gh-available path can fetch it by branch name.
fn create_repo_with_named_branch(
    pr_number: u64,
    branch_name: &str,
) -> (TempDir, TempDir, PathBuf, PathBuf) {
    // Start with the standard PR ref setup
    let (remote_dir, work_dir, main_wt, project_root) = create_repo_with_pr_ref(pr_number);

    let remote_path = remote_dir.path().join("remote.git");

    // Create the named branch on the remote pointing to same commit as PR ref
    let pr_ref = format!("refs/pull/{}/head", pr_number);
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["branch", branch_name, &pr_ref])
        .output()
        .expect("git branch failed");
    assert!(
        out.status.success(),
        "Failed to create branch '{}' on remote: {}",
        branch_name,
        String::from_utf8_lossy(&out.stderr)
    );

    // Fetch from the clone so origin/<branch_name> exists
    let git_dir = project_root.join(".git");
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["fetch", "origin"])
        .output()
        .expect("git fetch failed");
    assert!(
        out.status.success(),
        "git fetch failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    (remote_dir, work_dir, main_wt, project_root)
}

// ============================================================
// gh-available path tests: same-repo PRs
// ============================================================

#[test]
fn pr_gh_available_same_repo_creates_worktree() {
    let (_remote_dir, _work_dir, main_wt, project_root) =
        create_repo_with_named_branch(55, "feature-branch");

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "55"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_BRANCH", "feature-branch")
        .env("MOCK_GH_PR_OWNER", "myorg")
        .env("MOCK_GH_REPO_OWNER", "myorg")
        .env("MOCK_GH_REPO_NAME", "project")
        .assert()
        .success()
        .stdout(predicate::str::contains("Branch: feature-branch (from gh CLI)"))
        .stdout(predicate::str::contains("PR #55 checked out"));

    // Verify the worktree was created with default name
    let pr_dir = project_root.join("pr-55");
    assert!(pr_dir.exists(), "pr-55 worktree directory should exist");
    assert!(
        pr_dir.join(".git").is_file(),
        "pr-55 should have a .git worktree marker file"
    );
}

#[test]
fn pr_gh_available_same_repo_with_custom_name() {
    let (_remote_dir, _work_dir, main_wt, project_root) =
        create_repo_with_named_branch(55, "feature-branch");

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "55", "my-review"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_BRANCH", "feature-branch")
        .env("MOCK_GH_PR_OWNER", "myorg")
        .env("MOCK_GH_REPO_OWNER", "myorg")
        .env("MOCK_GH_REPO_NAME", "project")
        .assert()
        .success()
        .stdout(predicate::str::contains("Branch: feature-branch (from gh CLI)"))
        .stdout(predicate::str::contains("checked out to 'my-review'"));

    // Verify the custom-named worktree was created
    let custom_dir = project_root.join("my-review");
    assert!(
        custom_dir.exists(),
        "'my-review' worktree directory should exist"
    );
    assert!(
        !project_root.join("pr-55").exists(),
        "pr-55 should NOT exist when custom name is provided"
    );
}

#[test]
fn pr_gh_available_branch_already_exists() {
    let (_remote_dir, _work_dir, main_wt, project_root) =
        create_repo_with_named_branch(55, "existing-branch");

    let git_dir = project_root.join(".git");

    // Pre-create the branch locally so branch_exists() returns true
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "existing-branch", "origin/existing-branch"])
        .output()
        .expect("git branch failed");
    assert!(
        out.status.success(),
        "Failed to create local branch: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "55"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_BRANCH", "existing-branch")
        .env("MOCK_GH_PR_OWNER", "myorg")
        .env("MOCK_GH_REPO_OWNER", "myorg")
        .env("MOCK_GH_REPO_NAME", "project")
        .assert()
        .success()
        .stdout(predicate::str::contains("Branch: existing-branch (from gh CLI)"))
        .stdout(predicate::str::contains("PR #55 checked out"));

    // Verify the worktree was created (uses existing branch path)
    let pr_dir = project_root.join("pr-55");
    assert!(pr_dir.exists(), "pr-55 worktree should exist");
}

#[test]
fn pr_gh_available_same_repo_sets_upstream() {
    let (_remote_dir, _work_dir, main_wt, project_root) =
        create_repo_with_named_branch(55, "feature-branch");

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "55"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_BRANCH", "feature-branch")
        .env("MOCK_GH_PR_OWNER", "myorg")
        .env("MOCK_GH_REPO_OWNER", "myorg")
        .env("MOCK_GH_REPO_NAME", "project")
        .assert()
        .success();

    let pr_dir = project_root.join("pr-55");

    // For same-repo PRs (fetch_remote == "origin"), set_upstream is called
    // which sets upstream to origin/feature-branch. Verify the branch tracks origin.
    let remote_output = StdCommand::new("git")
        .arg("-C")
        .arg(&pr_dir)
        .args(["config", "branch.feature-branch.remote"])
        .output()
        .expect("git config failed");
    let remote_val = String::from_utf8_lossy(&remote_output.stdout)
        .trim()
        .to_string();
    assert_eq!(
        remote_val, "origin",
        "Same-repo PR branch should track origin"
    );
}

// ============================================================
// gh-available path tests: fork PRs
// ============================================================

#[test]
fn pr_gh_available_fork_pr_shows_fork_remote() {
    // Fork PR: pr_owner ("contributor") != repo_owner ("myorg")
    // The fork URL (https://github.com/contributor/project.git) is unreachable,
    // but FETCH_HEAD from the setup's git fetch origin allows worktree creation.
    // Key assertion: the gh-available fork path IS entered (Fork remote: message).
    let (_remote_dir, _work_dir, main_wt, project_root) =
        create_repo_with_named_branch(77, "fork-feature");

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "77"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_BRANCH", "fork-feature")
        .env("MOCK_GH_PR_OWNER", "contributor")
        .env("MOCK_GH_REPO_OWNER", "myorg")
        .env("MOCK_GH_REPO_NAME", "project")
        .output()
        .expect("failed to run lazywt");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The fork remote message proves we entered the gh-available fork path
    assert!(
        stdout.contains("Fork remote: https://github.com/contributor/project.git"),
        "Should show fork remote URL. Got stdout: {}",
        stdout
    );
    assert!(
        stdout.contains("Branch: fork-feature (from gh CLI)"),
        "Should show branch from gh CLI. Got stdout: {}",
        stdout
    );

    // Verify the worktree was created (FETCH_HEAD from prior fetch allows creation)
    let pr_dir = project_root.join("pr-77");
    assert!(
        pr_dir.exists(),
        "pr-77 worktree should exist (fork path via FETCH_HEAD)"
    );
}

#[test]
fn pr_gh_available_fork_pr_sets_tracking_config() {
    // For fork PRs where the fetch from fork URL fails but the branch
    // is available locally (from the PR ref), we can still verify
    // the tracking config is set correctly.
    let (_remote_dir, _work_dir, main_wt, project_root) =
        create_repo_with_named_branch(77, "fork-feature");

    let git_dir = project_root.join(".git");

    // Pre-create the branch locally so even if fork fetch fails,
    // worktree creation can succeed using the existing branch
    let out = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "fork-feature", "origin/fork-feature"])
        .output()
        .expect("git branch failed");
    assert!(
        out.status.success(),
        "Failed to create local branch: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "77"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_BRANCH", "fork-feature")
        .env("MOCK_GH_PR_OWNER", "contributor")
        .env("MOCK_GH_REPO_OWNER", "myorg")
        .env("MOCK_GH_REPO_NAME", "project")
        .assert()
        .success()
        .stdout(predicate::str::contains("Fork remote:"))
        .stdout(predicate::str::contains("PR #77 checked out"));

    let pr_dir = project_root.join("pr-77");

    // For fork PRs, tracking is set to refs/pull/N/head on origin
    let remote_output = StdCommand::new("git")
        .arg("-C")
        .arg(&pr_dir)
        .args(["config", "branch.fork-feature.remote"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&remote_output.stdout).trim(),
        "origin",
        "Fork PR branch remote should be 'origin'"
    );

    let merge_output = StdCommand::new("git")
        .arg("-C")
        .arg(&pr_dir)
        .args(["config", "branch.fork-feature.merge"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&merge_output.stdout).trim(),
        "refs/pull/77/head",
        "Fork PR branch merge ref should track refs/pull/77/head"
    );
}

// ============================================================
// gh failure/edge case tests (fallback path)
// ============================================================

#[test]
fn pr_gh_command_fails_falls_back_to_git_fetch() {
    let (_remote_dir, _work_dir, main_wt, project_root) = create_repo_with_pr_ref(99);

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "99"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_EXIT_CODE", "1")
        .assert()
        .success()
        // When gh fails, falls back to git fetch
        .stdout(predicate::str::contains("gh CLI not available"))
        .stdout(predicate::str::contains("Fetching refs/pull/99/head"))
        .stdout(predicate::str::contains("PR #99 checked out to 'pr-99'"));

    let pr_dir = project_root.join("pr-99");
    assert!(pr_dir.exists(), "pr-99 worktree should exist via fallback");
}

#[test]
fn pr_gh_returns_empty_output_falls_back() {
    let (_remote_dir, _work_dir, main_wt, project_root) = create_repo_with_pr_ref(88);

    let (_mock_keepalive, mock_path) = setup_mock_gh_dir();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["pr", "88"])
        .current_dir(&main_wt)
        .env("PATH", path_with_mock(&mock_path))
        .env("MOCK_GH_EMPTY_OUTPUT", "1")
        .assert()
        .success()
        // Empty output from gh causes None return (lines.next() -> None) -> fallback
        .stdout(predicate::str::contains("gh CLI not available"))
        .stdout(predicate::str::contains("PR #88 checked out to 'pr-88'"));

    let pr_dir = project_root.join("pr-88");
    assert!(
        pr_dir.exists(),
        "pr-88 worktree should exist via fallback after empty gh response"
    );
}
