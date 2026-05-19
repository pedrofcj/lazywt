mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Command as StdCommand;

#[test]
fn prune_detects_merged_branches() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a feature worktree with a branch
    let feature_wt = common::add_worktree(&git_dir, &root, "feature-auth", "feature-auth");

    // Configure git user in feature worktree
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Make a commit in the feature worktree (so it has >=1 unique commit)
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["commit", "--allow-empty", "-m", "feature work"])
        .output()
        .expect("feature commit failed");

    // Also make a commit on main so that merge isn't a fast-forward
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "main diverge"])
        .output()
        .unwrap();

    // Merge the feature branch into main (non-ff ensures a real merge commit)
    let merge_out = StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["merge", "feature-auth", "--no-ff", "--no-edit"])
        .output()
        .expect("merge failed");
    assert!(
        merge_out.status.success(),
        "merge should succeed: {}",
        String::from_utf8_lossy(&merge_out.stderr)
    );

    // Run prune --yes
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("feature-auth")
                .and(predicate::str::contains("merged"))
                .and(predicate::str::contains("Removed"))
                .and(predicate::str::contains("Pruned")),
        );

    // Verify the worktree directory was removed
    assert!(
        !feature_wt.exists(),
        "feature-auth worktree should be removed"
    );
}

#[test]
fn prune_detects_orphan_worktrees() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a feature worktree
    let feature_wt = common::add_worktree(&git_dir, &root, "orphan-wt", "orphan-branch");

    // Manually delete the worktree directory to make it an orphan
    std::fs::remove_dir_all(&feature_wt).expect("failed to remove worktree dir");

    // Run prune --yes
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("orphan")
                .and(predicate::str::contains("Removed")),
        );
}

#[test]
fn prune_skips_dirty_merged_worktrees() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a feature worktree
    let feature_wt = common::add_worktree(&git_dir, &root, "dirty-feat", "dirty-feat");

    // Configure git user
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Make a commit (>=1 unique commit for prune detection)
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["commit", "--allow-empty", "-m", "dirty feature work"])
        .output()
        .unwrap();

    // Diverge main so merge is not fast-forward
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "main diverge for dirty"])
        .output()
        .unwrap();

    // Merge into main with --no-ff
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["merge", "dirty-feat", "--no-ff", "--no-edit"])
        .output()
        .unwrap();

    // Make the worktree dirty (create an untracked file)
    std::fs::write(feature_wt.join("dirty.txt"), "uncommitted changes").unwrap();

    // Run prune --yes (without --force) -- should skip dirty worktree
    // "Skipped dirty worktrees" is output::warning -> stderr
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stderr(predicate::str::contains("Skipped dirty worktrees"))
        .stderr(predicate::str::contains("dirty-feat"));

    // Verify the worktree still exists
    assert!(
        feature_wt.exists(),
        "dirty-feat worktree should NOT be removed without --force"
    );
}

#[test]
fn prune_force_removes_dirty_merged_worktrees() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a feature worktree
    let feature_wt = common::add_worktree(&git_dir, &root, "dirty-force", "dirty-force");

    // Configure git user
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Make a commit
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["commit", "--allow-empty", "-m", "force dirty feature"])
        .output()
        .unwrap();

    // Diverge main so merge is not fast-forward
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "main diverge for force"])
        .output()
        .unwrap();

    // Merge into main with --no-ff
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["merge", "dirty-force", "--no-ff", "--no-edit"])
        .output()
        .unwrap();

    // Make the worktree dirty
    std::fs::write(feature_wt.join("dirty.txt"), "uncommitted changes").unwrap();

    // Run prune --yes --force -- should remove even dirty worktrees
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes", "--force"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Removed")
                .and(predicate::str::contains("dirty-force")),
        );

    // Verify the worktree was removed
    assert!(
        !feature_wt.exists(),
        "dirty-force worktree should be removed with --force"
    );
}

#[test]
fn prune_nothing_to_prune() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Run prune --yes on a repo with only the main worktree
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing to prune"));
}

#[test]
fn prune_skips_default_branch_worktree() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a feature worktree (with a commit + merge, so there IS something to prune)
    let feature_wt = common::add_worktree(&git_dir, &root, "side-branch", "side-branch");
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["commit", "--allow-empty", "-m", "side work"])
        .output()
        .unwrap();
    // Diverge main for non-ff merge
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "main diverge for side"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["merge", "side-branch", "--no-ff", "--no-edit"])
        .output()
        .unwrap();

    // Run prune --yes
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success();

    // Verify main worktree still exists (never pruned)
    assert!(
        main_wt.exists(),
        "main worktree must never be removed by prune"
    );
}

#[test]
fn prune_skips_fresh_branches() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a worktree at the same commit as main (zero unique commits)
    // This will show up in `git branch --merged` but should NOT be pruned
    // because rev-list --count shows 0 unique commits.
    let _fresh_wt = common::add_worktree(&git_dir, &root, "fresh-branch", "fresh-branch");

    // Run prune --yes
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing to prune"));
}

#[test]
fn prune_deletes_branch_after_removal() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a feature worktree
    let feature_wt = common::add_worktree(&git_dir, &root, "del-branch", "del-branch");

    // Configure git user
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .unwrap();

    // Make a commit (>=1 unique commit)
    StdCommand::new("git")
        .arg("-C")
        .arg(&feature_wt)
        .args(["commit", "--allow-empty", "-m", "branch to delete"])
        .output()
        .unwrap();

    // Diverge main so merge is not fast-forward
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "main diverge for del"])
        .output()
        .unwrap();

    // Merge into main with --no-ff
    StdCommand::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["merge", "del-branch", "--no-ff", "--no-edit"])
        .output()
        .unwrap();

    // Run prune --yes
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["prune", "--yes"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));

    // Verify the branch itself was deleted
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["branch", "--list", "del-branch"])
        .output()
        .expect("git branch --list failed");
    let branch_list = String::from_utf8_lossy(&output.stdout);
    assert!(
        branch_list.trim().is_empty(),
        "Branch 'del-branch' should be deleted after prune, got: {}",
        branch_list
    );
}
