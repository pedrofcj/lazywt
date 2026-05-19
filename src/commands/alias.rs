use anyhow::{bail, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::output;

/// Supported shells for alias installation.
#[allow(clippy::enum_variant_names)]
pub(crate) enum Shell {
    PowerShell,
    Bash,
    Zsh,
    Nushell,
}

/// Detect the user's current shell from environment variables.
pub(crate) fn detect_shell() -> Result<Shell> {
    // 1. PowerShell sets PSModulePath
    if env::var("PSModulePath").is_ok() {
        return Ok(Shell::PowerShell);
    }

    // 2. Nushell sets NU_VERSION
    if env::var("NU_VERSION").is_ok() {
        return Ok(Shell::Nushell);
    }

    // 3. SHELL env var (Unix-style)
    if let Ok(shell_val) = env::var("SHELL") {
        if shell_val.contains("zsh") {
            return Ok(Shell::Zsh);
        }
        if shell_val.contains("bash") {
            return Ok(Shell::Bash);
        }
    }

    // 4. Windows default: PowerShell
    #[cfg(target_os = "windows")]
    {
        Ok(Shell::PowerShell)
    }

    // 5. Nothing matched
    #[cfg(not(target_os = "windows"))]
    {
        bail!("Could not detect your shell. Supported: PowerShell, Bash, Zsh, Nushell");
    }
}

/// Resolve the profile file path for the given shell.
pub(crate) fn profile_path(shell: &Shell) -> Result<PathBuf> {
    match shell {
        Shell::PowerShell => {
            // Check PROFILE env var first (set inside PowerShell sessions)
            if let Ok(profile) = env::var("PROFILE") {
                return Ok(PathBuf::from(profile));
            }
            // Fallback based on platform
            #[cfg(target_os = "windows")]
            {
                if let Some(docs) = dirs::document_dir() {
                    return Ok(docs.join("PowerShell").join("Microsoft.PowerShell_profile.ps1"));
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                if let Some(home) = dirs::home_dir() {
                    return Ok(home
                        .join(".config")
                        .join("powershell")
                        .join("Microsoft.PowerShell_profile.ps1"));
                }
            }
            bail!("Could not determine PowerShell profile path");
        }
        Shell::Bash => {
            let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
            Ok(home.join(".bashrc"))
        }
        Shell::Zsh => {
            let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
            Ok(home.join(".zshrc"))
        }
        Shell::Nushell => {
            // Check NU_CONFIG_PATH env var first
            if let Ok(config_path) = env::var("NU_CONFIG_PATH") {
                return Ok(PathBuf::from(config_path));
            }
            // Fallback based on platform
            #[cfg(target_os = "windows")]
            {
                if let Some(config_dir) = dirs::config_dir() {
                    return Ok(config_dir.join("nushell").join("config.nu"));
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                if let Some(home) = dirs::home_dir() {
                    return Ok(home.join(".config").join("nushell").join("config.nu"));
                }
            }
            bail!("Could not determine Nushell config path");
        }
    }
}

/// Parse a shell name string into a Shell enum variant (case-insensitive).
pub(crate) fn parse_shell_name(name: &str) -> Result<Shell> {
    match name.to_lowercase().as_str() {
        "bash" => Ok(Shell::Bash),
        "zsh" => Ok(Shell::Zsh),
        "powershell" | "pwsh" => Ok(Shell::PowerShell),
        "nushell" | "nu" => Ok(Shell::Nushell),
        other => bail!(
            "Unknown shell '{}'. Supported: bash, zsh, powershell, nushell",
            other
        ),
    }
}

/// Generate a shell function wrapper for a given shell and command name.
///
/// The wrapper runs lazywt normally (stdout/stderr flow directly to the terminal),
/// then checks ~/.lazywt_cd for a navigation request. If the file exists, the
/// function cds into the path and deletes the file.
pub(crate) fn shell_function(shell: &Shell, name: &str) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => format!(
            r#"{name}() {{
    command lazywt "$@"
    local exit_code=$?
    local nav_file="$HOME/.lazywt_cd"
    if [ -f "$nav_file" ]; then
        local cd_path
        cd_path=$(cat "$nav_file")
        rm -f "$nav_file"
        if [ -n "$cd_path" ] && [ -d "$cd_path" ]; then
            export LAZYWT_PREV="$PWD"
            cd "$cd_path" || true
        fi
    fi
    return $exit_code
}}"#,
            name = name
        ),
        Shell::PowerShell => format!(
            r#"function {name} {{
    & lazywt @Args
    $navFile = Join-Path $HOME ".lazywt_cd"
    if (Test-Path $navFile) {{
        $cdPath = (Get-Content $navFile -Raw).Trim()
        Remove-Item $navFile -Force
        if ($cdPath -and (Test-Path $cdPath)) {{
            $env:LAZYWT_PREV = (Get-Location).Path
            Set-Location $cdPath
        }}
    }}
}}"#,
            name = name
        ),
        Shell::Nushell => format!(
            r#"def --env {name} [...args: string] {{
    lazywt ...$args
    let nav_file = ($env.HOME | path join ".lazywt_cd")
    if ($nav_file | path exists) {{
        let cd_path = (open $nav_file | str trim)
        rm $nav_file
        if ($cd_path | is-not-empty) {{
            $env.LAZYWT_PREV = (pwd)
            cd $cd_path
        }}
    }}
}}"#,
            name = name
        ),
    }
}

/// Return the pattern used to detect an existing function definition in a profile.
fn duplicate_pattern(shell: &Shell, name: &str) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => format!("{}()", name),
        Shell::PowerShell => format!("function {}", name),
        Shell::Nushell => format!("def --env {}", name),
    }
}

/// Generate the restart hint for a given shell and profile path.
fn restart_hint(shell: &Shell, profile: &str) -> String {
    match shell {
        Shell::PowerShell => format!("Restart your shell or run '. {}' to activate", profile),
        Shell::Nushell => format!("Restart your shell or run 'source {}' to activate", profile),
        Shell::Bash | Shell::Zsh => {
            format!("Restart your shell or run 'source {}' to activate", profile)
        }
    }
}

/// Set up a shell alias for lazywt in the user's shell profile.
pub fn run(config: &Config) -> Result<()> {
    let shell = detect_shell()?;
    let profile = profile_path(&shell)?;
    let profile_display = dunce::simplified(&profile).display().to_string();
    let name = &config.command_name;
    let function_def = shell_function(&shell, name);
    let dup_pattern = duplicate_pattern(&shell, name);

    // Check for duplicate (matches function definition opening, not the old alias line)
    if profile.exists() {
        let content = fs::read_to_string(&profile)?;
        if content.contains(&dup_pattern) {
            output::info(&format!(
                "Alias '{}' is already configured in {}",
                name, profile_display
            ));
            return Ok(());
        }
    }

    // Create parent dirs if needed
    if let Some(parent) = profile.parent() {
        fs::create_dir_all(parent)?;
    }

    // Append shell function wrapper
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile)?;
    writeln!(file)?;
    writeln!(file, "{}", function_def)?;

    output::success(&format!("Added alias '{}' to {}", name, profile_display));
    output::info(&restart_hint(&shell, &profile_display));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_function_bash_checks_nav_file() {
        let output = shell_function(&Shell::Bash, "wt");
        assert!(output.starts_with("wt()"), "Bash function should start with 'wt()'");
        assert!(output.contains(".lazywt_cd"), "Bash function should check .lazywt_cd file");
    }

    #[test]
    fn shell_function_zsh_checks_nav_file() {
        let output = shell_function(&Shell::Zsh, "wt");
        assert!(output.starts_with("wt()"), "Zsh function should start with 'wt()'");
        assert!(output.contains(".lazywt_cd"), "Zsh function should check .lazywt_cd file");
    }

    #[test]
    fn shell_function_powershell_checks_nav_file() {
        let output = shell_function(&Shell::PowerShell, "wt");
        assert!(output.starts_with("function wt"), "PowerShell function should start with 'function wt'");
        assert!(output.contains(".lazywt_cd"), "PowerShell function should check .lazywt_cd file");
    }

    #[test]
    fn shell_function_nushell_checks_nav_file() {
        let output = shell_function(&Shell::Nushell, "wt");
        assert!(output.starts_with("def --env wt"), "Nushell function should start with 'def --env wt'");
        assert!(output.contains(".lazywt_cd"), "Nushell function should check .lazywt_cd file");
    }

    #[test]
    fn duplicate_pattern_bash() {
        assert_eq!(duplicate_pattern(&Shell::Bash, "wt"), "wt()");
    }

    #[test]
    fn duplicate_pattern_zsh() {
        assert_eq!(duplicate_pattern(&Shell::Zsh, "wt"), "wt()");
    }

    #[test]
    fn duplicate_pattern_powershell() {
        assert_eq!(duplicate_pattern(&Shell::PowerShell, "wt"), "function wt");
    }

    #[test]
    fn duplicate_pattern_nushell() {
        assert_eq!(duplicate_pattern(&Shell::Nushell, "wt"), "def --env wt");
    }

    #[test]
    fn parse_shell_name_bash() {
        assert!(matches!(parse_shell_name("bash").unwrap(), Shell::Bash));
    }

    #[test]
    fn parse_shell_name_zsh() {
        assert!(matches!(parse_shell_name("zsh").unwrap(), Shell::Zsh));
    }

    #[test]
    fn parse_shell_name_powershell() {
        assert!(matches!(
            parse_shell_name("powershell").unwrap(),
            Shell::PowerShell
        ));
    }

    #[test]
    fn parse_shell_name_powershell_alias() {
        assert!(matches!(
            parse_shell_name("pwsh").unwrap(),
            Shell::PowerShell
        ));
    }

    #[test]
    fn parse_shell_name_nushell() {
        assert!(matches!(
            parse_shell_name("nushell").unwrap(),
            Shell::Nushell
        ));
    }

    #[test]
    fn parse_shell_name_nushell_alias() {
        assert!(matches!(parse_shell_name("nu").unwrap(), Shell::Nushell));
    }

    #[test]
    fn parse_shell_name_case_insensitive() {
        assert!(matches!(parse_shell_name("BASH").unwrap(), Shell::Bash));
    }

    #[test]
    fn parse_shell_name_unknown() {
        assert!(parse_shell_name("fish").is_err());
    }

    // --- restart_hint tests ---

    #[test]
    fn restart_hint_powershell_contains_restart() {
        let hint = restart_hint(&Shell::PowerShell, "/some/path");
        assert!(
            hint.contains("Restart"),
            "PowerShell hint should contain 'Restart', got: {}",
            hint
        );
    }

    #[test]
    fn restart_hint_bash_contains_source() {
        let hint = restart_hint(&Shell::Bash, "/some/path");
        assert!(
            hint.contains("source"),
            "Bash hint should contain 'source', got: {}",
            hint
        );
    }

    #[test]
    fn restart_hint_zsh_contains_source() {
        let hint = restart_hint(&Shell::Zsh, "/some/path");
        assert!(
            hint.contains("source"),
            "Zsh hint should contain 'source', got: {}",
            hint
        );
    }

    #[test]
    fn restart_hint_nushell_contains_source() {
        let hint = restart_hint(&Shell::Nushell, "/some/path");
        assert!(
            hint.contains("source"),
            "Nushell hint should contain 'source', got: {}",
            hint
        );
    }

    // --- profile_path tests ---

    #[test]
    fn profile_path_bash_ends_with_bashrc() {
        let path = profile_path(&Shell::Bash).unwrap();
        assert!(
            path.to_str().unwrap().ends_with(".bashrc"),
            "Bash profile path should end with .bashrc, got: {:?}",
            path
        );
    }

    #[test]
    fn profile_path_zsh_ends_with_zshrc() {
        let path = profile_path(&Shell::Zsh).unwrap();
        assert!(
            path.to_str().unwrap().ends_with(".zshrc"),
            "Zsh profile path should end with .zshrc, got: {:?}",
            path
        );
    }

    #[test]
    fn profile_path_powershell_with_env_var() {
        // Set PROFILE env var to test that code path
        std::env::set_var("PROFILE", "/test/profile.ps1");
        let path = profile_path(&Shell::PowerShell).unwrap();
        assert_eq!(
            path.to_str().unwrap(),
            "/test/profile.ps1",
            "Should use PROFILE env var when set"
        );
        std::env::remove_var("PROFILE");
    }

    #[test]
    fn profile_path_nushell_with_env_var() {
        // Set NU_CONFIG_PATH env var to test that code path
        std::env::set_var("NU_CONFIG_PATH", "/test/config.nu");
        let path = profile_path(&Shell::Nushell).unwrap();
        assert_eq!(
            path.to_str().unwrap(),
            "/test/config.nu",
            "Should use NU_CONFIG_PATH env var when set"
        );
        std::env::remove_var("NU_CONFIG_PATH");
    }

    // --- detect_shell tests ---
    // Note: detect_shell depends on env vars and platform, so we test
    // the code paths by setting specific env vars.

    #[test]
    fn detect_shell_with_nu_version() {
        // Save and clear other vars that might interfere
        let saved_ps = std::env::var("PSModulePath").ok();
        std::env::remove_var("PSModulePath");
        std::env::set_var("NU_VERSION", "0.80.0");

        let result = detect_shell();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Shell::Nushell));

        std::env::remove_var("NU_VERSION");
        if let Some(v) = saved_ps {
            std::env::set_var("PSModulePath", v);
        }
    }

    // LAZYWT_PREV tracking tests

    #[test]
    fn shell_function_bash_sets_lazywt_prev() {
        let output = shell_function(&Shell::Bash, "wt");
        assert!(
            output.contains(r#"LAZYWT_PREV="$PWD""#),
            "Bash function should set LAZYWT_PREV to $PWD"
        );
    }

    #[test]
    fn shell_function_zsh_sets_lazywt_prev() {
        let output = shell_function(&Shell::Zsh, "wt");
        assert!(
            output.contains(r#"LAZYWT_PREV="$PWD""#),
            "Zsh function should set LAZYWT_PREV to $PWD"
        );
    }

    #[test]
    fn shell_function_powershell_sets_lazywt_prev() {
        let output = shell_function(&Shell::PowerShell, "wt");
        assert!(
            output.contains("$env:LAZYWT_PREV"),
            "PowerShell function should set $env:LAZYWT_PREV"
        );
    }

    #[test]
    fn shell_function_nushell_sets_lazywt_prev() {
        let output = shell_function(&Shell::Nushell, "wt");
        assert!(
            output.contains("$env.LAZYWT_PREV"),
            "Nushell function should set $env.LAZYWT_PREV"
        );
    }

    #[test]
    fn shell_function_bash_lazywt_prev_before_cd() {
        let output = shell_function(&Shell::Bash, "wt");
        let prev_pos = output.find("LAZYWT_PREV").expect("LAZYWT_PREV should exist");
        let cd_pos = output.find(r#"cd "$cd_path""#).expect("cd should exist");
        assert!(
            prev_pos < cd_pos,
            "LAZYWT_PREV must be set BEFORE cd in Bash template"
        );
    }

    #[test]
    fn shell_function_powershell_lazywt_prev_before_set_location() {
        let output = shell_function(&Shell::PowerShell, "wt");
        let prev_pos = output.find("LAZYWT_PREV").expect("LAZYWT_PREV should exist");
        let cd_pos = output.find("Set-Location").expect("Set-Location should exist");
        assert!(
            prev_pos < cd_pos,
            "LAZYWT_PREV must be set BEFORE Set-Location in PowerShell template"
        );
    }

    #[test]
    fn shell_function_nushell_lazywt_prev_before_cd() {
        let output = shell_function(&Shell::Nushell, "wt");
        let prev_pos = output.find("LAZYWT_PREV").expect("LAZYWT_PREV should exist");
        let cd_pos = output.find("cd $cd_path").expect("cd should exist");
        assert!(
            prev_pos < cd_pos,
            "LAZYWT_PREV must be set BEFORE cd in Nushell template"
        );
    }
}
