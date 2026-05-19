mod common;
use assert_cmd::Command;
use predicates::prelude::*;
use serde_json;

// ---------------------------------------------------------------------------
// Existing tests
// ---------------------------------------------------------------------------

#[test]
fn list_shows_worktrees_in_bare_repo() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("list")
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("Git Worktrees for"))
        .stdout(predicate::str::contains("[bare repository]"))
        .stdout(predicate::str::contains("Worktrees:"))
        .stdout(predicate::str::contains("main"));
}

#[test]
fn list_shows_current_worktree_with_star() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("list")
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("\u{2605}"))
        .stdout(predicate::str::contains("[current]"));
}

#[test]
fn list_shows_multiple_worktrees() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "feature-auth", "feature/auth");

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("list")
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("main"))
        .stdout(predicate::str::contains("feature-auth"));
}

#[test]
fn list_short_flag_shows_minimal_output() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--short"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("main"))
        .stdout(predicate::str::contains("Worktrees:"));
}

#[test]
fn list_json_flag_produces_valid_json() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed.is_array());
    let arr = parsed.as_array().unwrap();
    assert!(!arr.is_empty());
    assert_eq!(arr[0]["name"], "main");
    assert_eq!(arr[0]["is_current"], true);
}

#[test]
fn list_no_path_flag_hides_paths() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--no-path"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let has_path_lines = stdout
        .lines()
        .any(|l| l.starts_with("       ") && !l.trim().is_empty());
    assert!(
        !has_path_lines,
        "No path lines should be present with --no-path, got:\n{}",
        stdout
    );
}

#[test]
fn list_shows_dirty_status_for_untracked_files() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    std::fs::write(main_wt.join("dirty.txt"), "dirty content").unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("list")
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("?"));
}

#[test]
fn list_json_includes_dirty_status() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    std::fs::write(main_wt.join("dirty.txt"), "dirty content").unwrap();

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--json"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr[0]["dirty"]["untracked"], true);
}

#[test]
fn list_succeeds_in_nonbare_repo() {
    let dir = tempfile::tempdir().unwrap();
    // Init a normal (non-bare) repo — now supported via NonBare layout
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Non-bare repos are now supported — list should succeed
    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("list")
        .current_dir(dir.path())
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// New display-path tests (Phase 7 Plan 3)
// ---------------------------------------------------------------------------

#[test]
fn list_shows_branch_arrow_when_name_differs() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    // Create worktree where directory name ("auth") differs from branch ("feature/auth")
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--no-color"])
        .current_dir(root.join("main"))
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // display.rs prints "name -> branch" when display_name != branch
    assert!(
        stdout.contains("auth -> feature/auth"),
        "Should contain 'auth -> feature/auth' arrow format, got:\n{}",
        stdout
    );
}

#[test]
fn list_short_shows_branch_arrow_when_name_differs() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    common::add_worktree(&git_dir, &root, "auth", "feature/auth");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--short", "--no-color"])
        .current_dir(root.join("main"))
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Short mode also shows the arrow format
    assert!(
        stdout.contains("auth -> feature/auth"),
        "Short mode should contain 'auth -> feature/auth', got:\n{}",
        stdout
    );
}

#[test]
fn list_shows_staged_and_modified_status() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Create a file, stage it, then modify it again
    std::fs::write(main_wt.join("staged.txt"), "initial content").unwrap();
    std::process::Command::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["add", "staged.txt"])
        .output()
        .expect("git add failed");

    // Modify the staged file again (creates both staged + modified state)
    std::fs::write(main_wt.join("staged.txt"), "modified after staging").unwrap();

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--no-color"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show "+" (staged) and "!" (modified) indicators
    assert!(
        stdout.contains("+"),
        "Should contain '+' for staged changes, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("!"),
        "Should contain '!' for modified changes, got:\n{}",
        stdout
    );
}

#[test]
fn list_no_color_flag_works() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--no-color"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain the expected text
    assert!(
        stdout.contains("Git Worktrees for"),
        "Should contain header, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("main"),
        "Should contain worktree name, got:\n{}",
        stdout
    );

    // Should NOT contain ANSI escape sequences when --no-color is used
    assert!(
        !stdout.contains("\x1b["),
        "Should not contain ANSI escape sequences with --no-color, got:\n{}",
        stdout
    );
}

#[test]
fn list_shows_current_worktree_in_short_mode() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    common::add_worktree(&git_dir, &root, "feature-x", "feature/x");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--short", "--no-color"])
        .current_dir(root.join("main"))
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The current worktree line should have the star character and [current] tag
    assert!(
        stdout.contains("\u{2605}"),
        "Should contain star character for current worktree, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("[current]"),
        "Should contain [current] tag, got:\n{}",
        stdout
    );

    // Verify the current tag is on the main line (we ran from main)
    let main_line = stdout.lines().find(|l| l.contains("main"));
    assert!(
        main_line.is_some(),
        "Should have a line containing 'main', got:\n{}",
        stdout
    );
    let main_line = main_line.unwrap();
    assert!(
        main_line.contains("[current]"),
        "The main line should have [current], got: {}",
        main_line
    );
}

/// list with --no-path and --no-color together should work.
#[test]
fn list_no_path_no_color_combined() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--no-path", "--no-color"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain header and worktree name
    assert!(stdout.contains("Git Worktrees for"));
    assert!(stdout.contains("main"));
    // Should NOT contain ANSI codes
    assert!(!stdout.contains("\x1b["));
}

/// list with --short and --no-path combined.
#[test]
fn list_short_no_path_combined() {
    let (_dir, _git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--short", "--no-path"])
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("main"));
}

/// list with multiple worktrees in --no-color mode shows all names without ANSI.
#[test]
fn list_multiple_worktrees_no_color() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");
    common::add_worktree(&git_dir, &root, "dev", "feature/dev");
    common::add_worktree(&git_dir, &root, "staging", "feature/staging");

    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["list", "--no-color"])
        .current_dir(&main_wt)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("main"));
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("staging"));
    assert!(!stdout.contains("\x1b["));
}
