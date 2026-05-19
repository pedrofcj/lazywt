use anyhow::{bail, Result};
use std::process::Command;

use crate::config::Config;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;

/// Parse PR number from either a raw number or a GitHub URL (D-84).
/// Validates that URL host is github.com to prevent accidentally
/// accepting GitLab, Bitbucket, or other hosts.
fn parse_pr_number(input: &str) -> Result<u64> {
    // Try direct number first
    if let Ok(n) = input.parse::<u64>() {
        return Ok(n);
    }
    // Try GitHub URL: https://github.com/owner/repo/pull/123
    let stripped = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"));
    if let Some(rest) = stripped {
        let parts: Vec<&str> = rest.split('/').collect();
        // Validate host is github.com
        if parts.is_empty() || parts[0] != "github.com" {
            bail!(
                "Only GitHub URLs are supported. Got host '{}'. \
                 Use a PR number instead for other forges.",
                parts.first().unwrap_or(&"unknown")
            );
        }
        // pattern: github.com/owner/repo/pull/N[/...]
        if parts.len() >= 5 && parts[3] == "pull" {
            if let Ok(n) = parts[4].parse::<u64>() {
                return Ok(n);
            }
        }
    }
    bail!(
        "Could not parse PR number from '{}'. Use a number or GitHub URL \
         (https://github.com/owner/repo/pull/N).",
        input
    )
}

/// Default worktree name for a PR (D-85).
fn default_worktree_name(pr_number: u64) -> String {
    format!("pr-{}", pr_number)
}

/// PR metadata from gh CLI.
struct PrMetadata {
    branch: String,
    /// Remote to fetch from. For forks, this is the fork owner's remote URL.
    /// For same-repo PRs, this is "origin".
    fetch_remote: String,
}

/// Try to get PR branch name and remote using gh CLI (D-86).
/// Fetches headRefName AND headRepositoryOwner.login to correctly
/// handle fork PRs. For forks, we need to fetch from the fork's remote, not origin.
fn gh_pr_metadata(pr_number: u64) -> Option<PrMetadata> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_number.to_string(),
            "--json",
            "headRefName,headRepositoryOwner",
            "-q",
            ".headRefName + \"\\n\" + .headRepositoryOwner.login",
        ])
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut lines = stdout.trim().lines();
        let branch = lines.next()?.to_string();
        let pr_owner = lines.next().unwrap_or("").to_string();

        if branch.is_empty() {
            return None;
        }

        // Check if this is a fork PR by comparing PR owner with repo owner
        let fetch_remote = if !pr_owner.is_empty() {
            let repo_owner = Command::new("gh")
                .args(["repo", "view", "--json", "owner", "-q", ".owner.login"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            if !repo_owner.is_empty() && pr_owner != repo_owner {
                // Fork PR: construct the fork URL to fetch from
                let repo_name = Command::new("gh")
                    .args(["repo", "view", "--json", "name", "-q", ".name"])
                    .output()
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "repo".to_string());
                format!("https://github.com/{}/{}.git", pr_owner, repo_name)
            } else {
                "origin".to_string()
            }
        } else {
            "origin".to_string()
        };

        Some(PrMetadata {
            branch,
            fetch_remote,
        })
    } else {
        None
    }
}

pub fn run(
    ctx: &ProjectContext,
    config: &Config,
    target: &str,
    name: Option<&str>,
) -> Result<()> {
    let pr_number = parse_pr_number(target)?;
    let wt_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| default_worktree_name(pr_number));
    let wt_path = ctx.worktree_parent.join(&wt_name);

    if wt_path.exists() {
        bail!("Worktree directory '{}' already exists", wt_name);
    }

    output::header(&format!("Checking out PR #{}", pr_number));

    // Try gh CLI first for branch name and remote info (D-86)
    let pr_meta = gh_pr_metadata(pr_number);

    let wt_path_str = wt_path.to_str().unwrap();

    if let Some(ref meta) = pr_meta {
        output::info(&format!("  Branch: {} (from gh CLI)", meta.branch));
        if meta.fetch_remote != "origin" {
            output::info(&format!("  Fork remote: {}", meta.fetch_remote));
        }

        // Fetch the branch from the correct remote
        // For fork PRs, fetch from the fork URL, not just origin
        let fetch_result = git::git(
            &ctx.project_dir,
            &["fetch", &meta.fetch_remote, &meta.branch],
        );
        if let Err(e) = fetch_result {
            output::warning(&format!("  Fetch warning: {}", e));
        }

        // Create worktree tracking the remote branch (D-87)
        let remote_ref = if meta.fetch_remote == "origin" {
            format!("origin/{}", meta.branch)
        } else {
            // For fork, we fetched directly; use FETCH_HEAD
            "FETCH_HEAD".to_string()
        };

        if git::branch::branch_exists(&ctx.project_dir, &meta.branch)? {
            git::git(
                &ctx.project_dir,
                &["worktree", "add", wt_path_str, &meta.branch],
            )?;
        } else {
            git::git(
                &ctx.project_dir,
                &[
                    "worktree",
                    "add",
                    "-b",
                    &meta.branch,
                    wt_path_str,
                    &remote_ref,
                ],
            )?;
        }

        // Set upstream tracking (D-87)
        if meta.fetch_remote == "origin" {
            let _ = git::branch::set_upstream(&wt_path, &meta.branch);
        } else {
            // Fork PR: set tracking to refs/pull/N/head on origin so sync --all works.
            // This matches the fallback path behavior (lines 217-232).
            let _ = git::git(
                &wt_path,
                &[
                    "config",
                    &format!("branch.{}.remote", meta.branch),
                    "origin",
                ],
            );
            let _ = git::git(
                &wt_path,
                &[
                    "config",
                    &format!("branch.{}.merge", meta.branch),
                    &format!("refs/pull/{}/head", pr_number),
                ],
            );
        }
    } else {
        // Fallback: fetch refs/pull/N/head directly (D-86)
        output::info("  gh CLI not available, using git fetch fallback");
        let local_branch = format!("pr-{}", pr_number);
        let refspec = format!("pull/{}/head:{}", pr_number, local_branch);

        output::info(&format!("  Fetching refs/pull/{}/head", pr_number));
        git::git(&ctx.project_dir, &["fetch", "origin", &refspec])?;

        // Create worktree on the fetched branch
        git::git(
            &ctx.project_dir,
            &["worktree", "add", wt_path_str, &local_branch],
        )?;

        // Set up tracking to the PR ref for future fetches (D-87)
        // Configure the branch to track the PR ref so `git pull` works.
        let _ = git::git(
            &wt_path,
            &[
                "config",
                &format!("branch.{}.remote", local_branch),
                "origin",
            ],
        );
        let _ = git::git(
            &wt_path,
            &[
                "config",
                &format!("branch.{}.merge", local_branch),
                &format!("refs/pull/{}/head", pr_number),
            ],
        );
    }

    output::success(&format!("PR #{} checked out to '{}'", pr_number, wt_name));

    // Navigate to new worktree
    crate::navigate::maybe_navigate(&wt_path, config);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_number_plain_number() {
        let result = parse_pr_number("42").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn parse_pr_number_github_https_url() {
        let result =
            parse_pr_number("https://github.com/owner/repo/pull/123").unwrap();
        assert_eq!(result, 123);
    }

    #[test]
    fn parse_pr_number_github_http_url() {
        let result =
            parse_pr_number("http://github.com/owner/repo/pull/456").unwrap();
        assert_eq!(result, 456);
    }

    #[test]
    fn parse_pr_number_not_a_number_returns_err() {
        let result = parse_pr_number("not-a-number");
        assert!(result.is_err());
    }

    #[test]
    fn parse_pr_number_github_issues_url_returns_err() {
        let result =
            parse_pr_number("https://github.com/owner/repo/issues/10");
        assert!(result.is_err());
    }

    #[test]
    fn parse_pr_number_non_github_host_returns_err() {
        let result =
            parse_pr_number("https://gitlab.com/owner/repo/pull/10");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Only GitHub URLs are supported"),
            "expected GitHub-only message, got: {}",
            msg
        );
    }

    #[test]
    fn default_worktree_name_formats_correctly() {
        assert_eq!(default_worktree_name(42), "pr-42");
    }
}
