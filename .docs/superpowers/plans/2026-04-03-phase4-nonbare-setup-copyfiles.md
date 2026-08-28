# Phase 4: Non-bare Support + Setup + Copy-files — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend junktree to work with non-bare repositories (worktrees in `.wts/` or sibling folder), add a `setup` command to convert regular repos to bare, and implement copy-files on worktree creation/removal.

**Architecture:** `layout.rs` gains a third layout variant `NonBare`. `ProjectContext` adapts path computations based on the layout. The `setup` command manipulates `.git` directory structure. Copy-files hooks into `add` and `remove` via glob matching from per-repo config.

**Tech Stack:** Rust, glob (new dependency for pattern matching), serde_json (already present).

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `src/layout.rs` | Add `NonBare` layout variant, detect non-bare repos |
| Modify | `src/git/repo.rs` | Find repo root for both bare and non-bare repos |
| Create | `src/commands/setup.rs` | Convert regular repo to bare |
| Create | `src/copy_files.rs` | Glob-based file copy and diff for worktree lifecycle |
| Modify | `src/commands/add.rs` | Call copy_files after worktree creation |
| Modify | `src/commands/remove.rs` | Warn about changed copy_files before removal |
| Modify | `src/cli.rs` | Add `Setup` subcommand |
| Modify | `src/commands/mod.rs` | Register new modules |
| Modify | `src/main.rs` | Wire setup, update repo detection flow |
| Modify | `Cargo.toml` | Add `glob` dependency |

---

### Task 1: Add glob dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add glob to dependencies**

```toml
glob = "0.3"
```

- [ ] **Step 2: Verify build**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add glob crate for copy-files pattern matching"
```

---

### Task 2: Extend layout.rs with NonBare variant

**Files:**
- Modify: `src/layout.rs`

- [ ] **Step 1: Write tests for non-bare layout**

```rust
#[test]
fn nonbare_inside_layout_worktree_parent() {
    let project_dir = PathBuf::from("/home/user/myproject/.git");
    let mut config = Config::default();
    config.worktree_location = "inside".to_string();
    let ctx = build_context(&project_dir, LayoutType::NonBare, &config);
    assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject/.wts"));
    assert_eq!(ctx.project_root, PathBuf::from("/home/user/myproject"));
}

#[test]
fn nonbare_sibling_layout_worktree_parent() {
    let project_dir = PathBuf::from("/home/user/myproject/.git");
    let mut config = Config::default();
    config.worktree_location = "sibling".to_string();
    let ctx = build_context(&project_dir, LayoutType::NonBare, &config);
    assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject-wts"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib layout::tests -- --nocapture`
Expected: FAIL — `NonBare` variant doesn't exist.

- [ ] **Step 3: Add NonBare variant and update build_context**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutType {
    Classic, // .bare file + trees/ subfolder
    Modern,  // hidden .git/ bare repo + root-level worktrees
    NonBare, // regular .git/ repo + worktrees in .wts/ or sibling
}
```

Update `build_context`:

```rust
pub fn build_context(project_dir: &Path, layout: LayoutType, config: &Config) -> ProjectContext {
    let project_root = match layout {
        LayoutType::Modern | LayoutType::NonBare => project_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| project_dir.to_path_buf()),
        LayoutType::Classic => project_dir.to_path_buf(),
    };

    let project_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let worktree_folder = match layout {
        LayoutType::Classic => "trees".to_string(),
        LayoutType::Modern => String::new(),
        LayoutType::NonBare => {
            if config.worktree_location == "sibling" {
                String::new() // handled specially below
            } else {
                ".wts".to_string()
            }
        }
    };

    let worktree_parent = match layout {
        LayoutType::NonBare if config.worktree_location == "sibling" => {
            let parent = project_root.parent().unwrap_or(&project_root);
            parent.join(format!("{}-wts", project_name))
        }
        _ => {
            if worktree_folder.is_empty() {
                project_root.clone()
            } else {
                project_root.join(&worktree_folder)
            }
        }
    };

    ProjectContext {
        project_dir: project_dir.to_path_buf(),
        project_root,
        layout_type: layout,
        project_name,
        worktree_folder,
        worktree_parent,
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib layout::tests -- --nocapture`
Expected: All PASS.

- [ ] **Step 5: Commit**

```bash
git add src/layout.rs
git commit -m "feat: add NonBare layout variant for regular repo worktree support"
```

---

### Task 3: Update repo detection to support non-bare repos

**Files:**
- Modify: `src/git/repo.rs`

- [ ] **Step 1: Rename and update find_bare_repo_root**

Rename to `find_repo_root` and make it return `(PathBuf, bool)` — the path and whether it's bare:

```rust
use std::path::PathBuf;
use anyhow::Result;

/// Repository root info.
pub struct RepoRoot {
    pub git_dir: PathBuf,
    pub is_bare: bool,
}

/// Find the repository root from the current directory.
/// Works from inside a bare repo, a worktree, or a regular repo.
pub fn find_repo_root() -> Result<RepoRoot> {
    // Step 1: Get git-dir from CWD
    let git_dir = super::git_in_cwd(&["rev-parse", "--git-dir"])?;
    let git_dir = dunce::canonicalize(PathBuf::from(git_dir.trim()))
        .unwrap_or_else(|_| PathBuf::from(git_dir.trim()));

    // Step 2: Check if directly in bare repo
    let is_bare = super::git_in_cwd(&["rev-parse", "--is-bare-repository"])?;
    if is_bare.trim() == "true" {
        return Ok(RepoRoot { git_dir, is_bare: true });
    }

    // Step 3: Check if in worktree of bare repo via --git-common-dir
    let common_dir = super::git_in_cwd(&["rev-parse", "--git-common-dir"])?;
    let common_dir = dunce::canonicalize(PathBuf::from(common_dir.trim()))
        .unwrap_or_else(|_| PathBuf::from(common_dir.trim()));
    let is_bare_common = super::git(&common_dir, &["rev-parse", "--is-bare-repository"])?;
    if is_bare_common.trim() == "true" {
        return Ok(RepoRoot { git_dir: common_dir, is_bare: true });
    }

    // Step 4: Regular (non-bare) repo
    Ok(RepoRoot { git_dir, is_bare: false })
}

/// Backward-compatible wrapper: find bare repo root only, fail if not bare.
pub fn find_bare_repo_root() -> Result<PathBuf> {
    let root = find_repo_root()?;
    if !root.is_bare {
        anyhow::bail!(
            "This command requires a bare Git repository.\n   \
             Please run from a bare repository directory or one of its worktrees."
        );
    }
    Ok(root.git_dir)
}
```

- [ ] **Step 2: Update main.rs to use new detection**

In `main.rs`, update the repo-dependent section to handle both bare and non-bare:

```rust
// Resolve repo root (bare or non-bare)
let repo = git::repo::find_repo_root()?;
let config = Config::load(Some(&repo.git_dir));

let layout = if repo.is_bare {
    layout::detect_layout(&repo.git_dir, &config)?
} else {
    layout::build_context(&repo.git_dir, layout::LayoutType::NonBare, &config)
};
```

- [ ] **Step 3: Run tests**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 4: Commit**

```bash
git add src/git/repo.rs src/main.rs
git commit -m "feat: support non-bare repo detection alongside bare repos"
```

---

### Task 4: Auto-add .wts to .gitignore

**Files:**
- Modify: `src/commands/add.rs`

- [ ] **Step 1: Write helper function**

```rust
/// Ensure `.wts` is in the repo's .gitignore (for non-bare inside mode).
fn ensure_gitignore_entry(project_root: &std::path::Path, entry: &str) -> Result<()> {
    let gitignore = project_root.join(".gitignore");
    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore)?;
        if content.lines().any(|line| line.trim() == entry) {
            return Ok(()); // already present
        }
    }
    // Append
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore)?;
    writeln!(file, "{}", entry)?;
    Ok(())
}
```

- [ ] **Step 2: Call it in add command for NonBare layout**

After worktree parent directory creation (step 8 in `add::run`), add:

```rust
// For non-bare inside layout, ensure .wts is gitignored
if ctx.layout_type == crate::layout::LayoutType::NonBare
    && ctx.worktree_folder == ".wts"
{
    ensure_gitignore_entry(&ctx.project_root, ".wts")?;
}
```

- [ ] **Step 3: Write test for gitignore helper**

```rust
#[test]
fn ensure_gitignore_adds_entry() {
    let dir = tempfile::tempdir().unwrap();
    ensure_gitignore_entry(dir.path(), ".wts").unwrap();
    let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(content.contains(".wts"));
}

#[test]
fn ensure_gitignore_does_not_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".gitignore"), ".wts\n").unwrap();
    ensure_gitignore_entry(dir.path(), ".wts").unwrap();
    let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert_eq!(content.matches(".wts").count(), 1);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib commands::add::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/commands/add.rs
git commit -m "feat: auto-add .wts to .gitignore for non-bare inside layout"
```

---

### Task 5: Implement copy_files module

**Files:**
- Create: `src/copy_files.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_source(dir: &TempDir) {
        std::fs::write(dir.path().join(".env"), "SECRET=abc").unwrap();
        std::fs::write(dir.path().join(".env.local"), "LOCAL=true").unwrap();
        std::fs::write(dir.path().join("README.md"), "# readme").unwrap();
    }

    #[test]
    fn copy_matching_files_env_pattern() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        setup_source(&source);

        let patterns = vec![".env*".to_string()];
        let copied = copy_matching_files(source.path(), dest.path(), &patterns).unwrap();
        assert_eq!(copied, 2);
        assert!(dest.path().join(".env").exists());
        assert!(dest.path().join(".env.local").exists());
        assert!(!dest.path().join("README.md").exists());
    }

    #[test]
    fn copy_no_patterns_copies_nothing() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        setup_source(&source);

        let patterns: Vec<String> = Vec::new();
        let copied = copy_matching_files(source.path(), dest.path(), &patterns).unwrap();
        assert_eq!(copied, 0);
    }

    #[test]
    fn diff_matching_files_detects_changes() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(source.path().join(".env"), "SECRET=abc").unwrap();
        std::fs::write(dest.path().join(".env"), "SECRET=xyz").unwrap();

        let patterns = vec![".env*".to_string()];
        let changed = diff_matching_files(source.path(), dest.path(), &patterns).unwrap();
        assert_eq!(changed, vec![".env"]);
    }

    #[test]
    fn diff_matching_files_no_changes() {
        let source = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();
        std::fs::write(source.path().join(".env"), "SECRET=abc").unwrap();
        std::fs::write(dest.path().join(".env"), "SECRET=abc").unwrap();

        let patterns = vec![".env*".to_string()];
        let changed = diff_matching_files(source.path(), dest.path(), &patterns).unwrap();
        assert!(changed.is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib copy_files::tests -- --nocapture`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement copy_files.rs**

```rust
use std::path::Path;
use anyhow::Result;

/// Copy files matching glob patterns from source to dest.
/// Returns the number of files copied.
pub fn copy_matching_files(source: &Path, dest: &Path, patterns: &[String]) -> Result<usize> {
    if patterns.is_empty() {
        return Ok(0);
    }

    let mut count = 0;
    for pattern in patterns {
        let full_pattern = source.join(pattern).display().to_string();
        for entry in glob::glob(&full_pattern)? {
            let entry = entry?;
            if entry.is_file() {
                let relative = entry.strip_prefix(source)?;
                let dest_path = dest.join(relative);
                if let Some(parent) = dest_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&entry, &dest_path)?;
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Check which files matching patterns differ between source and dest.
/// Returns a list of relative paths that are different.
pub fn diff_matching_files(source: &Path, dest: &Path, patterns: &[String]) -> Result<Vec<String>> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }

    let mut changed = Vec::new();
    for pattern in patterns {
        let full_pattern = dest.join(pattern).display().to_string();
        for entry in glob::glob(&full_pattern)? {
            let entry = entry?;
            if entry.is_file() {
                let relative = entry.strip_prefix(dest)?;
                let source_file = source.join(relative);

                if !source_file.exists() {
                    // File exists in dest but not source — it's new
                    changed.push(relative.display().to_string());
                } else {
                    // Compare contents
                    let dest_content = std::fs::read(&entry)?;
                    let source_content = std::fs::read(&source_file)?;
                    if dest_content != source_content {
                        changed.push(relative.display().to_string());
                    }
                }
            }
        }
    }

    Ok(changed)
}
```

- [ ] **Step 4: Register in lib.rs**

Add to `src/lib.rs`:

```rust
pub mod copy_files;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib copy_files::tests -- --nocapture`
Expected: All PASS.

- [ ] **Step 6: Commit**

```bash
git add src/copy_files.rs src/lib.rs
git commit -m "feat: add copy_files module for glob-based file copy and diff"
```

---

### Task 6: Hook copy_files into add command

**Files:**
- Modify: `src/commands/add.rs`

- [ ] **Step 1: After worktree creation, copy files**

After the successful worktree creation (step 10 in add::run) and before the success message:

```rust
// Copy files from default worktree if configured
if !config.copy_files.is_empty() {
    // Find the default worktree path
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?;
    if let Some(ref db) = default_branch {
        let worktrees = git::worktree::list_worktrees(&ctx.project_dir)?;
        let default_wt = worktrees.iter().find(|wt| {
            !wt.is_bare && wt.branch.as_deref() == Some(db.as_str())
        });
        if let Some(source_wt) = default_wt {
            let copied = crate::copy_files::copy_matching_files(
                &source_wt.path,
                &worktree_path,
                &config.copy_files,
            )?;
            if copied > 0 {
                output::info(&format!("  Copied {} file(s) from default worktree", copied));
            }
        }
    }
}
```

- [ ] **Step 2: Compile check**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/commands/add.rs
git commit -m "feat: copy configured files into new worktrees on add"
```

---

### Task 7: Hook copy_files diff into remove command

**Files:**
- Modify: `src/commands/remove.rs`

- [ ] **Step 1: Before removal, check for changed copy-files**

After confirmation prompt and before the actual removal:

```rust
// Check for changed copy-files (warn user)
if !config.copy_files.is_empty() && !yes {
    let default_branch = git::branch::detect_default_branch(&ctx.project_dir, false)?;
    if let Some(ref db) = default_branch {
        let worktrees_list = git::worktree::list_worktrees(&ctx.project_dir)?;
        let default_wt = worktrees_list.iter().find(|wt| {
            !wt.is_bare && wt.branch.as_deref() == Some(db.as_str())
        });
        if let Some(source_wt) = default_wt {
            let changed = crate::copy_files::diff_matching_files(
                &source_wt.path,
                &target.path,
                &config.copy_files,
            )?;
            if !changed.is_empty() {
                output::warning("  The following tracked files differ from the default worktree:");
                for f in &changed {
                    output::warning(&format!("    {}", f));
                }
                if !prompt::confirm("  Proceed with removal?", false)? {
                    output::info("Cancelled.");
                    return Ok(());
                }
            }
        }
    }
}
```

Update the `run` function signature to accept `config: &Config` (it already does via `_config` — just rename to use it).

- [ ] **Step 2: Compile check**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/commands/remove.rs
git commit -m "feat: warn about changed copy-files on worktree removal"
```

---

### Task 8: Implement setup command

**Files:**
- Create: `src/commands/setup.rs`
- Modify: `src/cli.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add Setup CLI variant**

```rust
/// Convert a regular repo into a bare repo with worktree structure
Setup {
    /// Skip confirmation prompt
    #[arg(long, short)]
    yes: bool,
    /// Show what would happen without making changes
    #[arg(long)]
    dry_run: bool,
},
```

- [ ] **Step 2: Implement setup.rs**

```rust
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use crate::config::Config;
use crate::git;
use crate::output;
use crate::prompt;

pub fn run(yes: bool, dry_run: bool) -> Result<()> {
    // 1. Verify we're in a regular (non-bare) repo
    let repo = git::repo::find_repo_root()?;
    if repo.is_bare {
        bail!("Already a bare repository — nothing to convert.");
    }

    let git_dir = &repo.git_dir; // .git directory
    let repo_root = git_dir.parent().ok_or_else(|| anyhow::anyhow!("Cannot determine repo root"))?;

    // 2. Detect current branch
    let current_branch = git::git_in_cwd(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    let current_branch = current_branch.trim();

    // 3. Plan
    let bare_dir = repo_root.join(".bare");
    let worktree_dir = repo_root.join(current_branch);

    output::header("Setup plan");
    output::info(&format!("  Repo root: {}", repo_root.display()));
    output::info(&format!("  Current branch: {}", current_branch));
    output::info(&format!("  Bare repo will be: {}", bare_dir.display()));
    output::info(&format!("  Default worktree: {}", worktree_dir.display()));
    output::info("  Steps:");
    output::info("    1. Move .git/ to .bare/");
    output::info("    2. Mark .bare/ as bare");
    output::info("    3. Create worktree for current branch");
    output::info("    4. Move working files into worktree");

    if dry_run {
        output::info("\n  (dry run — no changes made)");
        return Ok(());
    }

    // 4. Confirm
    if !prompt::confirm("Proceed with conversion?", yes)? {
        output::info("Cancelled.");
        return Ok(());
    }

    // 5. Move .git to .bare
    if bare_dir.exists() {
        bail!("'{}' already exists — cannot proceed", bare_dir.display());
    }
    fs::rename(git_dir, &bare_dir)?;

    // 6. Write .git pointer file
    fs::write(repo_root.join(".git"), format!("gitdir: .bare\n"))?;

    // 7. Mark as bare
    git::git(&bare_dir, &["config", "core.bare", "true"])?;

    // 8. Create worktree directory and move files
    fs::create_dir_all(&worktree_dir)?;

    // Move all files (except .git and .bare) into the worktree
    for entry in fs::read_dir(repo_root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_str().unwrap_or("");
        if name_str == ".git" || name_str == ".bare" || name_str == current_branch {
            continue;
        }
        let dest = worktree_dir.join(&name);
        fs::rename(entry.path(), dest)?;
    }

    // 9. Update .git pointer in worktree
    fs::write(worktree_dir.join(".git"), format!("gitdir: {}/.bare\n", repo_root.display()))?;

    // 10. Register worktree
    git::git(&bare_dir, &["worktree", "add", "--no-checkout", worktree_dir.to_str().unwrap(), current_branch]).ok();

    output::success("Repository converted to bare + worktree structure");
    output::info(&format!("  Bare repo: {}", bare_dir.display()));
    output::info(&format!("  Worktree '{}': {}", current_branch, worktree_dir.display()));

    Ok(())
}
```

- [ ] **Step 3: Register in mod.rs**

```rust
pub mod setup;
```

- [ ] **Step 4: Wire in main.rs**

Add to the non-repo commands section (before repo resolution):

```rust
Commands::Setup { yes, dry_run } => return commands::setup::run(*yes, *dry_run),
```

- [ ] **Step 5: Compile and test**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 6: Commit**

```bash
git add src/commands/setup.rs src/commands/mod.rs src/cli.rs src/main.rs
git commit -m "feat: add setup command — convert regular repo to bare"
```

---

### Task 9: Full integration test

**Files:**
- All modified files

- [ ] **Step 1: Full build**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 2: Full test suite**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 3: Fix any issues and commit**

```bash
git add -A
git commit -m "fix: resolve phase 4 integration issues"
```
