use anyhow::Result;
use crate::config::Config;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;

pub fn run(ctx: &ProjectContext, _config: &Config, all: bool) -> Result<()> {
    // Step 1: Fetch all remotes (D-91)
    output::header("Syncing worktrees");
    output::info("Fetching all remotes...");
    git::git(&ctx.project_dir, &["fetch", "--all", "--prune"])?;
    output::success("Fetched all remotes");

    // Step 2: Detect default branch
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?
        .ok_or_else(|| anyhow::anyhow!("Could not detect default branch"))?;

    // Step 3: Fast-forward default branch (D-91)
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
    let default_wt = worktrees.iter().find(|wt| {
        !wt.is_bare && wt.branch.as_deref() == Some(default_branch.as_str())
    });

    if let Some(wt) = default_wt {
        if wt.path.exists() {
            // Use explicit @{upstream} ref with --ff-only.
            // `git merge --ff-only` with no ref is unreliable.
            match git::git(&wt.path, &["merge", "--ff-only", "@{upstream}"]) {
                Ok(_) => output::success(&format!("Fast-forwarded {}", default_branch)),
                Err(_) => output::warning(&format!(
                    "Could not fast-forward {} (may have local changes, diverged, or no upstream set)",
                    default_branch
                )),
            }
        }
    }

    // Step 4: Check for deleted remote branches (D-91)
    report_gone_branches(&ctx.project_dir, &worktrees)?;

    // Step 5: If --all, fast-forward all worktree branches (D-92)
    if all {
        sync_all_branches(&ctx.project_dir, &worktrees, &default_branch)?;
    }

    output::success("Sync complete");
    Ok(())
}

/// Report worktrees tracking deleted remote branches (D-91).
fn report_gone_branches(
    project_dir: &std::path::Path,
    worktrees: &[git::worktree::Worktree],
) -> Result<()> {
    // Use --format alone (no -vv flag -- it's ignored when --format is specified)
    let output = git::git(
        project_dir,
        &["branch", "--format=%(refname:short) %(upstream:track)"],
    )?;
    let gone_branches: std::collections::HashSet<String> = output
        .lines()
        .filter(|l| l.contains("[gone]"))
        .filter_map(|l| l.split_whitespace().next().map(|s| s.to_string()))
        .collect();

    if gone_branches.is_empty() {
        return Ok(());
    }

    println!();
    output::warning("Worktrees tracking deleted remote branches:");
    for wt in worktrees {
        if let Some(ref branch) = wt.branch {
            if gone_branches.contains(branch) {
                let name = wt
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");
                output::warning(&format!("  {} (branch: {})", name, branch));
            }
        }
    }
    Ok(())
}

/// Fast-forward all non-default worktree branches (D-92).
fn sync_all_branches(
    _project_dir: &std::path::Path,
    worktrees: &[git::worktree::Worktree],
    default_branch: &str,
) -> Result<()> {
    println!();
    output::info("Fast-forwarding all branches...");

    for wt in worktrees {
        if wt.is_bare || wt.is_prunable || !wt.path.exists() {
            continue;
        }
        let branch = wt.branch.as_deref().unwrap_or("");
        if branch == default_branch || branch.is_empty() {
            continue;
        }

        let name = wt
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");

        // Check dirty status before ff
        let status = git::status::worktree_status(&wt.path).unwrap_or_default();
        if !status.is_clean() {
            output::warning(&format!("  {} -- skipped (dirty)", name));
            continue;
        }

        // Check if branch has a standard upstream (@{upstream} resolves).
        let has_upstream =
            git::git(&wt.path, &["rev-parse", "--abbrev-ref", "@{upstream}"]).is_ok();

        if has_upstream {
            // Standard upstream: use --ff-only @{upstream}.
            match git::git(&wt.path, &["merge", "--ff-only", "@{upstream}"]) {
                Ok(_) => output::success(&format!("  {} -- fast-forwarded", name)),
                Err(_) => output::info(&format!(
                    "  {} -- already up to date or diverged",
                    name
                )),
            }
        } else {
            // No standard upstream. Check for PR-style tracking (refs/pull/N/head).
            // These refs live outside refs/heads/* so git fetch --all doesn't fetch them,
            // which means @{upstream} can't resolve. Fetch the ref explicitly instead.
            let merge_ref = git::git(
                &wt.path,
                &["config", &format!("branch.{}.merge", branch)],
            )
            .ok()
            .map(|s| s.trim().to_string());

            if let Some(ref mref) = merge_ref {
                if mref.starts_with("refs/pull/") {
                    let remote = git::git(
                        &wt.path,
                        &["config", &format!("branch.{}.remote", branch)],
                    )
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "origin".to_string());

                    // Fetch the PR ref explicitly, then merge FETCH_HEAD
                    if git::git(&wt.path, &["fetch", &remote, mref]).is_ok() {
                        match git::git(&wt.path, &["merge", "--ff-only", "FETCH_HEAD"]) {
                            Ok(_) => output::success(&format!("  {} -- fast-forwarded", name)),
                            Err(_) => output::info(&format!(
                                "  {} -- already up to date or diverged",
                                name
                            )),
                        }
                    } else {
                        output::warning(&format!("  {} -- fetch failed for {}", name, mref));
                    }
                    continue;
                }
            }

            output::info(&format!("  {} -- skipped (no upstream)", name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // Sync functions require a real git repo with remotes, so unit tests
    // focus on ensuring the module compiles and the function signatures
    // are correct. CLI parse tests in cli.rs cover the argument parsing.

    #[test]
    fn sync_module_compiles() {
        // If this test runs, the module compiled successfully
        assert!(true);
    }
}
