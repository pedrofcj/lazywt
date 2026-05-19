use anyhow::Result;

use crate::git;
use crate::layout::ProjectContext;
use crate::output;

const EXPECTED_REFSPEC: &str = "+refs/heads/*:refs/remotes/origin/*";

pub fn run(ctx: &ProjectContext) -> Result<()> {
    output::header("Checking fetch refspec configuration");
    println!();

    // Read current refspec
    let current = git::config::get_config(&ctx.project_dir, "remote.origin.fetch")?;

    match &current {
        Some(value) if value == EXPECTED_REFSPEC => {
            output::info(&format!("Current fetch refspec: {}", value));
            output::success("Fetch refspec is already correctly configured");
            return Ok(());
        }
        Some(value) => {
            output::info(&format!("Current fetch refspec: {}", value));
            output::warning(&format!(
                "Fetch refspec is configured but not optimal\n   \
                 Current:  {}\n   \
                 Expected: {}",
                value, EXPECTED_REFSPEC
            ));
        }
        None => {
            output::warning("Fetch refspec is not configured");
        }
    }

    // Set the correct refspec
    git::config::set_config(&ctx.project_dir, "remote.origin.fetch", EXPECTED_REFSPEC)?;

    // Fetch all branches
    output::progress_start("Fetching all branches from origin");
    match git::git(&ctx.project_dir, &["fetch", "origin"]) {
        Ok(_) => {
            output::progress_complete("Fetched all branches", output::Status::Success);
        }
        Err(e) => {
            output::progress_complete("Failed to fetch", output::Status::Warning);
            output::warning(&format!("Could not fetch from origin: {}", e));
            output::warning(
                "The refspec has been fixed, but you may need to fetch manually.",
            );
        }
    }

    output::success("Fetch refspec fixed successfully!");
    output::info("   Remote branches are now available as 'remotes/origin/<branch-name>'");
    output::info("   You can now create worktrees from remote branches");

    Ok(())
}
