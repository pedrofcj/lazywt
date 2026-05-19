use std::io::{self, BufRead, IsTerminal, Write};

/// Prompt the user for confirmation.
///
/// - If `yes_flag` is true, returns Ok(true) immediately (--yes bypass).
/// - If stdin is not a TTY, returns an error (D-20: safe refusal in non-interactive contexts).
/// - Otherwise, prints the message to stderr and reads a single line from stdin.
///   Returns Ok(true) if the user types "y" or "Y".
pub fn confirm(message: &str, yes_flag: bool) -> anyhow::Result<bool> {
    if yes_flag {
        return Ok(true);
    }

    if !io::stdin().is_terminal() {
        return Err(anyhow::anyhow!(
            "Cannot prompt for confirmation -- use --yes to skip"
        ));
    }

    eprint!("{} (y/N) ", message);
    io::stderr().flush()?;

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let trimmed = line.trim();

    Ok(trimmed == "y" || trimmed == "Y")
}
