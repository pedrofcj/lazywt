mod common;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn remove_all_removes_non_default_worktrees() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");
    common::add_worktree(&git_dir, &root, "payments", "feature/payments");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove-all", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success();

    // Non-default worktrees should be gone
    assert!(
        !root.join("auth").exists(),
        "auth worktree directory should not exist after remove-all"
    );
    assert!(
        !root.join("payments").exists(),
        "payments worktree directory should not exist after remove-all"
    );

    // Default branch worktree should still exist
    assert!(
        root.join("main").exists(),
        "main worktree should still exist after remove-all"
    );
}

#[test]
fn remove_all_preserves_default_branch() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "temp", "feature/temp");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove-all", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success();

    // Main worktree must still exist
    assert!(
        root.join("main").exists(),
        "main worktree must be preserved after remove-all"
    );

    // Verify via git worktree list that bare entry and main branch are present
    let wt_output = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("git worktree list failed");
    let wt_list = String::from_utf8_lossy(&wt_output.stdout);
    assert!(
        wt_list.contains("bare"),
        "bare entry should still exist in worktree list"
    );
    // The main branch should still appear (checking branch ref, not path which varies by platform)
    assert!(
        wt_list.contains("branch refs/heads/main"),
        "main branch worktree should still appear in worktree list, got: {}",
        wt_list
    );
}

#[test]
fn remove_all_with_nothing_to_remove() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Only the default branch worktree exists -- nothing to remove
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove-all", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("No worktrees to remove"));
}

#[test]
fn remove_all_requires_yes_in_non_tty() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");

    // Run without --yes (stdin is piped in tests = not a TTY)
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove-all"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--yes"));
}

#[test]
fn remove_all_shows_count_on_multiple() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");
    common::add_worktree(&git_dir, &root, "dev", "feature/dev");
    common::add_worktree(&git_dir, &root, "staging", "feature/staging");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove-all", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("All 3 worktrees removed"));
}

#[test]
fn remove_all_shows_count_with_two_worktrees() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");
    common::add_worktree(&git_dir, &root, "payments", "feature/payments");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove-all", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("All 2 worktrees removed"));
}

#[test]
fn remove_all_handles_locked_worktree_failure() {
    // Exercise the failure path in remove_all (lines 89-93, 99-101) by
    // locking a worktree so git worktree remove --force fails.
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");
    common::add_worktree(&git_dir, &root, "payments", "feature/payments");

    // Lock the "auth" worktree -- git worktree remove --force should fail on locked worktrees
    std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["worktree", "lock", root.join("auth").to_str().unwrap()])
        .output()
        .expect("git worktree lock failed");

    let output = assert_cmd::Command::cargo_bin("lazywt")
        .unwrap()
        .args(["remove-all", "--yes"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The locked worktree should fail to remove, exercising the error path.
    // Success messages go to stdout, failure warnings go to stderr.
    // The summary line "1 removed, 1 failed" exercises lines 99-101.
    assert!(
        stdout.contains("Removed") || stdout.contains("removed"),
        "Should show at least one successful removal in stdout, got: {}",
        stdout
    );
    // The failure warning goes to stderr via output::warning
    assert!(
        stderr.contains("Failed to remove") || stdout.contains("failed"),
        "Should show failure warning for locked worktree, stdout: {} stderr: {}",
        stdout, stderr
    );
}
