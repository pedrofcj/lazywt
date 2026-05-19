mod common;
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn fix_fetch_sets_correct_refspec() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("fix-fetch")
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("refspec"));

    // Verify the refspec was set correctly
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "--get", "remote.origin.fetch"])
        .output()
        .unwrap();
    let refspec = String::from_utf8(output.stdout).unwrap();
    assert_eq!(refspec.trim(), "+refs/heads/*:refs/remotes/origin/*");
}

#[test]
fn fix_fetch_already_correct() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Set correct refspec first
    std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args([
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ])
        .output()
        .unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("fix-fetch")
        .current_dir(&main_wt)
        .assert()
        .success()
        .stdout(predicate::str::contains("already correctly configured"));
}

#[test]
fn fix_fetch_corrects_wrong_refspec() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Set a single-branch refspec (common misconfiguration)
    std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args([
            "config",
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/main",
        ])
        .output()
        .unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("fix-fetch")
        .current_dir(&main_wt)
        .assert()
        .success()
        // The warning path prints "not optimal"
        .stderr(predicate::str::contains("not optimal"));

    // Verify the refspec was corrected
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "--get", "remote.origin.fetch"])
        .output()
        .unwrap();
    let refspec = String::from_utf8(output.stdout).unwrap();
    assert_eq!(refspec.trim(), "+refs/heads/*:refs/remotes/origin/*");
}

#[test]
fn fix_fetch_handles_no_refspec() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Remove the refspec entirely
    let _ = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "--unset", "remote.origin.fetch"])
        .output()
        .unwrap();

    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("fix-fetch")
        .current_dir(&main_wt)
        .assert()
        .success()
        // The None path prints "not configured"
        .stderr(predicate::str::contains("not configured"));

    // Verify the refspec was set
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "--get", "remote.origin.fetch"])
        .output()
        .unwrap();
    let refspec = String::from_utf8(output.stdout).unwrap();
    assert_eq!(refspec.trim(), "+refs/heads/*:refs/remotes/origin/*");
}

#[test]
fn fix_fetch_handles_fetch_failure_gracefully() {
    let (_dir, git_dir, root) = common::create_modern_bare_repo();
    let main_wt = root.join("main");

    // Set remote URL to an unreachable location so fetch will fail
    std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args([
            "config",
            "remote.origin.url",
            "file:///nonexistent/repo.git",
        ])
        .output()
        .unwrap();

    // Remove refspec to trigger the fix path
    let _ = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "--unset", "remote.origin.fetch"])
        .output()
        .unwrap();

    // Command should succeed even when fetch fails (graceful handling)
    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("fix-fetch")
        .current_dir(&main_wt)
        .assert()
        .success()
        // The refspec fix message should still appear
        .stdout(predicate::str::contains("Fetch refspec fixed"));

    // Verify the refspec was still set despite fetch failure
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "--get", "remote.origin.fetch"])
        .output()
        .unwrap();
    let refspec = String::from_utf8(output.stdout).unwrap();
    assert_eq!(refspec.trim(), "+refs/heads/*:refs/remotes/origin/*");
}
