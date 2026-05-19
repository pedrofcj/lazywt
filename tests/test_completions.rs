mod common;

use assert_cmd::Command;
use predicates::prelude::*;

/// completions bash should output valid bash completion script.
#[test]
fn completions_bash_succeeds() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

/// completions zsh should output valid zsh completion script.
#[test]
fn completions_zsh_succeeds() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

/// completions powershell should output valid PowerShell completion script.
#[test]
fn completions_powershell_succeeds() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["completions", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

/// completions fish should output valid fish completion script.
#[test]
fn completions_fish_succeeds() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

/// completions with unsupported shell should fail with error.
#[test]
fn completions_unsupported_shell_fails() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["completions", "unknown_shell"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unsupported shell"));
}

/// completions does not require being inside a git repo.
#[test]
fn completions_works_outside_git_repo() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["completions", "bash"])
        .current_dir(dir.path())
        .assert()
        .success();
}
