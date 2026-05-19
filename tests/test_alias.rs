mod common;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn alias_creates_powershell_function_in_profile() {
    let temp_home = TempDir::new().unwrap();
    let profile_path = temp_home.path().join("profile.ps1");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["alias"])
        .env("PROFILE", profile_path.to_str().unwrap())
        .env("PSModulePath", "C:\\fake")
        .env_remove("NU_VERSION")
        .env_remove("SHELL")
        .assert()
        .success()
        .stdout(predicate::str::contains("Added alias"));

    // Verify the profile file was created with a PowerShell function
    let content = std::fs::read_to_string(&profile_path).unwrap();
    assert!(content.contains("lazywt"), "profile should contain lazywt reference");
    assert!(content.contains(".lazywt_cd"), "profile should contain nav file check");
    assert!(content.contains("function "), "should contain a function definition");
    assert!(content.contains("Set-Location"), "should contain Set-Location for cd");
}

#[test]
fn alias_detects_duplicate_in_profile() {
    let temp_home = TempDir::new().unwrap();
    let profile_path = temp_home.path().join("profile.ps1");

    // First invocation: writes the function
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["alias"])
        .env("PROFILE", profile_path.to_str().unwrap())
        .env("PSModulePath", "C:\\fake")
        .env_remove("NU_VERSION")
        .env_remove("SHELL")
        .assert()
        .success()
        .stdout(predicate::str::contains("Added alias"));

    // Second invocation: should detect duplicate
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["alias"])
        .env("PROFILE", profile_path.to_str().unwrap())
        .env("PSModulePath", "C:\\fake")
        .env_remove("NU_VERSION")
        .env_remove("SHELL")
        .assert()
        .success()
        .stdout(predicate::str::contains("already configured"));
}

#[test]
fn alias_creates_parent_dirs_if_needed() {
    let temp_home = TempDir::new().unwrap();

    // Set PROFILE to a path with non-existent parent directories
    let deep_profile = temp_home.path().join("deep").join("nested").join("profile.ps1");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["alias"])
        .env("PROFILE", deep_profile.to_str().unwrap())
        .env("PSModulePath", "C:\\fake")
        .env_remove("NU_VERSION")
        .env_remove("SHELL")
        .assert()
        .success()
        .stdout(predicate::str::contains("Added alias"));

    assert!(deep_profile.exists(), "profile file should be created even in deep directory");
}

#[test]
fn alias_powershell_includes_lazywt_prev() {
    let temp_home = TempDir::new().unwrap();
    let profile_path = temp_home.path().join("profile.ps1");

    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["alias"])
        .env("PROFILE", profile_path.to_str().unwrap())
        .env("PSModulePath", "C:\\fake")
        .env_remove("NU_VERSION")
        .env_remove("SHELL")
        .assert()
        .success();

    let content = std::fs::read_to_string(&profile_path).unwrap();
    assert!(
        content.contains("LAZYWT_PREV"),
        "PowerShell function should set LAZYWT_PREV for switch - support"
    );
}
