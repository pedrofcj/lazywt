mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn remove_deletes_worktree_and_branch() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");

    // Verify worktree exists before removal
    assert!(root.join("auth").exists(), "auth worktree should exist before remove");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove", "auth", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success();

    // Verify worktree directory is gone
    assert!(
        !root.join("auth").exists(),
        "auth worktree directory should not exist after remove"
    );

    // Verify branch was deleted
    let branch_output = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "--list", "feature/auth"])
        .output()
        .expect("git branch --list failed");
    let branch_list = String::from_utf8_lossy(&branch_output.stdout);
    assert!(
        branch_list.trim().is_empty(),
        "branch 'feature/auth' should have been deleted, got: {}",
        branch_list
    );
}

#[test]
fn remove_protects_default_branch() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove", "main", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("default"));
}

#[test]
fn remove_fails_for_nonexistent_worktree() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove", "nonexistent", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn remove_requires_yes_in_non_tty() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");

    // Run without --yes (stdin is piped in tests = not a TTY)
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove", "auth"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--yes"));
}

#[test]
fn remove_y_short_flag_works() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove", "auth", "-y"])
        .current_dir(&main_wt)
        .assert()
        .success();

    // Verify worktree directory is gone
    assert!(
        !root.join("auth").exists(),
        "auth worktree directory should not exist after remove -y"
    );
}

#[test]
fn remove_warns_about_copy_file_differences() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "feature", "feature/test");

    // Create a copy-tracked file in both worktrees with different content
    std::fs::write(main_wt.join(".env"), "MAIN_VALUE=1").unwrap();
    std::fs::write(root.join("feature").join(".env"), "FEATURE_VALUE=2").unwrap();

    // Write per-repo config with copy_files setting inside the bare .git dir
    std::fs::write(
        git_dir.join("lazywt.json"),
        r#"{"copy_files": [".env"]}"#,
    )
    .unwrap();

    let temp_home = TempDir::new().unwrap();

    // Run remove -- per-repo config has copy_files set, should show diff warning
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove", "feature", "--yes"])
        .current_dir(&main_wt)
        .env("LAZYWT_HOME", temp_home.path())
        .assert()
        .success()
        .stderr(
            predicate::str::contains("differ from default worktree")
                .or(predicate::str::contains("Warning"))
                .or(predicate::str::contains("lost")),
        );

    // Verify worktree was removed
    assert!(
        !root.join("feature").exists(),
        "feature worktree should be removed after remove"
    );
}
