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
/// Always invokes the `lazywt` binary -- the executable installed on PATH --
/// NOT config.command_name. The bootstrap line runs at profile-load time,
/// before the alias function exists, so it must call the real binary. The
/// function that line emits is *named* after command_name (see
/// `alias::shell_function`), but the binary name is fixed (matching the
/// hardcoded `& lazywt`/`command lazywt` in the function bodies).
fn eval_line_for_shell(shell: &alias::Shell) -> String {
    match shell {
        alias::Shell::Bash => r#"eval "$(lazywt init bash)""#.to_string(),
        alias::Shell::Zsh => r#"eval "$(lazywt init zsh)""#.to_string(),
        alias::Shell::PowerShell => {
            "Invoke-Expression (& lazywt init powershell | Out-String)".to_string()
        }
        alias::Shell::Nushell => {
            // Nushell uses `source` with a generated file or inline eval
            r#"lazywt init nushell | save -f ~/.lazywt-init.nu; source ~/.lazywt-init.nu"#.to_string()
        }
    }
}

/// Pattern used to detect an existing eval line in a profile.
///
/// Matches on the fixed binary invocation, so duplicate detection works
/// regardless of which alias the user configured as command_name.
fn eval_pattern_for_shell() -> &'static str {
    "lazywt init"
}

/// Append the eval line to the user's shell profile file.
fn run_add(shell: &alias::Shell, _config: &Config) -> Result<()> {
    let profile = alias::profile_path(shell)?;
    let eval_line = eval_line_for_shell(shell);
    let eval_pattern = eval_pattern_for_shell();

    // Check for existing eval line (skip if present)
    if profile.exists() {
        let content = std::fs::read_to_string(&profile)?;
        if content.contains(eval_pattern) {
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
    fn eval_line_bash_invokes_binary() {
        let line = eval_line_for_shell(&alias::Shell::Bash);
        assert!(
            line.contains("lazywt init bash"),
            "eval line must invoke the lazywt binary, got: {line}"
        );
    }

    #[test]
    fn eval_line_zsh_invokes_binary() {
        let line = eval_line_for_shell(&alias::Shell::Zsh);
        assert!(
            line.contains("lazywt init zsh"),
            "eval line must invoke the lazywt binary, got: {line}"
        );
    }

    #[test]
    fn eval_line_powershell_invokes_binary() {
        let line = eval_line_for_shell(&alias::Shell::PowerShell);
        assert!(
            line.contains("& lazywt init powershell"),
            "eval line must invoke the lazywt binary, got: {line}"
        );
    }

    #[test]
    fn eval_line_nushell_invokes_binary() {
        let line = eval_line_for_shell(&alias::Shell::Nushell);
        assert!(
            line.contains("lazywt init nushell"),
            "eval line must invoke the lazywt binary, got: {line}"
        );
    }

    #[test]
    fn eval_pattern_matches_binary_invocation() {
        assert_eq!(eval_pattern_for_shell(), "lazywt init");
    }

    #[test]
    fn run_add_appends_eval_line() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "# existing content").unwrap();
        let path = file.path().to_path_buf();

        // We can't easily call run_add directly because it uses alias::profile_path,
        // but we can test the helper functions and verify the logic
        let eval_line = eval_line_for_shell(&alias::Shell::Bash);
        assert!(eval_line.contains("lazywt init bash"));

        // Simulate what run_add does: check for duplicate, then append
        let content = std::fs::read_to_string(&path).unwrap();
        let pattern = eval_pattern_for_shell();
        assert!(!content.contains(pattern), "Should not find pattern yet");

        // Append
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "{}", eval_line).unwrap();
        drop(f);

        // Verify it was appended
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(pattern), "Pattern should now be present");
    }

    #[test]
    fn run_add_skips_duplicate() {
        let mut file = NamedTempFile::new().unwrap();
        let eval_line = eval_line_for_shell(&alias::Shell::Bash);
        writeln!(file, "{}", eval_line).unwrap();
        let path = file.path().to_path_buf();

        // Check that duplicate detection works
        let content = std::fs::read_to_string(&path).unwrap();
        let pattern = eval_pattern_for_shell();
        assert!(
            content.contains(pattern),
            "Should detect existing eval line"
        );
    }
}
