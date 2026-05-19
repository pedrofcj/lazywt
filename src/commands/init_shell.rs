use anyhow::Result;
use std::path::Path;

use crate::commands::alias;
use crate::config::Config;
use crate::output;

/// Output shell function code to stdout for eval in shell profiles,
/// or append eval line to the profile file with --add.
///
/// Usage: `eval "$(lazywt init)"` or `eval "$(lazywt init bash)"`
/// Or: `lazywt init --add` to auto-configure the shell profile.
pub fn run(shell_name: Option<&str>, add: bool, config: &Config) -> Result<()> {
    let shell = match shell_name {
        Some(name) => alias::parse_shell_name(name)?,
        None => alias::detect_shell()?,
    };

    if add {
        return run_add(&shell, config);
    }

    let function_text = alias::shell_function(&shell, &config.command_name);
    println!("{}", function_text);
    Ok(())
}

/// Generate the eval line that sources the shell function for a given shell.
///
/// Uses config.command_name (e.g., "wt" or "lazywt") -- NOT hardcoded "lazywt".
fn eval_line_for_shell(shell: &alias::Shell, command_name: &str) -> String {
    match shell {
        alias::Shell::Bash => {
            format!(r#"eval "$({command_name} init bash)""#)
        }
        alias::Shell::Zsh => {
            format!(r#"eval "$({command_name} init zsh)""#)
        }
        alias::Shell::PowerShell => {
            format!("Invoke-Expression (& {command_name} init powershell | Out-String)")
        }
        alias::Shell::Nushell => {
            // Nushell uses `source` with a generated file or inline eval
            format!(r#"{command_name} init nushell | save -f ~/.lazywt-init.nu; source ~/.lazywt-init.nu"#)
        }
    }
}

/// Generate the pattern used to detect an existing eval line in a profile.
///
/// Uses config.command_name so duplicate detection works regardless of
/// whether the user configured "wt" or "lazywt" as the command name.
fn eval_pattern_for_shell(command_name: &str) -> String {
    format!("{} init", command_name)
}

/// Append the eval line to the user's shell profile file.
fn run_add(shell: &alias::Shell, config: &Config) -> Result<()> {
    let profile = alias::profile_path(shell)?;
    let eval_line = eval_line_for_shell(shell, &config.command_name);
    let eval_pattern = eval_pattern_for_shell(&config.command_name);

    // Check for existing eval line (skip if present)
    if profile.exists() {
        let content = std::fs::read_to_string(&profile)?;
        if content.contains(&eval_pattern) {
            output::warning(&format!(
                "Shell integration already configured in {}",
                display_path(&profile)
            ));
            return Ok(());
        }
    }

    // Create parent dirs if needed
    if let Some(parent) = profile.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Append eval line
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile)?;
    writeln!(file)?;
    writeln!(file, "{}", eval_line)?;

    output::success(&format!(
        "Added shell integration to {}",
        display_path(&profile)
    ));
    output::info("Restart your shell or source the profile to activate.");
    Ok(())
}

/// Display a path using dunce for clean Windows paths.
fn display_path(path: &Path) -> String {
    dunce::simplified(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn eval_line_bash_uses_command_name() {
        let line = eval_line_for_shell(&alias::Shell::Bash, "wt");
        assert!(line.contains("wt init bash"), "eval line should use command_name 'wt'");
        assert!(!line.contains("lazywt"), "eval line should NOT hardcode 'lazywt'");
    }

    #[test]
    fn eval_line_zsh_uses_command_name() {
        let line = eval_line_for_shell(&alias::Shell::Zsh, "wt");
        assert!(line.contains("wt init zsh"), "eval line should use command_name 'wt'");
    }

    #[test]
    fn eval_line_powershell_uses_command_name() {
        let line = eval_line_for_shell(&alias::Shell::PowerShell, "wt");
        assert!(
            line.contains("wt init powershell"),
            "eval line should use command_name 'wt'"
        );
    }

    #[test]
    fn eval_line_nushell_uses_command_name() {
        let line = eval_line_for_shell(&alias::Shell::Nushell, "wt");
        assert!(
            line.contains("wt init nushell"),
            "eval line should use command_name 'wt'"
        );
    }

    #[test]
    fn eval_pattern_uses_command_name() {
        let pattern = eval_pattern_for_shell("wt");
        assert_eq!(pattern, "wt init");
    }

    #[test]
    fn eval_pattern_custom_name() {
        let pattern = eval_pattern_for_shell("lazywt");
        assert_eq!(pattern, "lazywt init");
    }

    #[test]
    fn run_add_appends_eval_line() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "# existing content").unwrap();
        let path = file.path().to_path_buf();

        // We can't easily call run_add directly because it uses alias::profile_path,
        // but we can test the helper functions and verify the logic
        let eval_line = eval_line_for_shell(&alias::Shell::Bash, "wt");
        assert!(eval_line.contains("wt init bash"));

        // Simulate what run_add does: check for duplicate, then append
        let content = std::fs::read_to_string(&path).unwrap();
        let pattern = eval_pattern_for_shell("wt");
        assert!(!content.contains(&pattern), "Should not find pattern yet");

        // Append
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "{}", eval_line).unwrap();
        drop(f);

        // Verify it was appended
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(&pattern), "Pattern should now be present");
    }

    #[test]
    fn run_add_skips_duplicate() {
        let mut file = NamedTempFile::new().unwrap();
        let eval_line = eval_line_for_shell(&alias::Shell::Bash, "wt");
        writeln!(file, "{}", eval_line).unwrap();
        let path = file.path().to_path_buf();

        // Check that duplicate detection works
        let content = std::fs::read_to_string(&path).unwrap();
        let pattern = eval_pattern_for_shell("wt");
        assert!(
            content.contains(&pattern),
            "Should detect existing eval line"
        );
    }
}
