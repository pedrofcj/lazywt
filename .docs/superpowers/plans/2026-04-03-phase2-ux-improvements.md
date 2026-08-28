# Phase 2: UX Improvements — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve daily workflow UX — direct `switch` by name with fuzzy matching, `switch -` to go back, `init` auto-detect + `--add` to profile, and `list` enhancements showing orphan and merged worktree status.

**Architecture:** `switch` gains an optional positional arg and `-` literal. `init` reuses existing `alias.rs` shell detection but adds `--add` flag and `JUNKTREE_PREV` tracking in the emitted shell functions. `list` enriches the existing `collect::WorktreeInfo` with `merged` status via `git branch --merged`.

**Tech Stack:** Rust, clap derive, dialoguer, owo-colors. No new dependencies.

---

## File Structure

| Action | Path | Responsibility |
|--------|------|----------------|
| Modify | `src/cli.rs` | Add optional `name` arg to `Switch`, add `--add` flag to `Init`, unhide `Init` |
| Modify | `src/commands/switch.rs` | Direct switch by name, `switch -`, fuzzy matching |
| Modify | `src/commands/init_shell.rs` | Auto-detect shell, `--add` flag |
| Modify | `src/commands/alias.rs` | Update shell functions to track `JUNKTREE_PREV`, auto-detect without arg |
| Modify | `src/commands/list/collect.rs` | Add `merged` field to `WorktreeInfo` |
| Modify | `src/commands/list/display.rs` | Show `(orphan)` and `(merged)` labels |
| Modify | `src/commands/list/json.rs` | Add `orphan` and `merged` fields to JSON output |
| Modify | `src/commands/list/mod.rs` | Pass default branch to display layer |
| Modify | `src/main.rs` | Update Switch dispatch with new args |

---

### Task 1: Update Switch CLI to accept optional name

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Update Switch variant**

```rust
/// Interactively switch to a worktree (or by name)
Switch {
    /// Worktree name to switch to directly, or "-" for previous
    name: Option<String>,
},
```

- [ ] **Step 2: Add CLI tests**

```rust
#[test]
fn parse_switch_no_args() {
    let cli = Cli::parse_from(["junktree", "switch"]);
    assert_eq!(cli.command, Commands::Switch { name: None });
}

#[test]
fn parse_switch_with_name() {
    let cli = Cli::parse_from(["junktree", "switch", "auth"]);
    assert_eq!(cli.command, Commands::Switch { name: Some("auth".to_string()) });
}

#[test]
fn parse_switch_dash() {
    let cli = Cli::parse_from(["junktree", "switch", "-"]);
    assert_eq!(cli.command, Commands::Switch { name: Some("-".to_string()) });
}
```

- [ ] **Step 3: Run CLI tests**

Run: `cargo test --lib cli::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add optional name arg to switch command"
```

---

### Task 2: Implement direct switch, switch -, and fuzzy matching

**Files:**
- Modify: `src/commands/switch.rs`

- [ ] **Step 1: Write fuzzy match helper with test**

```rust
/// Find the closest worktree name using simple substring and edit distance matching.
/// Returns the best match if similarity is above threshold.
fn fuzzy_match<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    // Exact substring match first
    let mut best: Option<(&str, usize)> = None;
    for &candidate in candidates {
        if candidate.contains(input) || input.contains(candidate) {
            return Some(candidate);
        }
        // Simple character-overlap score
        let score = input.chars().filter(|c| candidate.contains(*c)).count();
        if score > input.len() / 2 {
            if best.is_none() || score > best.unwrap().1 {
                best = Some((candidate, score));
            }
        }
    }
    best.map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_match_exact_substring() {
        let candidates = vec!["auth-service", "payment", "dashboard"];
        assert_eq!(fuzzy_match("auth", &candidates), Some("auth-service"));
    }

    #[test]
    fn fuzzy_match_no_match() {
        let candidates = vec!["auth", "payment"];
        assert_eq!(fuzzy_match("xyz", &candidates), None);
    }

    #[test]
    fn fuzzy_match_close_match() {
        let candidates = vec!["authentication", "authorization"];
        assert_eq!(fuzzy_match("authn", &candidates), Some("authentication"));
    }
}
```

- [ ] **Step 2: Run test to verify**

Run: `cargo test --lib commands::switch::tests -- --nocapture`
Expected: PASS.

- [ ] **Step 3: Implement the updated run function**

```rust
pub fn run(ctx: &ProjectContext, _config: &Config, name: Option<&str>) -> Result<()> {
    let worktrees = list_worktrees(&ctx.project_dir)?;
    let non_bare: Vec<_> = worktrees.iter().filter(|w| !w.is_bare).collect();

    if non_bare.is_empty() {
        output::warning("No worktrees found");
        return Ok(());
    }

    match name {
        None => interactive_switch(&non_bare),
        Some("-") => switch_previous(&non_bare),
        Some(target) => direct_switch(&non_bare, target),
    }
}

fn switch_previous(worktrees: &[&crate::git::worktree::Worktree]) -> Result<()> {
    let prev = std::env::var("JUNKTREE_PREV").ok();
    match prev {
        Some(ref prev_name) if !prev_name.is_empty() => {
            direct_switch(worktrees, prev_name)
        }
        _ => {
            anyhow::bail!("No previous worktree — JUNKTREE_PREV is not set. Make sure you're using the shell wrapper from 'junktree init'.");
        }
    }
}

fn direct_switch(worktrees: &[&crate::git::worktree::Worktree], target: &str) -> Result<()> {
    // Try exact match first
    let found = worktrees.iter().find(|wt| {
        wt.path.file_name().and_then(|n| n.to_str()) == Some(target)
    });

    if let Some(wt) = found {
        navigate::request_cd(&wt.path);
        return Ok(());
    }

    // Fuzzy match
    let names: Vec<&str> = worktrees.iter()
        .filter_map(|wt| wt.path.file_name().and_then(|n| n.to_str()))
        .collect();

    if let Some(suggestion) = fuzzy_match(target, &names) {
        output::error(&format!("worktree '{}' not found", target));
        output::info(&format!("  Did you mean '{}'?", suggestion));
    } else {
        output::error(&format!("worktree '{}' not found", target));
    }

    std::process::exit(1);
}

fn interactive_switch(worktrees: &[&crate::git::worktree::Worktree]) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("Interactive selection requires a terminal");
    }

    // ... (move existing interactive logic here, unchanged)
}
```

Move the existing interactive `Select` logic from the current `run()` into `interactive_switch()`.

- [ ] **Step 4: Run tests**

Run: `cargo test --lib commands::switch -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/commands/switch.rs
git commit -m "feat: switch by name, switch -, fuzzy matching"
```

---

### Task 3: Update main.rs switch dispatch

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update the Switch match arm**

```rust
Commands::Switch { name } => commands::switch::run(&ctx, &config, name.as_deref()),
```

- [ ] **Step 2: Compile and test**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire switch name arg through main dispatch"
```

---

### Task 4: Update init command — auto-detect, --add, JUNKTREE_PREV

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/commands/init_shell.rs`
- Modify: `src/commands/alias.rs`

- [ ] **Step 1: Update Init CLI variant**

Unhide the command and make shell optional, add `--add` flag:

```rust
/// Output shell function for eval (e.g. eval "$(junktree init)")
Init {
    /// Shell name: bash, zsh, powershell, nushell (auto-detected if omitted)
    shell: Option<String>,
    /// Append the eval line to your shell profile
    #[arg(long)]
    add: bool,
},
```

Remove `#[command(hide = true)]` from `Init`.

- [ ] **Step 2: Update init_shell.rs to handle auto-detect and --add**

```rust
pub fn run(shell_name: Option<&str>, add_to_profile: bool, config: &Config) -> Result<()> {
    let shell = match shell_name {
        Some(name) => alias::parse_shell_name(name)?,
        None => alias::detect_shell_public()?,
    };
    let function_text = alias::shell_function(&shell, &config.command_name);

    if add_to_profile {
        alias::add_to_profile(&shell, config)?;
    } else {
        println!("{}", function_text);
    }
    Ok(())
}
```

- [ ] **Step 3: Update shell functions in alias.rs to track JUNKTREE_PREV**

Update each shell function template to set `JUNKTREE_PREV` before `cd`:

For Bash/Zsh:
```rust
Shell::Bash | Shell::Zsh => format!(
    r#"{name}() {{
    command junktree "$@"
    local exit_code=$?
    local nav_file="$HOME/.junktree_cd"
    if [ -f "$nav_file" ]; then
        local cd_path
        cd_path=$(cat "$nav_file")
        rm -f "$nav_file"
        if [ -n "$cd_path" ] && [ -d "$cd_path" ]; then
            export JUNKTREE_PREV="$PWD"
            cd "$cd_path" || true
        fi
    fi
    return $exit_code
}}"#,
    name = name
),
```

For PowerShell:
```rust
Shell::PowerShell => format!(
    r#"function {name} {{
    & junktree @Args
    $navFile = Join-Path $HOME ".junktree_cd"
    if (Test-Path $navFile) {{
        $cdPath = (Get-Content $navFile -Raw).Trim()
        Remove-Item $navFile -Force
        if ($cdPath -and (Test-Path $cdPath)) {{
            $env:JUNKTREE_PREV = (Get-Location).Path
            Set-Location $cdPath
        }}
    }}
}}"#,
    name = name
),
```

For Nushell:
```rust
Shell::Nushell => format!(
    r#"def --env {name} [...args: string] {{
    junktree ...$args
    let nav_file = ($env.HOME | path join ".junktree_cd")
    if ($nav_file | path exists) {{
        let cd_path = (open $nav_file | str trim)
        rm $nav_file
        if ($cd_path | is-not-empty) {{
            $env.JUNKTREE_PREV = (pwd)
            cd $cd_path
        }}
    }}
}}"#,
    name = name
),
```

- [ ] **Step 4: Make detect_shell public in alias.rs**

Rename `detect_shell` to `detect_shell_public` (or make it `pub(crate)`):

```rust
pub(crate) fn detect_shell_public() -> Result<Shell> {
    detect_shell()
}
```

Or just change the visibility of the existing function:

```rust
pub(crate) fn detect_shell() -> Result<Shell> {
```

- [ ] **Step 5: Add add_to_profile function to alias.rs**

Extract the profile-writing logic from the existing `alias::run` into a reusable function:

```rust
/// Add the eval/source line to the shell profile (called by `init --add`).
pub(crate) fn add_to_profile(shell: &Shell, config: &Config) -> Result<()> {
    let profile = profile_path(shell)?;
    let profile_display = dunce::simplified(&profile).display().to_string();
    let name = &config.command_name;

    // Build the eval line
    let eval_line = match shell {
        Shell::Bash | Shell::Zsh => format!("eval \"$(junktree init {})\"", shell_name_str(shell)),
        Shell::PowerShell => format!("Invoke-Expression (& junktree init {})", shell_name_str(shell)),
        Shell::Nushell => format!("source (junktree init {} | save --force /tmp/junktree_init.nu; /tmp/junktree_init.nu)", shell_name_str(shell)),
    };

    // Check for duplicate
    if profile.exists() {
        let content = fs::read_to_string(&profile)?;
        if content.contains("junktree init") {
            output::info(&format!(
                "Shell init already configured in {}",
                profile_display
            ));
            return Ok(());
        }
    }

    // Create parent dirs if needed
    if let Some(parent) = profile.parent() {
        fs::create_dir_all(parent)?;
    }

    // Append
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile)?;
    writeln!(file)?;
    writeln!(file, "# junktree shell integration")?;
    writeln!(file, "{}", eval_line)?;

    output::success(&format!("Added shell init to {}", profile_display));
    output::info(&restart_hint(shell, &profile_display));

    Ok(())
}

fn shell_name_str(shell: &Shell) -> &'static str {
    match shell {
        Shell::Bash => "bash",
        Shell::Zsh => "zsh",
        Shell::PowerShell => "powershell",
        Shell::Nushell => "nushell",
    }
}
```

- [ ] **Step 6: Update main.rs Init dispatch**

```rust
Commands::Init { shell, add } => return commands::init_shell::run(shell.as_deref(), *add, config),
```

- [ ] **Step 7: Update alias.rs tests for JUNKTREE_PREV**

```rust
#[test]
fn shell_function_bash_tracks_prev() {
    let output = shell_function(&Shell::Bash, "wt");
    assert!(output.contains("JUNKTREE_PREV"), "Bash function should set JUNKTREE_PREV");
}

#[test]
fn shell_function_powershell_tracks_prev() {
    let output = shell_function(&Shell::PowerShell, "wt");
    assert!(output.contains("JUNKTREE_PREV"), "PowerShell function should set JUNKTREE_PREV");
}

#[test]
fn shell_function_nushell_tracks_prev() {
    let output = shell_function(&Shell::Nushell, "wt");
    assert!(output.contains("JUNKTREE_PREV"), "Nushell function should set JUNKTREE_PREV");
}
```

- [ ] **Step 8: Run all tests**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 9: Commit**

```bash
git add src/cli.rs src/commands/init_shell.rs src/commands/alias.rs src/main.rs
git commit -m "feat: init auto-detect, --add to profile, JUNKTREE_PREV tracking"
```

---

### Task 5: Add merged status to list collect

**Files:**
- Modify: `src/commands/list/collect.rs`

- [ ] **Step 1: Add merged field to WorktreeInfo**

```rust
pub struct WorktreeInfo {
    pub worktree: Worktree,
    pub status: WorktreeStatus,
    pub main_rel: MainRelationship,
    pub operation: ActiveOperation,
    pub branch_info: Option<BranchInfo>,
    pub line_diff_head: Option<LineDiff>,
    pub line_diff_main: Option<LineDiff>,
    pub is_current: bool,
    pub display_name: String,
    pub is_merged: bool,     // NEW
}
```

- [ ] **Step 2: Collect merged branches**

At the start of `collect_all`, after getting `default_branch`, compute the merged set:

```rust
// Collect merged branches (transitive — includes chains)
let merged_output = crate::git::git(project_dir, &["branch", "--merged", &default_branch])
    .unwrap_or_default();
let merged_branches: std::collections::HashSet<String> = merged_output
    .lines()
    .map(|line| line.trim().trim_start_matches("* ").to_string())
    .collect();
```

Then when building each `WorktreeInfo`, set:

```rust
let is_merged = wt.branch.as_ref()
    .map(|b| merged_branches.contains(b.as_str()))
    .unwrap_or(false);
```

Add `is_merged` to every `WorktreeInfo` construction (including bare entries and prunable entries — set to `false` for bare, check branch for prunable).

- [ ] **Step 3: Update short mode too**

In `collect_short`, set `is_merged: false` for all entries (short mode doesn't compute this).

- [ ] **Step 4: Run compile check**

Run: `cargo check`
Expected: Compiles (display/json will need updates — next tasks).

- [ ] **Step 5: Commit**

```bash
git add src/commands/list/collect.rs
git commit -m "feat: collect merged status for each worktree"
```

---

### Task 6: Show orphan and merged labels in list display

**Files:**
- Modify: `src/commands/list/display.rs`

- [ ] **Step 1: Add labels to the display name line**

In the display loop where each worktree name is printed, append labels:

```rust
// After the name/branch line, append status labels
if info.worktree.is_prunable {
    print!(" {}", "(orphan)".if_supports_color(Stdout, |t| t.red()));
}
if info.is_merged {
    print!(" {}", "(merged)".if_supports_color(Stdout, |t| t.dimmed()));
}
```

Find the exact insertion point in the existing `print_rich` function where each worktree entry's header line is printed. The labels go right after the name/branch display, before the newline.

- [ ] **Step 2: Run compile check**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src/commands/list/display.rs
git commit -m "feat: show (orphan) and (merged) labels in list output"
```

---

### Task 7: Add orphan and merged to JSON output

**Files:**
- Modify: `src/commands/list/json.rs`

- [ ] **Step 1: Add fields to JsonWorktree**

```rust
#[derive(Serialize)]
struct JsonWorktree {
    name: String,
    branch: Option<String>,
    path: String,
    is_current: bool,
    dirty: JsonDirty,
    upstream: Option<JsonUpstream>,
    main_relationship: String,
    operation: Option<String>,
    commit: Option<JsonCommit>,
    line_diff_head: Option<JsonLineDiff>,
    line_diff_main: Option<JsonLineDiff>,
    is_locked: bool,
    is_prunable: bool,
    orphan: bool,     // NEW
    merged: bool,     // NEW
}
```

- [ ] **Step 2: Set values in to_json_worktree**

```rust
orphan: info.worktree.is_prunable,
merged: info.is_merged,
```

- [ ] **Step 3: Add test**

```rust
#[test]
fn json_includes_orphan_and_merged() {
    let mut info = make_info();
    info.worktree.is_prunable = true;
    info.is_merged = true;
    let json_wt = to_json_worktree(&info);
    assert!(json_wt.orphan);
    assert!(json_wt.merged);
}

#[test]
fn json_orphan_and_merged_default_false() {
    let info = make_info();
    let json_wt = to_json_worktree(&info);
    assert!(!json_wt.orphan);
    assert!(!json_wt.merged);
}
```

- [ ] **Step 4: Update make_info helper in tests**

Add `is_merged: false` to the `make_info()` function.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib commands::list -- --nocapture`
Expected: All PASS.

- [ ] **Step 6: Commit**

```bash
git add src/commands/list/json.rs src/commands/list/collect.rs
git commit -m "feat: add orphan and merged fields to list JSON output"
```

---

### Task 8: Run full test suite and fix any remaining issues

**Files:**
- All modified files

- [ ] **Step 1: Full build**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 2: Full test suite**

Run: `cargo test`
Expected: All PASS.

- [ ] **Step 3: Fix any failing tests**

Address any issues found. Common problems:
- `WorktreeInfo` construction sites missing `is_merged` field
- Switch dispatch signature mismatch in main.rs
- Init CLI changes not reflected in tests

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "fix: resolve all phase 2 integration issues"
```
