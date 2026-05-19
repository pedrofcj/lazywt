use anyhow::Result;
use crate::config::Config;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;
use crate::prompt;

/// Remove all non-default worktrees with confirmation.
///
/// Steps per RESEARCH.md Remove-All Command Sequence:
/// 1. List worktrees and detect default branch
/// 2. Collect removable worktrees (exclude bare and default)
/// 3. Show what will be removed and prompt for confirmation
/// 4. Remove each worktree, continuing on individual failures
pub fn run(ctx: &ProjectContext, _config: &Config, yes: bool) -> Result<()> {
    // 1. List worktrees
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;

    // 2. Detect default branch (no network)
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?;

    // 3. Collect removable worktrees (RALL-03): exclude bare, default-named, default-branched
    let removable: Vec<&git::worktree::Worktree> = worktrees
        .iter()
        .filter(|wt| {
            if wt.is_bare {
                return false;
            }
            let wt_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if let Some(ref default_name) = default_branch {
                if wt_name == default_name.as_str() {
                    return false;
                }
                if wt.branch.as_deref() == Some(default_name.as_str()) {
                    return false;
                }
            }
            true
        })
        .collect();

    // 4. If nothing to remove
    if removable.is_empty() {
        output::info("No worktrees to remove.");
        return Ok(());
    }

    // 5. Print header
    output::header("Remove All Worktrees");

    // 6. List what will be removed
    for wt in &removable {
        let wt_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        output::info(&format!("  {} {}", output::CROSS, wt_name));
    }

    // 7. Prompt (RALL-02)
    if !prompt::confirm("Remove all listed worktrees?", yes)? {
        output::info("Cancelled.");
        return Ok(());
    }

    // 8. Process each worktree
    // Change CWD to project_dir before removal. On Windows, a directory cannot
    // be deleted while any process holds it as CWD.  If the user ran lazywt
    // from inside a worktree being removed, the OS would block the delete.
    // Moving CWD to the bare repo root releases the directory lock.
    std::env::set_current_dir(&ctx.project_dir).ok();
    let mut removed = 0;
    let mut failed = 0;
    for wt in &removable {
        let wt_name = wt
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let wt_path_str = wt.path.to_str().unwrap_or_default();
        match git::git(&ctx.project_dir, &["worktree", "remove", wt_path_str, "--force"]) {
            Ok(_) => {
                // Delete branch if present and not default
                if let Some(ref branch) = wt.branch {
                    if default_branch.as_deref() != Some(branch.as_str()) {
                        let _ = git::git(&ctx.project_dir, &["branch", "-D", branch]);
                    }
                }
                removed += 1;
                output::success(&format!("Removed '{}'", wt_name));
            }
            Err(e) => {
                failed += 1;
                output::warning(&format!("Failed to remove '{}': {}", wt_name, e));
            }
        }
    }

    // 9. Print summary
    if failed == 0 {
        output::success(&format!("All {} worktrees removed", removed));
    } else {
        output::warning(&format!("{} removed, {} failed", removed, failed));
    }

    Ok(())
}
