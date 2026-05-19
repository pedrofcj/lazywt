mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

/// Read the nav file from a given home directory.
fn read_nav_file(home: &Path) -> Option<String> {
    std::fs::read_to_string(home.join(".lazywt_cd")).ok()
}

#[test]
fn switch_exact_match_succeeds() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let _feature_wt = common::add_worktree(&git_dir, &root, "feature-auth", "feature/auth");

    // Use isolated LAZYWT_HOME to avoid race with other tests
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "feature-auth"])
        .current_dir(root.join("main"))
        .env("LAZYWT_HOME", temp_home.path())
        .assert()
        .success();

    // Nav file should contain the path to feature-auth worktree
    let nav = read_nav_file(temp_home.path());
    assert!(nav.is_some(), "nav file should exist after switch");
    let nav_content = nav.unwrap();
    assert!(
        nav_content.contains("feature-auth"),
        "nav file should point to feature-auth, got: {}",
        nav_content
    );
}

#[test]
fn switch_previous_with_env_var() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Use isolated LAZYWT_HOME to avoid race with other tests
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "-"])
        .current_dir(&main_wt)
        .env("LAZYWT_HOME", temp_home.path())
        .env("LAZYWT_PREV", main_wt.to_str().unwrap())
        .assert()
        .success();

    let nav = read_nav_file(temp_home.path());
    assert!(nav.is_some(), "nav file should exist after switch -");
    let nav_content = nav.unwrap();
    assert!(
        nav_content.contains("main"),
        "nav file should point to main, got: {}",
        nav_content
    );
}

#[test]
fn switch_previous_without_env_var_fails() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "-"])
        .current_dir(&main_wt)
        .env("LAZYWT_HOME", temp_home.path())
        .env_remove("LAZYWT_PREV")
        .assert()
        .failure()
        .stderr(predicate::str::contains("LAZYWT_PREV"));
}

#[test]
fn switch_no_match_fails() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "nonexistent-worktree-xyz"])
        .current_dir(&main_wt)
        .env("LAZYWT_HOME", temp_home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No worktree found matching"));
}

#[test]
fn switch_interactive_fails_without_tty() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    let temp_home = tempfile::tempdir().unwrap();

    // No args -> interactive mode, but no TTY in test environment
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch"])
        .current_dir(&main_wt)
        .env("LAZYWT_HOME", temp_home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal"));
}

#[test]
fn switch_short_input_no_fuzzy_no_substring() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let _wt1 = common::add_worktree(&git_dir, &root, "abc", "branch-abc");
    let _wt2 = common::add_worktree(&git_dir, &root, "def", "branch-def");
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "z"])
        .current_dir(root.join("main"))
        .env("LAZYWT_HOME", temp_home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("No worktree found matching"));
}

#[test]
fn switch_fuzzy_multiple_candidates() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let _wt1 = common::add_worktree(&git_dir, &root, "auth", "branch-auth");
    let _wt2 = common::add_worktree(&git_dir, &root, "auto", "branch-auto");
    let _wt3 = common::add_worktree(&git_dir, &root, "autz", "branch-autz");
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "autx"])
        .current_dir(root.join("main"))
        .env("LAZYWT_HOME", temp_home.path())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Did you mean")
                .or(predicate::str::contains("closest"))
                .or(predicate::str::contains("candidates")),
        );
}

// --- New tests for 09-05 gap closure ---

#[test]
fn switch_substring_single_match_non_tty() {
    // "feat" is a substring of "feature-auth" and only "feature-auth"
    // In non-TTY (test) environment, this should fail with "No exact match" or "closest"
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let _wt = common::add_worktree(&git_dir, &root, "feature-auth", "feature/auth");
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "feat"])
        .current_dir(root.join("main"))
        .env("LAZYWT_HOME", temp_home.path())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("No exact match")
                .or(predicate::str::contains("closest")),
        );
}

#[test]
fn switch_previous_nonexistent_path_fails() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let temp_home = tempfile::tempdir().unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["switch", "-"])
        .current_dir(root.join("main"))
        .env("LAZYWT_HOME", temp_home.path())
        .env("LAZYWT_PREV", "/nonexistent/path/that/does/not/exist")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no longer exists"));
}
