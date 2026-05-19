use std::fs;
use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;

use owo_colors::OwoColorize;
use owo_colors::Stream;

use crate::config::Config;

/// Well-known file path for the navigation side channel.
/// Shell function wrappers read this after lazywt exits.
const NAVIGATE_FILE: &str = ".lazywt_cd";

/// Resolve the full path to the navigate file in the user's home directory.
/// Checks LAZYWT_HOME env var first (used by tests for isolation),
/// then falls back to dirs::home_dir().
fn navigate_path() -> Option<std::path::PathBuf> {
    // Allow tests to isolate the nav file via LAZYWT_HOME
    if let Ok(home) = std::env::var("LAZYWT_HOME") {
        return Some(std::path::PathBuf::from(home).join(NAVIGATE_FILE));
    }
    dirs::home_dir().map(|h| h.join(NAVIGATE_FILE))
}

/// Signal the shell wrapper to cd into the given path after lazywt exits.
/// Writes the path to ~/.lazywt_cd. The shell function reads and deletes it.
pub(crate) fn request_cd(path: &Path) {
    if let Some(nav_path) = navigate_path() {
        let _ = fs::write(&nav_path, dunce::simplified(path).display().to_string());
    }
}

/// Clean up the navigate file. Called at startup so stale files don't cause
/// unexpected cd on the next command.
pub fn cleanup() {
    if let Some(nav_path) = navigate_path() {
        let _ = fs::remove_file(&nav_path);
    }
}

/// Prompt or auto-navigate to the given path based on config.
///
/// - If `auto_navigate` is true: writes nav file immediately (no prompt).
/// - If `auto_navigate` is false and stdin is a TTY: prompts y/N.
/// - If stdin is not a TTY: skips silently.
pub fn maybe_navigate(path: &Path, config: &Config) {
    if config.auto_navigate {
        request_cd(path);
        return;
    }

    // Only prompt if stdin is a TTY
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return;
    }

    // Ask y/N
    let display_path = dunce::simplified(path).display().to_string();
    print!(
        "{}",
        format!("  Navigate to {}? (y/N) ", display_path)
            .if_supports_color(Stream::Stdout, |s| s.cyan())
    );
    let _ = std::io::stdout().flush();

    let mut answer = String::new();
    if stdin.lock().read_line(&mut answer).is_ok() {
        let answer = answer.trim().to_lowercase();
        if answer == "y" || answer == "yes" {
            request_cd(path);
        }
    }
}
