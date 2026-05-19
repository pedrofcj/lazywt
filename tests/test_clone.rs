mod common;

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Create a local bare repository that acts as a "remote" for clone tests.
/// Returns the TempDir (keep alive) and the path to the bare repo.
fn create_local_remote() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let remote_path = dir.path().join("remote.git");

    // Initialize bare repo
    Command::new("git")
        .args(["init", "--bare"])
        .arg(&remote_path)
        .output()
        .expect("git init --bare failed");

    // Create a temporary worktree to make an initial commit
    let tmp_wt = dir.path().join("tmp-wt");
    Command::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args([
            "worktree",
            "add",
            tmp_wt.to_str().unwrap(),
            "-b",
            "main",
        ])
        .output()
        .expect("git worktree add failed");

    // Configure git user
    Command::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("git config user.email failed");

    Command::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .expect("git config user.name failed");

    // Create initial commit
    Command::new("git")
        .arg("-C")
        .arg(&tmp_wt)
        .args(["commit", "--allow-empty", "-m", "initial commit"])
        .output()
        .expect("git commit failed");

    // Point HEAD to main
    Command::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .output()
        .expect("git symbolic-ref HEAD failed");

    // Remove temporary worktree
    Command::new("git")
        .arg("-C")
        .arg(&remote_path)
        .args(["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"])
        .output()
        .expect("git worktree remove failed");

    (dir, remote_path)
}

/// Run lazywt clone in a specific working directory.
fn run_clone(cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lazywt"))
        .arg("clone")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run lazywt clone")
}

#[test]
fn clone_creates_modern_layout() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dest = work_dir.path().join("test-project");
    let git_dir = dest.join(".git");

    // Verify .git directory exists
    assert!(git_dir.exists(), ".git directory should exist");
    assert!(git_dir.is_dir(), ".git should be a directory");

    // Verify wt.layout is set to modern
    let layout_output = Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "wt.layout"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&layout_output.stdout).trim(),
        "modern"
    );

    // Verify core.bare is true
    let bare_output = Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "core.bare"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&bare_output.stdout).trim(),
        "true"
    );
}

#[test]
fn clone_creates_default_branch_worktree() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dest = work_dir.path().join("test-project");
    let main_wt = dest.join("main");

    // Verify main worktree directory exists
    assert!(main_wt.exists(), "main worktree directory should exist");

    // Verify it is a worktree (has a .git file, not directory)
    let dot_git = main_wt.join(".git");
    assert!(dot_git.exists(), ".git marker file should exist in worktree");
    assert!(
        dot_git.is_file(),
        ".git in worktree should be a file (worktree marker), not a directory"
    );
}

#[test]
fn clone_with_custom_dirname() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "custom-name"],
    );

    assert!(
        output.status.success(),
        "clone should succeed with custom dirname. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dest = work_dir.path().join("custom-name");
    assert!(dest.exists(), "custom-name directory should exist");
    assert!(dest.join(".git").exists(), ".git should exist under custom-name");
}

#[test]
fn clone_fails_if_directory_exists() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    // Pre-create the target directory
    let existing = work_dir.path().join("existing-dir");
    std::fs::create_dir_all(&existing).unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "existing-dir"],
    );

    assert!(
        !output.status.success(),
        "clone should fail when directory already exists"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already exists"),
        "error should mention 'already exists', got: {}",
        stderr
    );
}

#[test]
fn clone_sets_correct_fetch_refspec() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let git_dir = work_dir.path().join("test-project").join(".git");

    // Verify fetch refspec
    let fetch_output = Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "remote.origin.fetch"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&fetch_output.stdout).trim(),
        "+refs/heads/*:refs/remotes/origin/*"
    );
}

#[test]
fn clone_auto_extracts_repo_name() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    // Convert the remote path to use forward slashes so extract_repo_name
    // can correctly split on '/' (it doesn't handle Windows backslashes)
    let remote_url = remote_path.to_str().unwrap().replace('\\', "/");

    // Clone WITHOUT specifying dirname -- auto-extract name from URL
    let output = run_clone(work_dir.path(), &[&remote_url]);

    assert!(
        output.status.success(),
        "clone should succeed without dirname. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The auto-extracted name from ".../remote.git" should be "remote"
    let dest = work_dir.path().join("remote");
    assert!(
        dest.exists(),
        "auto-extracted directory 'remote' should exist at {}",
        dest.display()
    );
    assert!(
        dest.join(".git").exists(),
        ".git should exist under auto-extracted name"
    );
}

#[test]
fn clone_succeeds_and_creates_worktree_from_local_remote() {
    // This test verifies the full clone_inner flow including steps 6-17
    // It exercises clone output lines for progress messages
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "full-flow-test"],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "clone should succeed. stderr: {}", String::from_utf8_lossy(&output.stderr));

    // Verify progress messages were printed (exercises output lines)
    assert!(stdout.contains("Cloning"), "should show cloning progress");
    assert!(stdout.contains("Repository ready"), "should show success");
    assert!(stdout.contains("Main worktree"), "should show worktree info");
}

#[test]
fn clone_with_invalid_url_cleans_up() {
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &["https://invalid.example.com/nonexistent.git", "test-fail"],
    );

    assert!(
        !output.status.success(),
        "clone should fail with invalid URL"
    );

    // Verify cleanup -- directory should NOT exist
    let dest = work_dir.path().join("test-fail");
    assert!(
        !dest.exists(),
        "test-fail directory should be cleaned up after failed clone"
    );
}

// === --bare-dir flag tests ===

#[test]
fn clone_with_bare_dir_creates_custom_directory() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project", "--bare-dir", ".repo"],
    );

    assert!(
        output.status.success(),
        "clone with --bare-dir should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dest = work_dir.path().join("test-project");

    // .repo directory should exist (the custom bare dir)
    assert!(dest.join(".repo").exists(), ".repo directory should exist");
    assert!(dest.join(".repo").is_dir(), ".repo should be a directory");

    // .git directory should NOT exist
    assert!(!dest.join(".git").exists(), ".git directory should NOT exist when --bare-dir is used");

    // Verify wt.layout is set to modern
    let layout_output = Command::new("git")
        .arg("-C")
        .arg(dest.join(".repo"))
        .args(["config", "wt.layout"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&layout_output.stdout).trim(),
        "modern"
    );

    // Verify core.bare is true
    let bare_output = Command::new("git")
        .arg("-C")
        .arg(dest.join(".repo"))
        .args(["config", "core.bare"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&bare_output.stdout).trim(),
        "true"
    );
}

#[test]
fn clone_with_bare_dir_stores_wt_baredir() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project", "--bare-dir", ".repo"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bare_dir = work_dir.path().join("test-project").join(".repo");

    // Read wt.baredir from git config
    let baredir_output = Command::new("git")
        .arg("-C")
        .arg(&bare_dir)
        .args(["config", "wt.baredir"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&baredir_output.stdout).trim(),
        ".repo",
        "wt.baredir should be .repo"
    );
}

#[test]
fn clone_default_stores_wt_baredir() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let git_dir = work_dir.path().join("test-project").join(".git");

    // Read wt.baredir from git config -- should be ".git" even for default (D-04)
    let baredir_output = Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "wt.baredir"])
        .output()
        .expect("git config failed");
    assert_eq!(
        String::from_utf8_lossy(&baredir_output.stdout).trim(),
        ".git",
        "wt.baredir should be .git for default clone (D-04: always write)"
    );
}

#[test]
fn clone_with_bare_dir_shows_info_line() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project", "--bare-dir", ".repo"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Bare repo: .repo"),
        "should show 'Bare repo: .repo' info line. stdout: {}",
        stdout
    );
}

#[test]
fn clone_default_no_bare_repo_info_line() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Bare repo:"),
        "should NOT show 'Bare repo:' line for default clone. stdout: {}",
        stdout
    );
}

#[test]
fn clone_with_bare_dir_creates_worktree() {
    let (_remote_dir, remote_path) = create_local_remote();
    let work_dir = tempfile::tempdir().unwrap();

    let output = run_clone(
        work_dir.path(),
        &[remote_path.to_str().unwrap(), "test-project", "--bare-dir", ".repo"],
    );

    assert!(
        output.status.success(),
        "clone should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let dest = work_dir.path().join("test-project");
    let main_wt = dest.join("main");

    // Verify main worktree directory exists
    assert!(main_wt.exists(), "main worktree directory should exist");

    // Verify it is a worktree (has a .git file, not directory)
    let dot_git = main_wt.join(".git");
    assert!(dot_git.exists(), ".git marker file should exist in worktree");
    assert!(
        dot_git.is_file(),
        ".git in worktree should be a file (worktree marker), not a directory"
    );
}
