use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Create a regular (non-bare) git repo for setup tests.
/// Returns (TempDir, PathBuf to repo root).
fn create_regular_repo() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("my-project");
    std::fs::create_dir_all(&repo).unwrap();

    StdCommand::new("git")
        .args(["init"])
        .current_dir(&repo)
        .output()
        .expect("git init failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("git config failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["config", "user.name", "Test User"])
        .output()
        .expect("git config failed");

    // Create initial commit on main branch
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["checkout", "-b", "main"])
        .output()
        .expect("git checkout failed");

    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["commit", "--allow-empty", "-m", "initial commit"])
        .output()
        .expect("git commit failed");

    (dir, repo)
}

#[test]
fn setup_dry_run_shows_plan_without_changes() {
    let (_dir, repo) = create_regular_repo();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--dry-run"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("Bare repo will be at"))
        .stdout(predicate::str::contains("Backup will be at"))
        .stdout(predicate::str::contains("[DRY RUN]"));

    // Verify no changes were made
    assert!(
        repo.join(".git").is_dir(),
        ".git directory should still exist after dry run"
    );
}

#[test]
fn setup_converts_regular_repo_to_bare() {
    let (_dir, repo) = create_regular_repo();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--yes"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("Repository converted to bare"));

    // Verify .git is now a bare repo directory
    let git_dir = repo.join(".git");
    assert!(git_dir.is_dir(), ".git directory should exist");

    // Verify it's actually bare
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["rev-parse", "--is-bare-repository"])
        .output()
        .expect("git rev-parse failed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "true",
        ".git should be a bare repository"
    );

    // Verify main worktree was created
    assert!(
        repo.join("main").exists(),
        "main worktree should be created"
    );
}

#[test]
fn setup_fails_on_bare_repo() {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("bare.git");

    StdCommand::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .output()
        .expect("git init --bare failed");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--yes"])
        .current_dir(&bare)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already bare"));
}

#[test]
fn setup_fails_with_uncommitted_changes() {
    let (_dir, repo) = create_regular_repo();

    // Create an uncommitted file
    std::fs::write(repo.join("dirty.txt"), "uncommitted").unwrap();
    StdCommand::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["add", "dirty.txt"])
        .output()
        .expect("git add failed");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--yes"])
        .current_dir(&repo)
        .assert()
        .failure()
        .stderr(predicate::str::contains("uncommitted changes"));
}

#[test]
fn setup_fails_when_backup_already_exists() {
    let (_dir, repo) = create_regular_repo();

    // Create the backup directory that setup would try to use
    let parent = repo.parent().unwrap();
    let repo_name = repo.file_name().unwrap().to_str().unwrap();
    let backup_dir = parent.join(format!("{}.setup-backup", repo_name));
    std::fs::create_dir_all(&backup_dir).unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--yes"])
        .current_dir(&repo)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Backup directory already exists"));
}

// ---------------------------------------------------------------------------
// --bare-dir tests (Phase 12)
// ---------------------------------------------------------------------------

#[test]
fn setup_with_bare_dir_uses_custom_name() {
    let (_dir, repo) = create_regular_repo();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--bare-dir", ".repo", "--yes"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("Repository converted to bare"));

    // Custom bare dir should exist
    assert!(repo.join(".repo").is_dir(), ".repo directory should exist");
    // Default .git should NOT exist (original was moved to backup)
    assert!(!repo.join(".git").is_dir(), ".git should NOT exist when --bare-dir .repo used");

    // Verify it's actually bare
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(repo.join(".repo"))
        .args(["rev-parse", "--is-bare-repository"])
        .output()
        .expect("git rev-parse failed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "true",
        ".repo should be a bare repository"
    );

    // Verify wt.baredir git config
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(repo.join(".repo"))
        .args(["config", "--get", "wt.baredir"])
        .output()
        .expect("git config --get wt.baredir failed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        ".repo",
        "wt.baredir should be .repo"
    );

    // Verify wt.layout is modern
    let layout_output = StdCommand::new("git")
        .arg("-C")
        .arg(repo.join(".repo"))
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
fn setup_default_stores_wt_baredir() {
    let (_dir, repo) = create_regular_repo();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--yes"])
        .current_dir(&repo)
        .assert()
        .success();

    // Verify wt.baredir is ".git" even for default
    let output = StdCommand::new("git")
        .arg("-C")
        .arg(repo.join(".git"))
        .args(["config", "--get", "wt.baredir"])
        .output()
        .expect("git config --get wt.baredir failed");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        ".git",
        "wt.baredir should be .git for default setup"
    );
}

#[test]
fn setup_with_bare_dir_shows_info_line() {
    let (_dir, repo) = create_regular_repo();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--bare-dir", ".repo", "--dry-run"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("Bare repo dir: .repo"));
}

#[test]
fn setup_default_no_bare_repo_info_line() {
    let (_dir, repo) = create_regular_repo();

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["setup", "--dry-run"])
        .current_dir(&repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("Bare repo dir:").not());
}
