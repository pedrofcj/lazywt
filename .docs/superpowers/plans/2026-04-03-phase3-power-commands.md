# Phase 3: Power Commands — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three new commands — `pr` (fetch a GitHub PR into a worktree), `prune` (remove merged/orphan worktrees), and `sync` (fetch remotes + update branches).

**Architecture:** Each command is a new module under `src/commands/`. `pr` uses `gh` CLI with fallback to GitHub API via `ureq`. `prune` reuses `remove` logic and `git branch --merged` for detection. `sync` operates on git refs directly via `git fetch` and `git merge --ff-only`. A shared `src/git/github.rs` module handles PR metadata resolution.

**Tech Stack:** Rust, clap derive, ureq (already in Cargo.toml), `gh` CLI (optional runtime dependency).

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `src/cli.rs` | Add `Pr`, `Prune`, `Sync` subcommands |
| Create | `src/commands/pr.rs` | Fetch PR branch into new worktree |
| Create | `src/commands/prune.rs` | Remove merged + orphan worktrees |
| Create | `src/commands/sync.rs` | Fetch remotes, update branches |
| Create | `src/git/github.rs` | PR metadata resolution (gh + API fallback) |
| Modify | `src/git/mod.rs` | Register `github` module |
| Modify | `src/commands/mod.rs` | Register new command modules |
| Modify | `src/main.rs` | Wire up new commands |

---

### Task 1: Add CLI subcommands for pr, prune, sync

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Add Pr variant**

```rust
/// Fetch a GitHub PR into a new worktree for review
Pr {
    /// PR number or GitHub PR URL
    pr_ref: String,
    /// Custom worktree name (default: pr-<number>)
    name: Option<String>,
},
```

- [ ] **Step 2: Add Prune variant**

```rust
/// Remove worktrees with merged branches or missing directories
Prune {
    /// Skip confirmation prompt
    #[arg(long, short)]
    yes: bool,
    /// Also remove dirty worktrees (uncommitted changes)
    #[arg(long, short)]
    force: bool,
},
```

- [ ] **Step 3: Add Sync variant**

```rust
/// Fetch remotes and update branches
Sync {
    /// Also update all worktree branches (not just default)
    #[arg(long)]
    all: bool,
},
```

- [ ] **Step 4: Add CLI tests**

```rust
#[test]
fn parse_pr_number() {
    let cli = Cli::parse_from(["junktree", "pr", "272"]);
    assert_eq!(cli.command, Commands::Pr { pr_ref: "272".to_string(), name: None });
}

#[test]
fn parse_pr_with_name() {
    let cli = Cli::parse_from(["junktree", "pr", "272", "review-mess"]);
    assert_eq!(cli.command, Commands::Pr { pr_ref: "272".to_string(), name: Some("review-mess".to_string()) });
}

#[test]
fn parse_pr_url() {
    let cli = Cli::parse_from(["junktree", "pr", "https://github.com/owner/repo/pull/42"]);
    assert_eq!(cli.command, Commands::Pr { pr_ref: "https://github.com/owner/repo/pull/42".to_string(), name: None });
}

#[test]
fn parse_prune_defaults() {
    let cli = Cli::parse_from(["junktree", "prune"]);
    assert_eq!(cli.command, Commands::Prune { yes: false, force: false });
}

#[test]
fn parse_prune_yes() {
    let cli = Cli::parse_from(["junktree", "prune", "--yes"]);
    assert_eq!(cli.command, Commands::Prune { yes: true, force: false });
}

#[test]
fn parse_prune_force() {
    let cli = Cli::parse_from(["junktree", "prune", "--force"]);
    assert_eq!(cli.command, Commands::Prune { yes: false, force: true });
}

#[test]
fn parse_sync_defaults() {
    let cli = Cli::parse_from(["junktree", "sync"]);
    assert_eq!(cli.command, Commands::Sync { all: false });
}

#[test]
fn parse_sync_all() {
    let cli = Cli::parse_from(["junktree", "sync", "--all"]);
    assert_eq!(cli.command, Commands::Sync { all: true });
}
```

- [ ] **Step 5: Run CLI tests**

Run: `cargo test --lib cli::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add pr, prune, sync CLI subcommands"
```

---

### Task 2: Create GitHub PR metadata resolver

**Files:**
- Create: `src/git/github.rs`
- Modify: `src/git/mod.rs`

- [ ] **Step 1: Write test for PR number extraction from URL**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pr_number_from_plain_number() {
        assert_eq!(parse_pr_ref("272"), PrRef { number: 272 });
    }

    #[test]
    fn extract_pr_number_from_url() {
        assert_eq!(
            parse_pr_ref("https://github.com/owner/repo/pull/42"),
            PrRef { number: 42 }
        );
    }

    #[test]
    fn extract_pr_number_from_url_trailing_slash() {
        assert_eq!(
            parse_pr_ref("https://github.com/owner/repo/pull/42/"),
            PrRef { number: 42 }
        );
    }

    #[test]
    fn extract_origin_owner_repo_https() {
        let result = parse_remote_url("https://github.com/owner/repo.git");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn extract_origin_owner_repo_ssh() {
        let result = parse_remote_url("git@github.com:owner/repo.git");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }

    #[test]
    fn extract_origin_owner_repo_no_git_suffix() {
        let result = parse_remote_url("https://github.com/owner/repo");
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib git::github::tests -- --nocapture`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement github.rs**

```rust
use std::path::Path;
use anyhow::{Result, bail};

#[derive(Debug, PartialEq)]
pub struct PrRef {
    pub number: u64,
}

/// Parse a PR reference — either a plain number or a GitHub URL.
pub fn parse_pr_ref(input: &str) -> PrRef {
    // Try to extract number from URL pattern: .../pull/<number>
    if input.contains("/pull/") {
        if let Some(num_str) = input.split("/pull/").nth(1) {
            let num_str = num_str.trim_end_matches('/');
            if let Ok(n) = num_str.parse::<u64>() {
                return PrRef { number: n };
            }
        }
    }
    // Try plain number
    PrRef {
        number: input.parse::<u64>().unwrap_or(0),
    }
}

/// Parse a git remote URL to extract owner and repo name.
pub fn parse_remote_url(url: &str) -> Option<(String, String)> {
    // HTTPS: https://github.com/owner/repo.git
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.trim_end_matches('/').splitn(3, '/').collect();
        if parts.len() >= 2 {
            let repo = parts[1].trim_end_matches(".git");
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }
    // SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = rest.trim_end_matches('/').splitn(3, '/').collect();
        if parts.len() >= 2 {
            let repo = parts[1].trim_end_matches(".git");
            return Some((parts[0].to_string(), repo.to_string()));
        }
    }
    None
}

/// Resolve a PR number to its head branch name.
/// Strategy 1: Use `gh` CLI if available.
/// Strategy 2: Fall back to GitHub API via `ureq`.
pub fn resolve_pr_branch(project_dir: &Path, pr_number: u64) -> Result<String> {
    // Strategy 1: gh CLI
    match try_gh(project_dir, pr_number) {
        Ok(branch) => return Ok(branch),
        Err(_) => {} // fall through to API
    }

    // Strategy 2: GitHub API
    try_github_api(project_dir, pr_number)
}

fn try_gh(project_dir: &Path, pr_number: u64) -> Result<String> {
    let output = std::process::Command::new("gh")
        .arg("pr")
        .arg("view")
        .arg(pr_number.to_string())
        .arg("--json")
        .arg("headRefName")
        .arg("--jq")
        .arg(".headRefName")
        .current_dir(project_dir)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if branch.is_empty() {
                bail!("gh returned empty branch name");
            }
            Ok(branch)
        }
        _ => bail!("gh CLI not available or failed"),
    }
}

fn try_github_api(project_dir: &Path, pr_number: u64) -> Result<String> {
    // Get remote URL
    let remote_url = super::git(project_dir, &["remote", "get-url", "origin"])?;
    let (owner, repo) = parse_remote_url(remote_url.trim())
        .ok_or_else(|| anyhow::anyhow!("Could not parse GitHub owner/repo from origin URL"))?;

    let url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}",
        owner, repo, pr_number
    );

    let response = ureq::get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "junktree")
        .call()?;

    let body = response.into_body().read_to_string()?;

    // Extract head.ref from JSON response
    // Parse with serde_json since it's already a dependency
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let branch = json["head"]["ref"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Could not extract branch name from GitHub API response"))?;

    Ok(branch.to_string())
}
```

- [ ] **Step 4: Register module in git/mod.rs**

Add to `src/git/mod.rs`:

```rust
pub mod github;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib git::github::tests -- --nocapture`
Expected: PASS for parsing tests. Network tests are not run.

- [ ] **Step 6: Commit**

```bash
git add src/git/github.rs src/git/mod.rs
git commit -m "feat: add GitHub PR metadata resolver (gh + API fallback)"
```

---

### Task 3: Implement pr command

**Files:**
- Create: `src/commands/pr.rs`
- Modify: `src/commands/mod.rs`

- [ ] **Step 1: Implement pr.rs**

```rust
use anyhow::{Result, bail};
use crate::config::Config;
use crate::git;
use crate::git::github;
use crate::layout::ProjectContext;
use crate::output;

pub fn run(ctx: &ProjectContext, _config: &Config, pr_ref: &str, name: Option<&str>) -> Result<()> {
    // 1. Parse PR reference
    let pr = github::parse_pr_ref(pr_ref);
    if pr.number == 0 {
        bail!("Invalid PR reference: '{}'. Provide a PR number or GitHub URL.", pr_ref);
    }

    // 2. Determine worktree name
    let wt_name = match name {
        Some(n) => n.to_string(),
        None => format!("pr-{}", pr.number),
    };

    // 3. Check worktree doesn't already exist
    let worktree_path = ctx.worktree_parent.join(&wt_name);
    if worktree_path.exists() {
        bail!("Worktree '{}' already exists at {}", wt_name, worktree_path.display());
    }

    // 4. Resolve PR branch name
    output::info(&format!("Resolving PR #{} branch...", pr.number));
    let branch_name = github::resolve_pr_branch(&ctx.project_dir, pr.number)?;

    output::header(&format!("Fetching PR #{} into worktree '{}'", pr.number, wt_name));
    output::info(&format!("  Branch: {}", branch_name));

    // 5. Fetch the branch from remote
    git::git(&ctx.project_dir, &["fetch", "origin", &branch_name])?;

    // 6. Create worktree tracking the remote branch
    std::fs::create_dir_all(&ctx.worktree_parent)?;
    let wt_path_str = worktree_path.to_str().unwrap();
    git::git(
        &ctx.project_dir,
        &["worktree", "add", "--track", "-b", &branch_name, wt_path_str, &format!("origin/{}", branch_name)],
    ).or_else(|_| {
        // Branch might already exist locally — try checking it out instead
        git::git(&ctx.project_dir, &["worktree", "add", wt_path_str, &branch_name])
    })?;

    // 7. Set upstream tracking
    let _ = git::branch::set_upstream(&worktree_path, &branch_name);

    // 8. Success
    output::success(&format!(
        "PR #{} checked out in worktree '{}' at {}",
        pr.number, wt_name, worktree_path.display()
    ));

    // 9. Prompt navigation
    crate::navigate::maybe_navigate(&worktree_path, _config);

    Ok(())
}
```

- [ ] **Step 2: Register in mod.rs**

Add to `src/commands/mod.rs`:

```rust
pub mod pr;
```

- [ ] **Step 3: Compile check**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add src/commands/pr.rs src/commands/mod.rs
git commit -m "feat: add pr command — fetch GitHub PR into worktree"
```

---

### Task 4: Implement prune command

**Files:**
- Create: `src/commands/prune.rs`
- Modify: `src/commands/mod.rs`

- [ ] **Step 1: Write test for merged branch detection helper**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_merged_branch() {
        // If branch is in the merged set, it should be classified as Merged
        let merged: std::collections::HashSet<String> = ["feature/done"].iter().map(|s| s.to_string()).collect();
        let wt = crate::git::worktree::Worktree {
            path: std::path::PathBuf::from("/test/done"),
            head: Some("abc".to_string()),
            branch: Some("feature/done".to_string()),
            is_bare: false,
            is_detached: false,
            is_locked: false,
            lock_reason: None,
            is_prunable: false,
        };
        let reason = classify_worktree(&wt, &merged, false);
        assert_eq!(reason, Some(PruneReason::Merged));
    }

    #[test]
    fn classify_orphan() {
        let merged = std::collections::HashSet::new();
        let wt = crate::git::worktree::Worktree {
            path: std::path::PathBuf::from("/test/gone"),
            head: Some("abc".to_string()),
            branch: Some("feature/gone".to_string()),
            is_bare: false,
            is_detached: false,
            is_locked: false,
            lock_reason: None,
            is_prunable: true,
        };
        let reason = classify_worktree(&wt, &merged, false);
        assert_eq!(reason, Some(PruneReason::Orphan));
    }

    #[test]
    fn classify_normal_not_prunable() {
        let merged = std::collections::HashSet::new();
        let wt = crate::git::worktree::Worktree {
            path: std::path::PathBuf::from("/test/active"),
            head: Some("abc".to_string()),
            branch: Some("feature/active".to_string()),
            is_bare: false,
            is_detached: false,
            is_locked: false,
            lock_reason: None,
            is_prunable: false,
        };
        let reason = classify_worktree(&wt, &merged, false);
        assert_eq!(reason, None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib commands::prune::tests -- --nocapture`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement prune.rs**

```rust
use std::collections::HashSet;

use anyhow::Result;
use crate::config::Config;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;
use crate::prompt;

#[derive(Debug, PartialEq)]
enum PruneReason {
    Merged,
    Orphan,
}

/// Classify a worktree for pruning.
fn classify_worktree(
    wt: &git::worktree::Worktree,
    merged_branches: &HashSet<String>,
    _force: bool,
) -> Option<PruneReason> {
    if wt.is_bare {
        return None;
    }
    if wt.is_prunable {
        return Some(PruneReason::Orphan);
    }
    if let Some(ref branch) = wt.branch {
        if merged_branches.contains(branch) {
            return Some(PruneReason::Merged);
        }
    }
    None
}

/// Check if a worktree has uncommitted changes.
fn is_dirty(wt_path: &std::path::Path) -> bool {
    git::git(wt_path, &["status", "--porcelain"])
        .map(|output| !output.trim().is_empty())
        .unwrap_or(false)
}

pub fn run(ctx: &ProjectContext, config: &Config, yes: bool, force: bool) -> Result<()> {
    // 1. Detect default branch
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?
        .ok_or_else(|| anyhow::anyhow!("Could not detect default branch"))?;

    // 2. Get merged branches
    let merged_output = git::git(&ctx.project_dir, &["branch", "--merged", &default_branch])
        .unwrap_or_default();
    let merged_branches: HashSet<String> = merged_output
        .lines()
        .map(|line| line.trim().trim_start_matches("* ").to_string())
        .filter(|b| b != &default_branch)
        .collect();

    // 3. List and classify worktrees
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
    let mut to_prune: Vec<(&git::worktree::Worktree, PruneReason)> = Vec::new();
    let mut dirty_skipped: Vec<String> = Vec::new();

    for wt in &worktrees {
        if let Some(reason) = classify_worktree(wt, &merged_branches, force) {
            // Protect default branch
            if wt.branch.as_deref() == Some(default_branch.as_str()) {
                continue;
            }

            let wt_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");

            // Check dirty status (skip for orphans — no directory to check)
            if reason == PruneReason::Merged && !force && is_dirty(&wt.path) {
                dirty_skipped.push(wt_name.to_string());
                continue;
            }

            to_prune.push((wt, reason));
        }
    }

    // 4. Report
    if to_prune.is_empty() && dirty_skipped.is_empty() {
        output::info("Nothing to prune — all worktrees are active.");
        return Ok(());
    }

    output::header("Prune summary");
    for (wt, reason) in &to_prune {
        let wt_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        let label = match reason {
            PruneReason::Merged => "merged",
            PruneReason::Orphan => "orphan",
        };
        output::info(&format!("  {} ({})", wt_name, label));
    }

    if !dirty_skipped.is_empty() {
        output::warning(&format!(
            "  Skipping dirty worktrees (use --force to include): {}",
            dirty_skipped.join(", ")
        ));
    }

    if to_prune.is_empty() {
        return Ok(());
    }

    // 5. Confirm
    if !prompt::confirm(&format!("Remove {} worktree(s)?", to_prune.len()), yes)? {
        output::info("Cancelled.");
        return Ok(());
    }

    // 6. Remove each
    let mut removed = 0;
    for (wt, reason) in &to_prune {
        let wt_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");

        if *reason == PruneReason::Orphan {
            // For orphans, just prune the git worktree record
            if git::git(&ctx.project_dir, &["worktree", "prune"]).is_ok() {
                output::success(&format!("  Pruned orphan '{}'", wt_name));
                removed += 1;
            }
        } else {
            // For merged, remove worktree + branch
            let wt_path_str = wt.path.to_str().unwrap_or_default();
            if git::git(&ctx.project_dir, &["worktree", "remove", wt_path_str, "--force"]).is_ok() {
                if let Some(ref branch) = wt.branch {
                    let _ = git::git(&ctx.project_dir, &["branch", "-D", branch]);
                }
                output::success(&format!("  Removed '{}' (merged)", wt_name));
                removed += 1;
            } else {
                output::warning(&format!("  Failed to remove '{}'", wt_name));
            }
        }
    }

    output::success(&format!("Pruned {} worktree(s)", removed));
    Ok(())
}
```

- [ ] **Step 4: Register in mod.rs**

Add to `src/commands/mod.rs`:

```rust
pub mod prune;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib commands::prune::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/commands/prune.rs src/commands/mod.rs
git commit -m "feat: add prune command — remove merged and orphan worktrees"
```

---

### Task 5: Implement sync command

**Files:**
- Create: `src/commands/sync.rs`
- Modify: `src/commands/mod.rs`

- [ ] **Step 1: Implement sync.rs**

```rust
use anyhow::Result;
use crate::config::Config;
use crate::git;
use crate::layout::ProjectContext;
use crate::output;

pub fn run(ctx: &ProjectContext, _config: &Config, all: bool) -> Result<()> {
    // 1. Fetch all remotes
    output::progress_start("Fetching from all remotes");
    match git::git(&ctx.project_dir, &["fetch", "--all", "--prune"]) {
        Ok(_) => output::progress_complete("Fetched", output::Status::Success),
        Err(e) => {
            output::progress_complete("Fetch failed", output::Status::Error);
            return Err(e);
        }
    }

    // 2. Detect default branch
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?
        .ok_or_else(|| anyhow::anyhow!("Could not detect default branch"))?;

    // 3. Fast-forward default branch
    output::progress_start(&format!("Updating '{}'", default_branch));
    // Find the default branch worktree
    let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
    let default_wt = worktrees.iter().find(|wt| {
        !wt.is_bare && wt.branch.as_deref() == Some(default_branch.as_str())
    });

    if let Some(wt) = default_wt {
        match git::git(&wt.path, &["merge", "--ff-only", &format!("origin/{}", default_branch)]) {
            Ok(_) => output::progress_complete("Updated", output::Status::Success),
            Err(_) => output::progress_complete("Already up to date or diverged", output::Status::Warning),
        }
    } else {
        // Default branch not checked out — update ref directly
        let remote_ref = format!("refs/remotes/origin/{}", default_branch);
        let local_ref = format!("refs/heads/{}", default_branch);
        match git::git(&ctx.project_dir, &["update-ref", &local_ref, &remote_ref]) {
            Ok(_) => output::progress_complete("Updated ref", output::Status::Success),
            Err(_) => output::progress_complete("Could not update", output::Status::Warning),
        }
    }

    // 4. Check for worktrees tracking deleted remote branches
    let remote_branches_output = git::git(&ctx.project_dir, &["branch", "-r"])
        .unwrap_or_default();
    let remote_branches: std::collections::HashSet<String> = remote_branches_output
        .lines()
        .map(|l| l.trim().to_string())
        .collect();

    let mut warnings = Vec::new();
    let mut behind_info = Vec::new();

    for wt in &worktrees {
        if wt.is_bare {
            continue;
        }
        let wt_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        if let Some(ref branch) = wt.branch {
            if branch == &default_branch {
                continue;
            }
            let remote_ref = format!("origin/{}", branch);
            if !remote_branches.contains(&remote_ref) {
                warnings.push(format!("  '{}' tracks deleted remote branch '{}'", wt_name, branch));
            } else {
                // Check if behind
                let behind = git::git(
                    &ctx.project_dir,
                    &["rev-list", "--count", &format!("{}..{}", branch, remote_ref)],
                ).ok().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
                if behind > 0 {
                    behind_info.push(format!("  '{}' is {} commit(s) behind origin", wt_name, behind));
                }
            }
        }
    }

    // 5. Report warnings
    if !warnings.is_empty() {
        println!();
        output::warning("Worktrees tracking deleted remote branches:");
        for w in &warnings {
            output::warning(w);
        }
    }

    if !behind_info.is_empty() {
        println!();
        output::info("Worktrees behind remote:");
        for b in &behind_info {
            output::info(b);
        }
    }

    // 6. --all: update all worktree branches
    if all {
        println!();
        output::header("Updating all worktree branches");
        for wt in &worktrees {
            if wt.is_bare || wt.is_prunable {
                continue;
            }
            let wt_name = wt.path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
            if let Some(ref branch) = wt.branch {
                if branch == &default_branch {
                    continue; // already updated
                }

                // Check dirty
                let status_output = git::git(&wt.path, &["status", "--porcelain"]).unwrap_or_default();
                if !status_output.trim().is_empty() {
                    output::warning(&format!("  Skipping '{}' — dirty working tree", wt_name));
                    continue;
                }

                // Try fast-forward
                match git::git(&wt.path, &["merge", "--ff-only", &format!("origin/{}", branch)]) {
                    Ok(_) => output::success(&format!("  Updated '{}'", wt_name)),
                    Err(_) => output::warning(&format!("  Skipping '{}' — cannot fast-forward", wt_name)),
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Register in mod.rs**

Add to `src/commands/mod.rs`:

```rust
pub mod sync;
```

- [ ] **Step 3: Compile check**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add src/commands/sync.rs src/commands/mod.rs
git commit -m "feat: add sync command — fetch remotes, update branches"
```

---

### Task 6: Wire new commands in main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add match arms for new commands**

In the `match cli.command` block after the bare repo resolution:

```rust
Commands::Pr { pr_ref, name } => {
    commands::pr::run(&ctx, &config, &pr_ref, name.as_deref())
}
Commands::Prune { yes, force } => {
    commands::prune::run(&ctx, &config, yes, force)
}
Commands::Sync { all } => {
    commands::sync::run(&ctx, &config, all)
}
```

Add `Commands::Pr { .. } | Commands::Prune { .. } | Commands::Sync { .. }` to the `unreachable!()` arm.

- [ ] **Step 2: Compile and test**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire pr, prune, sync commands into main dispatch"
```

---

### Task 7: Full integration check

**Files:**
- All modified files

- [ ] **Step 1: Full build**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 2: Full test suite**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 3: Commit if any fixes needed**

```bash
git add -A
git commit -m "fix: resolve phase 3 integration issues"
```
