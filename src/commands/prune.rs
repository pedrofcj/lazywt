use anyhow::Result;
use crate::config::Config;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;
use crate::prompt;
use std::collections::HashSet;

#[derive(Debug)]
struct PruneCandidate {
    name: String,
    path: std::path::PathBuf,
    branch: Option<String>,
    reason: String, // "merged", "orphan"
}

/// Find worktrees that are candidates for pruning (D-88).
fn find_prune_candidates(
    ctx: &ProjectContext,
    default_branch: &str,
    force: bool,
) -> Result<(Vec<PruneCandidate>, Vec<(String, String)>)> {
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;

    // Get merged branches (D-88)
    let merged_branches: HashSet<String> = git::git(
        &ctx.project_dir,
        &["branch", "--merged", default_branch, "--format=%(refname:short)"],
    )
    .map(|out| out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    .unwrap_or_default();

    let mut candidates = Vec::new();
    let mut skipped_dirty = Vec::new();

    for wt in &worktrees {
        if wt.is_bare {
            continue;
        }
        let name = wt
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let branch_name = wt.branch.as_deref().unwrap_or("");

        // Skip the default branch worktree
        if branch_name == default_branch {
            continue;
        }

        // Check orphan (D-88: missing directory)
        if !wt.path.exists() || wt.is_prunable {
            candidates.push(PruneCandidate {
                name: name.clone(),
                path: wt.path.clone(),
                branch: wt.branch.clone(),
                reason: "orphan".to_string(),
            });
            continue;
        }

        // Check merged (D-88)
        let is_merged = !branch_name.is_empty() && merged_branches.contains(branch_name);

        if is_merged {
            // Filter false positives: branches at the exact same commit as
            // default (fresh branches with no work) should NOT be pruned.
            // Compare commit SHAs directly instead of rev-list --count,
            // because after a real merge all feature commits become reachable
            // from default, making rev-list --count return 0 for both fresh
            // branches AND genuinely merged branches.
            let default_sha = git::git(
                &ctx.project_dir,
                &["rev-parse", default_branch],
            )
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

            let branch_sha = git::git(
                &ctx.project_dir,
                &["rev-parse", branch_name],
            )
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

            if default_sha == branch_sha {
                // Fresh branch at exact same commit as default — skip
                continue;
            }

            // Check dirty status (D-89)
            let status = git::status::worktree_status(&wt.path).unwrap_or_default();
            if !status.is_clean() && !force {
                skipped_dirty.push((name, "dirty + merged".to_string()));
                continue;
            }
            candidates.push(PruneCandidate {
                name,
                path: wt.path.clone(),
                branch: wt.branch.clone(),
                reason: "merged".to_string(),
            });
        }
    }

    Ok((candidates, skipped_dirty))
}

pub fn run(ctx: &ProjectContext, _config: &Config, force: bool, yes: bool) -> Result<()> {
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?
        .ok_or_else(|| anyhow::anyhow!("Could not detect default branch"))?;

    let (candidates, skipped) = find_prune_candidates(ctx, &default_branch, force)?;

    // Report skipped dirty worktrees (D-89)
    if !skipped.is_empty() {
        output::warning("Skipped dirty worktrees (use --force to include):");
        for (name, reason) in &skipped {
            output::warning(&format!("  {} ({})", name, reason));
        }
        println!();
    }

    if candidates.is_empty() {
        output::info("Nothing to prune -- all worktrees are clean.");
        return Ok(());
    }

    // Show what will be removed (D-90)
    output::header("Prune worktrees");
    for c in &candidates {
        output::info(&format!("  {} ({})", c.name, c.reason));
    }
    println!();

    // Confirmation prompt (D-90)
    let msg = format!("Remove {} worktree(s)?", candidates.len());
    if !prompt::confirm(&msg, yes)? {
        output::info("Cancelled.");
        return Ok(());
    }

    // Pitfall 9: Move CWD away from worktrees being deleted (Windows lock)
    std::env::set_current_dir(&ctx.project_dir).ok();

    // Remove each candidate
    let mut removed = 0;
    for c in &candidates {
        let path_str = c.path.to_str().unwrap_or_default();

        // Remove worktree
        // Do NOT use --force unconditionally. Only use --force when the user
        // passes --force flag. Without this, --force bypasses git's lock
        // protection even when the user didn't ask for it.
        let result = if c.reason == "orphan" {
            // For orphaned worktrees, use git worktree prune to clean up git internal state
            git::git(&ctx.project_dir, &["worktree", "prune"])
        } else if force {
            // User explicitly asked for --force (D-89: --force removes dirty worktrees)
            git::git(
                &ctx.project_dir,
                &["worktree", "remove", path_str, "--force"],
            )
        } else {
            // Normal removal without --force
            git::git(&ctx.project_dir, &["worktree", "remove", path_str])
        };

        match result {
            Ok(_) => {
                // Delete branch after worktree removal
                if let Some(ref branch) = c.branch {
                    if c.reason == "merged" {
                        // Use -d for merged branches (safe — refuses if not fully merged)
                        let _ = git::git(&ctx.project_dir, &["branch", "-d", branch]);
                    } else if c.reason == "orphan" {
                        // Use -D for orphan branches (directory gone, branch cleanup needed)
                        let _ = git::git(&ctx.project_dir, &["branch", "-D", branch]);
                    }
                }
                output::success(&format!("Removed {} ({})", c.name, c.reason));
                removed += 1;
            }
            Err(e) => {
                output::warning(&format!("Failed to remove {}: {}", c.name, e));
            }
        }
    }

    output::success(&format!("Pruned {}/{} worktrees", removed, candidates.len()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn prune_candidate_merged_reason() {
        let candidate = PruneCandidate {
            name: "feature-auth".to_string(),
            path: PathBuf::from("/tmp/project/feature-auth"),
            branch: Some("feature/auth".to_string()),
            reason: "merged".to_string(),
        };
        assert_eq!(candidate.reason, "merged");
        assert_eq!(candidate.name, "feature-auth");
        assert_eq!(candidate.branch, Some("feature/auth".to_string()));
    }

    #[test]
    fn prune_candidate_orphan_reason() {
        let candidate = PruneCandidate {
            name: "old-worktree".to_string(),
            path: PathBuf::from("/tmp/project/old-worktree"),
            branch: Some("old-branch".to_string()),
            reason: "orphan".to_string(),
        };
        assert_eq!(candidate.reason, "orphan");
        assert_eq!(candidate.name, "old-worktree");
    }

    #[test]
    fn prune_candidate_orphan_no_branch() {
        let candidate = PruneCandidate {
            name: "ghost".to_string(),
            path: PathBuf::from("/tmp/project/ghost"),
            branch: None,
            reason: "orphan".to_string(),
        };
        assert_eq!(candidate.reason, "orphan");
        assert!(candidate.branch.is_none());
    }
}
