mod common;
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_prints_version_string() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains("lazywt 1.0.0"));
}

#[test]
fn version_works_outside_git_repo() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("lazywt")
        .unwrap()
        .arg("version")
        .current_dir(dir.path())
        .assert()
        .success();
}
