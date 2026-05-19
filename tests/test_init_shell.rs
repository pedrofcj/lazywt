mod common;
use assert_cmd::Command;
use predicates::prelude::*;

/// Build a `lazywt` command isolated from the developer's real config.
///
/// Uses WT_RENAME to force a known command_name ("wt"), because on Windows
/// dirs::config_dir() uses the Windows API (not APPDATA env var) and may
/// pick up the developer's real global config with a custom command_name.
///
/// Also clears PSModulePath and NU_VERSION to avoid shell auto-detection
/// side effects.
fn isolated_cmd() -> Command {
    let mut cmd = Command::cargo_bin("lazywt").unwrap();
    cmd.env("WT_RENAME", "wt")
        .env_remove("PSModulePath")
        .env_remove("NU_VERSION");
    cmd
}

// ---------------------------------------------------------------------------
// Shell output tests -- verify stdout contains correct function definitions
// ---------------------------------------------------------------------------

#[test]
fn init_bash_outputs_shell_function() {
    let dir = tempfile::tempdir().unwrap();
    isolated_cmd()
        .args(["init", "bash"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("wt()"))
        .stdout(predicate::str::contains("command lazywt"))
        .stdout(predicate::str::contains(".lazywt_cd"))
        .stdout(predicate::str::contains("LAZYWT_PREV"));
}

#[test]
fn init_zsh_outputs_shell_function() {
    let dir = tempfile::tempdir().unwrap();
    isolated_cmd()
        .args(["init", "zsh"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("wt()"))
        .stdout(predicate::str::contains("command lazywt"))
        .stdout(predicate::str::contains(".lazywt_cd"))
        .stdout(predicate::str::contains("LAZYWT_PREV"));
}

#[test]
fn init_powershell_outputs_shell_function() {
    let dir = tempfile::tempdir().unwrap();
    isolated_cmd()
        .args(["init", "powershell"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("function wt"))
        .stdout(predicate::str::contains("& lazywt"))
        .stdout(predicate::str::contains(".lazywt_cd"))
        .stdout(predicate::str::contains("LAZYWT_PREV"));
}

#[test]
fn init_nushell_outputs_shell_function() {
    let dir = tempfile::tempdir().unwrap();
    isolated_cmd()
        .args(["init", "nushell"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("def --env wt"))
        .stdout(predicate::str::contains("lazywt ...$args"))
        .stdout(predicate::str::contains(".lazywt_cd"))
        .stdout(predicate::str::contains("LAZYWT_PREV"));
}

#[test]
fn init_unknown_shell_fails() {
    let dir = tempfile::tempdir().unwrap();
    isolated_cmd()
        .args(["init", "fish"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Unknown shell")
                .or(predicate::str::contains("Supported")),
        );
}

#[test]
fn init_bash_respects_wt_rename() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["init", "bash"])
        .env("WT_RENAME", "mycmd")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("mycmd()"))
        .stdout(predicate::str::contains(".lazywt_cd"));
}

#[test]
fn init_nushell_respects_wt_rename() {
    let dir = tempfile::tempdir().unwrap();
    Command::cargo_bin("lazywt")
        .unwrap()
        .args(["init", "nushell"])
        .env("WT_RENAME", "tree")
        .current_dir(dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("def --env tree"))
        .stdout(predicate::str::contains(".lazywt_cd"));
}

// ---------------------------------------------------------------------------
// --add tests -- verify profile file modification
//
// On Windows, dirs::home_dir() uses the Windows API (SHGetKnownFolderPath)
// and cannot be redirected via HOME/USERPROFILE env vars. Therefore bash/zsh
// --add tests use the real home directory. PowerShell and Nushell --add tests
// use PROFILE and NU_CONFIG_PATH env vars respectively, which ARE respected.
// ---------------------------------------------------------------------------

#[test]
fn init_add_powershell_appends_eval_line() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join("profile.ps1");
    std::fs::write(&profile, "# ps profile\n").unwrap();

    isolated_cmd()
        .args(["init", "powershell", "--add"])
        .env("PROFILE", &profile)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Added shell integration"));

    let content = std::fs::read_to_string(&profile).unwrap();
    assert!(
        content.contains("wt init powershell"),
        "PowerShell profile should contain eval line, got: {}",
        content
    );
}

#[test]
fn init_add_powershell_detects_duplicate() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join("profile.ps1");
    // Pre-populate with an eval line matching the expected pattern
    std::fs::write(
        &profile,
        "Invoke-Expression (& wt init powershell | Out-String)\n",
    )
    .unwrap();
    let original = std::fs::read_to_string(&profile).unwrap();

    isolated_cmd()
        .args(["init", "powershell", "--add"])
        .env("PROFILE", &profile)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("already configured"));

    let after = std::fs::read_to_string(&profile).unwrap();
    assert_eq!(
        original, after,
        "Profile should not be modified when eval line already exists"
    );
}

#[test]
fn init_add_creates_profile_when_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = tmp.path().join("new_profile.ps1");
    // File does not exist yet

    isolated_cmd()
        .args(["init", "powershell", "--add"])
        .env("PROFILE", &profile)
        .current_dir(tmp.path())
        .assert()
        .success();

    assert!(
        profile.exists(),
        "Profile file should be created by init --add"
    );
    let content = std::fs::read_to_string(&profile).unwrap();
    assert!(
        content.contains("wt init powershell"),
        "Newly created profile should contain eval line, got: {}",
        content
    );
}

#[test]
fn init_add_nushell_appends_eval_line() {
    let tmp = tempfile::tempdir().unwrap();
    let config_nu = tmp.path().join("config.nu");
    std::fs::write(&config_nu, "# nushell config\n").unwrap();

    isolated_cmd()
        .args(["init", "nushell", "--add"])
        .env("NU_CONFIG_PATH", &config_nu)
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Added shell integration"));

    let content = std::fs::read_to_string(&config_nu).unwrap();
    assert!(
        content.contains("wt init nushell"),
        "Nushell config should contain eval line, got: {}",
        content
    );
}
