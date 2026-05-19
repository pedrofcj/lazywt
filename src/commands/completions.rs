use anyhow::{Result, bail};
use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::Cli;

pub fn run(shell_name: &str) -> Result<()> {
    let shell = match shell_name.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "powershell" => Shell::PowerShell,
        "fish" => Shell::Fish,
        other => bail!(
            "Unsupported shell: '{}'. Supported: bash, zsh, powershell, fish",
            other
        ),
    };

    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, &bin_name, &mut std::io::stdout());
    Ok(())
}
