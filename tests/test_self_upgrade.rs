mod common;
use assert_cmd::Command;
use predicates::prelude::*;

/// wt update should succeed even if crates.io and GitHub both return 404
/// (crate not published yet). Per UPD-05, network errors are silent.
#[test]
fn update_command_exits_successfully() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["update"])
        .assert()
        .success();
}

/// Update does not require being inside a git repo (UPD-01).
#[test]
fn update_works_outside_git_repo() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["update"])
        .current_dir(dir.path())
        .assert()
        .success();
}

/// Update should show some recognizable message -- either "Checking for updates",
/// "latest version", "Could not check", or "available".
#[test]
fn update_shows_checking_message() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["update"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Checking for updates")
                .or(predicate::str::contains("latest version"))
                .or(predicate::str::contains("Could not check"))
                .or(predicate::str::contains("available")),
        );
}

/// Regression check: version command still works after update wiring.
#[test]
fn version_command_still_works_after_update_wiring() {
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["version"])
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

/// update output covers ALL three run() branches: update available, up to date, or fetch failed.
/// Since we cannot control the network, assert that the output matches one of the three.
#[test]
fn update_output_contains_version_info() {
    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["update"])
        .output()
        .expect("failed to run lazywt update");

    assert!(output.status.success(), "update command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // All three branches produce recognizable output:
    // 1. "New version X available" (update available)
    // 2. "latest version" (up to date -- contains current version)
    // 3. "Could not check" (network failure)
    let has_update_available = stdout.contains("available");
    let has_up_to_date = stdout.contains("latest version");
    let has_fetch_failed = stdout.contains("Could not check");
    assert!(
        has_update_available || has_up_to_date || has_fetch_failed,
        "update output must match one of the three run() branches. Got: {}",
        stdout
    );
}

/// update writes a cache file to the config directory after checking.
/// The cache is at dirs::config_dir()/lazywt/update_check.
#[test]
fn update_writes_cache_file() {
    // Run update -- it will write the cache regardless of network outcome
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["update"])
        .assert()
        .success();

    // Check that the cache file was written
    if let Some(config_dir) = dirs::config_dir() {
        let cache_path = config_dir.join("lazywt").join("update_check");
        assert!(
            cache_path.exists(),
            "Cache file should exist at {:?} after update",
            cache_path
        );
        let content = std::fs::read_to_string(&cache_path).unwrap();
        // Cache should contain at least a timestamp (Unix epoch number)
        let first_line = content.lines().next().unwrap_or("");
        let timestamp: Result<u64, _> = first_line.trim().parse();
        assert!(
            timestamp.is_ok(),
            "Cache first line should be a valid timestamp, got: {}",
            first_line
        );
    }
}

/// update first line of output is always "Checking for updates..." regardless of outcome.
#[test]
fn update_checking_message_on_stdout() {
    let output = Command::cargo_bin("lazywt")
        .unwrap()
        .args(["update"])
        .output()
        .expect("failed to run lazywt update");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Checking for updates"),
        "First output should contain 'Checking for updates', got: {}",
        stdout
    );
}
