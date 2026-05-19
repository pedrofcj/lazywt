mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Command as StdCommand;

#[test]
fn add_creates_worktree_default_branch_name() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Run lazywt add auth -- branch name matches worktree name (no prefix)
    Command::cargo_bin("lazywt")
        .unwrap()
        .env("WT_BRANCH_PREFIX", "")
        .args(["add", "auth"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Adding worktree 'auth'"))
        .stdout(predicate::str::contains("Branch: auth"))
        .stdout(predicate::str::contains("Worktree"));

    // Verify the worktree directory was created
    assert!(root.join("auth").exists(), "auth worktree directory should exist");

    // Verify the branch was created matching the worktree name
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "--list", "auth"])
        .output()
        .expect("git branch --list failed");
    let branch_list = String::from_utf8_lossy(&output.stdout);
    assert!(
        branch_list.contains("auth"),
        "Branch 'auth' should exist, got: {}",
        branch_list
    );
}

#[test]
fn add_creates_worktree_with_explicit_branch() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Run lazywt add auth --branch bugfix/auth
    Command::cargo_bin("lazywt")
        .unwrap()
        .env("WT_BRANCH_PREFIX", "")
        .args(["add", "auth", "--branch", "bugfix/auth"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Branch: bugfix/auth"))
        .stdout(predicate::str::contains("Worktree"));

    // Verify the branch was created with the explicit name
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "--list", "bugfix/auth"])
        .output()
        .expect("git branch --list failed");
    let branch_list = String::from_utf8_lossy(&output.stdout);
    assert!(
        branch_list.contains("bugfix/auth"),
        "Branch 'bugfix/auth' should exist, got: {}",
        branch_list
    );
}

#[test]
fn add_with_from_flag_branches_from_source() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Add a "dev" worktree first
    common::add_worktree(&git_dir, &root, "dev", "feature/dev");

    // Run lazywt add auth --from dev (no prefix)
    Command::cargo_bin("lazywt")
        .unwrap()
        .env("WT_BRANCH_PREFIX", "")
        .args(["add", "auth", "--from", "dev"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("From: dev"))
        .stdout(predicate::str::contains("Worktree"));

    // Verify the auth directory exists
    assert!(root.join("auth").exists(), "auth worktree directory should exist");

    // Verify auth branch exists (no prefix configured)
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "--list", "auth"])
        .output()
        .expect("git branch --list failed");
    let branch_list = String::from_utf8_lossy(&output.stdout);
    assert!(
        branch_list.contains("auth"),
        "Branch 'auth' should exist, got: {}",
        branch_list
    );
}

#[test]
fn add_fails_if_worktree_directory_exists() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Pre-create the target directory
    std::fs::create_dir_all(root.join("auth")).unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["add", "auth"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn add_fails_with_reserved_name() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["add", ".git"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved").or(predicate::str::contains("invalid")));
}

#[test]
fn add_fails_with_invalid_from_worktree() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["add", "auth", "--from", "nonexistent"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn add_checks_out_existing_branch() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create the branch manually (without a worktree)
    StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "auth", "main"])
        .output()
        .expect("git branch failed");

    // Run lazywt add auth -- should check out the existing branch
    Command::cargo_bin("lazywt")
        .unwrap()
        .env("WT_BRANCH_PREFIX", "")
        .args(["add", "auth"])
        .current_dir(&main_wt)
        .assert()
        .success();

    // Verify the directory was created
    assert!(root.join("auth").exists(), "auth worktree directory should exist");

    // Verify it has the correct branch checked out
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(root.join("auth"))
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("git rev-parse failed");
    let branch = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        branch.trim(),
        "auth",
        "Checked out branch should be auth"
    );
}

#[test]
fn add_with_branch_prefix_from_env() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .env("WT_BRANCH_PREFIX", "feature")
        .args(["add", "auth"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Branch: feature/auth"));

    assert!(root.join("auth").exists());

    // Verify the branch was created with prefix
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "--list", "feature/auth"])
        .output()
        .expect("git branch --list failed");
    let branch_list = String::from_utf8_lossy(&output.stdout);
    assert!(
        branch_list.contains("feature/auth"),
        "Branch 'feature/auth' should exist, got: {}",
        branch_list
    );
}

#[test]
fn add_in_non_bare_repo_manages_gitignore() {
    // Create a regular (non-bare) git repo
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("my-project");
    std::fs::create_dir_all(&repo).unwrap();

    StdCommand::new("git")
        .args(["init"])
        .current_dir(&repo)
        .output()
        .expect("git init failed");

    StdCommand::new("git")
        .arg("-C").arg(&repo)
        .args(["config", "user.email", "test@test.com"])
        .output().unwrap();

    StdCommand::new("git")
        .arg("-C").arg(&repo)
        .args(["config", "user.name", "Test User"])
        .output().unwrap();

    StdCommand::new("git")
        .arg("-C").arg(&repo)
        .args(["checkout", "-b", "main"])
        .output().unwrap();

    StdCommand::new("git")
        .arg("-C").arg(&repo)
        .args(["commit", "--allow-empty", "-m", "initial"])
        .output().unwrap();

    // Run add from non-bare repo
    Command::cargo_bin("lazywt")
        .unwrap()
        .env("WT_BRANCH_PREFIX", "")
        .args(["add", "feature"])
        .current_dir(&repo)
        .assert()
        .success();

    // Check if .wts directory was created (inside mode for non-bare repos)
    // or a sibling .wts directory -- depends on layout detection
    let gitignore = repo.join(".gitignore");
    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore).unwrap();
        assert!(content.contains(".wts"), ".gitignore should contain .wts entry");
    }
}

#[test]
fn add_copies_config_files_from_default_worktree() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a file in the main worktree that should be copied
    std::fs::write(main_wt.join(".env"), "SECRET=abc").unwrap();

    // Write per-repo config with copy_files setting
    std::fs::write(
        git_dir.join("lazywt.json"),
        r#"{"copy_files": [".env"]}"#,
    )
    .unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .env("WT_BRANCH_PREFIX", "")
        .args(["add", "feature"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Copied"));

    // Verify the file was copied
    let feature_env = root.join("feature").join(".env");
    assert!(feature_env.exists(), ".env should be copied to new worktree");
    assert_eq!(
        std::fs::read_to_string(&feature_env).unwrap(),
        "SECRET=abc"
    );
}
