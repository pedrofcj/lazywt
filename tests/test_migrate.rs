mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use std::process;

// ---------------------------------------------------------------------------
// Existing tests
// ---------------------------------------------------------------------------

#[test]
fn migrate_converts_classic_to_modern() {
    let (_dir, bare_dir, container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    // Add a second worktree to test multiple moves
    common::add_classic_worktree(&bare_dir, "feature-x", "feature/x");

    // Verify classic layout before migration
    assert!(bare_dir.exists(), ".bare should exist before migrate");
    assert!(bare_dir.join("trees").join("main").exists(), "trees/main should exist");
    assert!(bare_dir.join("trees").join("feature-x").exists(), "trees/feature-x should exist");

    // Run migrate with --yes (non-interactive)
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Migration complete"));

    // Verify modern layout after migration
    let new_git_dir = container.join(".git");
    assert!(new_git_dir.exists(), ".git should exist after migrate");
    assert!(!bare_dir.exists(), ".bare should NOT exist after migrate");
    assert!(container.join("main").exists(), "main worktree should be at root level");
    assert!(container.join("feature-x").exists(), "feature-x worktree should be at root level");

    // Verify git config was updated
    let layout_output = process::Command::new("git")
        .arg("-C")
        .arg(&new_git_dir)
        .args(["config", "--get", "wt.layout"])
        .output()
        .expect("git config --get wt.layout failed");
    let layout = String::from_utf8_lossy(&layout_output.stdout);
    assert_eq!(layout.trim(), "modern", "wt.layout should be 'modern' after migration");

    // Verify worktrees are functional via git worktree list
    let list_output = process::Command::new("git")
        .arg("-C")
        .arg(&new_git_dir)
        .args(["worktree", "list"])
        .output()
        .expect("git worktree list failed");
    let list = String::from_utf8_lossy(&list_output.stdout);
    assert!(list.contains("main"), "worktree list should include main");
}

#[test]
fn migrate_dry_run_shows_plan_without_changes() {
    let (_dir, bare_dir, _container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--dry-run"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run"));

    // Verify layout is unchanged
    assert!(bare_dir.exists(), ".bare should still exist after dry run");
    assert!(
        bare_dir.join("trees").join("main").exists(),
        "trees/main should still exist after dry run"
    );
}

#[test]
fn migrate_fails_on_modern_layout() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already using modern layout"));
}

#[test]
fn migrate_detects_dirty_worktrees() {
    let (_dir, bare_dir, _container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    // Create an uncommitted file in the main worktree
    std::fs::write(main_wt.join("dirty.txt"), "uncommitted").unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--dry-run"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stderr(predicate::str::contains("uncommitted change"));
}

#[test]
fn migrate_detects_collision() {
    let (_dir, bare_dir, container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    // Create a .git file/directory at the target location to cause collision
    std::fs::create_dir_all(container.join(".git")).unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn migrate_fails_on_worktree_name_collision() {
    let (_dir, bare_dir, container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    // Create a directory at the root level with the same name as an internal worktree
    // "main" worktree will be moved to container/main, so create that collision
    std::fs::create_dir_all(container.join("main")).unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

// ---------------------------------------------------------------------------
// --bare-dir tests (Phase 12)
// ---------------------------------------------------------------------------

#[test]
fn migrate_with_bare_dir_uses_custom_name() {
    let (_dir, bare_dir, container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--bare-dir", ".repo", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Migration complete"));

    // Custom bare dir should exist
    assert!(container.join(".repo").exists(), ".repo should exist after migrate with --bare-dir");
    // Default .git should NOT exist
    assert!(!container.join(".git").exists(), ".git should NOT exist when --bare-dir .repo used");
    // Old .bare should be gone
    assert!(!bare_dir.exists(), ".bare should not exist after migrate");

    // Verify wt.baredir git config
    let output = process::Command::new("git")
        .arg("-C")
        .arg(container.join(".repo"))
        .args(["config", "--get", "wt.baredir"])
        .output()
        .expect("git config --get wt.baredir failed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        ".repo",
        "wt.baredir should be .repo"
    );

    // Verify wt.layout is still modern
    let layout_output = process::Command::new("git")
        .arg("-C")
        .arg(container.join(".repo"))
        .args(["config", "--get", "wt.layout"])
        .output()
        .expect("git config --get wt.layout failed");
    assert_eq!(
        String::from_utf8_lossy(&layout_output.stdout).trim(),
        "modern",
        "wt.layout should be 'modern'"
    );
}

#[test]
fn migrate_with_default_stores_wt_baredir() {
    let (_dir, bare_dir, container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success();

    // Verify wt.baredir is ".git" even for default
    let output = process::Command::new("git")
        .arg("-C")
        .arg(container.join(".git"))
        .args(["config", "--get", "wt.baredir"])
        .output()
        .expect("git config --get wt.baredir failed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        ".git",
        "wt.baredir should be .git for default migration"
    );
}

#[test]
fn migrate_with_bare_dir_shows_info_line() {
    let (_dir, bare_dir, _container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--bare-dir", ".repo", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Bare repo dir: .repo"));
}

#[test]
fn migrate_default_no_bare_repo_info_line() {
    let (_dir, bare_dir, _container) = common::create_classic_bare_repo();
    let main_wt = bare_dir.join("trees").join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["migrate", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Bare repo dir:").not());
}
