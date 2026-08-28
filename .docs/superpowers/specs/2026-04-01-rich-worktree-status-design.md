# Rich Worktree Status Display

**Date:** 2026-04-01
**Status:** Approved

## Overview

Enhance `junktree list` to show comprehensive per-worktree status: uncommitted changes, upstream ahead/behind, relationship to default branch, active git operations, commit age, commit message, and line-level diff stats. Modeled after worktrunk's status display with aligned status columns.

## Data Model

### WorktreeStatus (extended)

Extend existing `src/git/status.rs` struct with upstream tracking:

```rust
pub struct WorktreeStatus {
    pub has_staged: bool,
    pub has_modified: bool,
    pub has_untracked: bool,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
}
```

Parsed from `git status --porcelain=v2 --branch`. The `# branch.ab +N -M` header line provides ahead/behind. Falls back to v1 porcelain (existing behavior) if v2 fails.

### BranchInfo (new: `src/git/branch_info.rs`)

Batch-collected via single `git for-each-ref` call for all branches:

```rust
pub struct BranchInfo {
    pub upstream: Option<String>,
    pub commit_age_secs: i64,
    pub commit_message: String,
}
```

Collected into `HashMap<String, BranchInfo>` keyed by branch name.

### MainRelationship (new: `src/git/main_relationship.rs`)

Mutually exclusive states for branch's relationship to the default branch:

```rust
pub enum MainRelationship {
    IsDefault,
    Ahead(u32),
    Behind(u32),
    Diverged { ahead: u32, behind: u32 },
    Integrated,
    WouldConflict,
    SameCommit,
    Orphan,
}
```

### ActiveOperation (new: `src/git/operations.rs`)

Detected via filesystem checks (no git calls):

```rust
pub enum ActiveOperation {
    None,
    Rebase,
    Merge,
}
```

### LineDiff (new: `src/git/line_diff.rs`)

Parsed from `git diff --numstat`:

```rust
pub struct LineDiff {
    pub insertions: u32,
    pub deletions: u32,
}
```

### WorktreeInfo (new: `src/commands/list/collect.rs`)

Composite struct combining all per-worktree data:

```rust
pub struct WorktreeInfo {
    pub worktree: Worktree,
    pub status: WorktreeStatus,
    pub main_rel: MainRelationship,
    pub operation: ActiveOperation,
    pub branch_info: Option<BranchInfo>,
    pub line_diff_head: Option<LineDiff>,
    pub line_diff_main: Option<LineDiff>,
}
```

## Git Commands & Collection Strategy

### Batch phase (once per invocation)

| Command | Purpose | Parsed into |
|---------|---------|-------------|
| `git for-each-ref --format='%(refname:short)\|%(upstream:short)\|%(committerdate:unix)\|%(subject)' refs/heads` | Branch metadata for ALL branches | `HashMap<String, BranchInfo>` |
| `git rev-parse --abbrev-ref HEAD` (on bare repo) | Detect default branch name | `String` |

### Per-worktree phase (sequential, per worktree)

| Command | Purpose | Parsed into |
|---------|---------|-------------|
| `git -C <path> status --porcelain=v2 --branch` | Dirty status + upstream ahead/behind | `WorktreeStatus` |
| `git -C <path> diff --numstat HEAD` | Line diff vs HEAD | `LineDiff` |
| `git merge-base <head> <default>` | Common ancestor | SHA (input for next commands) |
| `git rev-list --count <merge-base>..<head>` | Commits ahead of default | `MainRelationship` |
| `git rev-list --count <merge-base>..<default>` | Commits behind default | `MainRelationship` |
| `git merge-tree --write-tree <merge-base> <head> <default>` | Conflict detection (git 2.38+) | `MainRelationship::WouldConflict` |
| `git rev-parse <default>^{tree}` vs `git rev-parse <merge-base>^{tree}` | Integration detection: if merge-base tree matches default tree, branch content is already in default (squash-merged/rebased) | `MainRelationship::Integrated` |
| `git diff --numstat <default>...<head>` | Line diff vs default branch | `LineDiff` |
| Filesystem: `rebase-merge/`, `rebase-apply/`, `MERGE_HEAD` | Active operation detection | `ActiveOperation` |

### Collection flow

```
list_worktrees()           -> Vec<Worktree>
batch_branch_info()        -> HashMap<String, BranchInfo>    (ONE git call)
detect_default_branch()    -> String                          (ONE git call)

for each worktree:
  extended_status()        -> WorktreeStatus                  (ONE git call)
  line_diff_head()         -> LineDiff                        (ONE git call)
  main_relationship()      -> MainRelationship                (2-3 git calls)
  detect_operation()       -> ActiveOperation                 (filesystem only)
  line_diff_main()         -> LineDiff                        (ONE git call)

-> Vec<WorktreeInfo>
```

### MainRelationship detection priority

States are checked in this order (first match wins):

1. **IsDefault** — branch name equals default branch name
2. **Orphan** — `git merge-base` fails (no common ancestor)
3. **SameCommit** — head == default head (ahead=0, behind=0)
4. **Integrated** — merge-base tree matches default tree (content already in default)
5. **WouldConflict** — `merge-tree --write-tree` reports conflicts (git 2.38+ only)
6. **Diverged** — both ahead > 0 and behind > 0
7. **Ahead** — ahead > 0, behind == 0
8. **Behind** — behind > 0, ahead == 0

## Output Format

### Rich output (default)

```
=== Git Worktrees for myproject ===

[bare repository] -> D:/repos/myproject/.bare

Worktrees:
★ [current] ▸ main              clean  ⇣2     ^       3d ago  fix: resolve auth bug
       D:/repos/myproject/main
  ▸ feature-auth -> feat/auth     +!?   ⇡1    ↓5      1h ago  wip: add oauth flow
       D:/repos/myproject/feature-auth
  ▸ fix-login                      !     |     ↓1      5d ago  fix: login redirect
       D:/repos/myproject/fix-login
  ▸ refactor-db -> refactor/db   clean  ⇡3    ↑2 ✗    2w ago  refactor: extract query layer
       D:/repos/myproject/refactor-db
  ▸ spike-perf                   clean   —    ∅       4w ago  spike: benchmark queries
       D:/repos/myproject/spike-perf
```

### Column definitions

| Column | Width | Content | Color |
|--------|-------|---------|-------|
| Gutter | 2-18 | `★ [current] ▸` or `  ▸` | cyan |
| Name | dynamic, right-padded | Worktree display name | cyan (current) / white |
| Branch | dynamic | `-> branch` when differs from name | green |
| Dirty | 5 fixed | `+!?`, `clean`, or subset | yellow (dirty) / dim green (clean) |
| Upstream | 4 fixed | `⇡N` / `⇣N` / `⇅` / `\|` / `—` | cyan (sync) / yellow (ahead) / red (behind) |
| Main | 6 fixed | `^` / `↑N` / `↓N` / `↕` / `⊂` / `✗` / `_` / `∅` | varies |
| Age | 7 fixed | relative time (`1h ago`, `3d ago`, `2w ago`) | dim |
| Message | truncated ~40ch | First line of last commit | dim |

### Upstream symbols

| Symbol | Meaning | Color |
|--------|---------|-------|
| `⇡N` | N commits ahead of remote (unpushed) | yellow |
| `⇣N` | N commits behind remote (to pull) | red |
| `⇅` | Diverged from remote | red |
| `\|` | In sync with remote | dim/green |
| `—` | No upstream configured | dim |

### Main relationship symbols

| Symbol | Meaning | Color |
|--------|---------|-------|
| `^` | This IS the default branch | cyan |
| `↑N` | N commits ahead of default | yellow |
| `↓N` | N commits behind default | dim |
| `↕` | Diverged from default | yellow |
| `⊂` | Content integrated (squash/rebase merged) | green |
| `✗` | Would conflict with default | red |
| `_` | Same commit as default, safe to delete | dim |
| `∅` | Orphan, no common ancestor | dim |

### Special indicators (between name and dirty column)

| Symbol | Meaning | Color |
|--------|---------|-------|
| `⤴` | Rebase in progress | yellow |
| `⤵` | Merge in progress | yellow |
| `⊞` | Locked worktree | yellow |
| `⊟` | Prunable worktree | red |

## CLI Flags

```rust
/// List all worktrees
List {
    /// Minimal output: name + branch + dirty only
    #[arg(long)]
    short: bool,

    /// Output as JSON for scripting
    #[arg(long)]
    json: bool,

    /// Hide worktree path lines
    #[arg(long)]
    no_path: bool,

    /// Disable colored output
    #[arg(long)]
    no_color: bool,
}
```

| Flag | Effect |
|------|--------|
| (none) | Full rich output with all columns, paths, colors |
| `--short` | Current output style. Skips expensive git calls — only runs `git status --porcelain` |
| `--json` | Structured JSON array with all collected fields. Implies `--no-color` |
| `--no-path` | Omit indented path lines — one line per worktree |
| `--no-color` | Force disable ANSI colors. Also respects `NO_COLOR` env var |

### JSON output shape

```json
[
  {
    "name": "main",
    "branch": "main",
    "path": "D:/repos/myproject/main",
    "is_current": true,
    "dirty": { "staged": false, "modified": false, "untracked": false },
    "upstream": { "ahead": 0, "behind": 2, "tracking": "origin/main" },
    "main_relationship": "is_default",
    "operation": null,
    "commit": { "age_secs": 259200, "message": "fix: resolve auth bug" },
    "line_diff_head": { "insertions": 0, "deletions": 0 },
    "line_diff_main": null,
    "is_locked": false,
    "is_prunable": false
  }
]
```

## Error Handling & Resilience

| Scenario | Behavior |
|----------|----------|
| `git status --porcelain=v2` fails | Fall back to v1 porcelain. If both fail, show `?` in dirty column |
| `git for-each-ref` fails | Skip age + message columns for all worktrees |
| `git merge-base` fails (orphan) | `MainRelationship::Orphan` — show `∅` |
| `git merge-tree` fails (git < 2.38) | Skip conflict detection, show ahead/behind only |
| `git diff --numstat` fails | Omit line diff data for that worktree |
| Prunable worktree (directory missing) | Show `⊟`, skip per-worktree git calls |
| Locked worktree | Show `⊞`, still attempt status collection |

### Git version detection

Run `git merge-tree --write-tree` once at startup with dummy args. If it fails with "unknown option", set `supports_merge_tree: bool` and skip conflict detection for all worktrees.

## File Structure

### New files

| File | Purpose |
|------|---------|
| `src/git/branch_info.rs` | `BranchInfo` + `batch_branch_info()` — `for-each-ref` parsing |
| `src/git/main_relationship.rs` | `MainRelationship` + `detect_main_relationship()` — merge-base logic |
| `src/git/operations.rs` | `ActiveOperation` + `detect_operation()` — filesystem checks |
| `src/git/line_diff.rs` | `LineDiff` + `line_diff()` — `diff --numstat` parsing |
| `src/commands/list/mod.rs` | Refactored list orchestrator (replaces `list.rs`) |
| `src/commands/list/collect.rs` | `WorktreeInfo` + collection orchestration |
| `src/commands/list/display.rs` | Rich output formatting — alignment, colors, symbols |
| `src/commands/list/json.rs` | JSON serialization (`--json` flag) |

### Modified files

| File | Change |
|------|--------|
| `src/cli.rs` | `Commands::List` becomes struct variant with flag fields |
| `src/git/status.rs` | Add `ahead`, `behind`, `has_upstream` to `WorktreeStatus`. Add `parse_porcelain_v2()` |
| `src/git/mod.rs` | Register new submodules |
| `src/output.rs` | Add new Unicode constants for status symbols |
| `src/main.rs` | Pass `List` fields to `commands::list::run()` |
| `Cargo.toml` | Add `serde`, `serde_json` |

### Unchanged

- `src/git/worktree.rs` — `Worktree` struct untouched, wrapped by `WorktreeInfo`
- `src/git/repo.rs`, `src/layout.rs`, `src/config.rs` — no changes
- All existing tests remain valid

### Module dependency flow

```
cli.rs (parse flags)
  -> main.rs (dispatch)
    -> commands::list::mod.rs (orchestrate)
      -> commands::list::collect.rs
        -> git::worktree (existing)
        -> git::status (extended)
        -> git::branch_info (new)
        -> git::main_relationship (new)
        -> git::operations (new)
        -> git::line_diff (new)
      -> commands::list::display.rs (rich output)
      -> commands::list::json.rs (--json output)
```

## Dependencies

Add to `Cargo.toml`:

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

## Testing Strategy

### Methodology: Test-Driven Development (TDD)

All new code follows strict TDD: write failing test first, then implement minimal code to pass, then refactor. Tests are committed alongside (or before) the implementation they validate.

### Coverage target: 85%+

Measured with `cargo tarpaulin` or `cargo llvm-cov`. The 85% floor applies to new code introduced by this feature.

### Test breakdown by module

| Module | Test approach | Expected coverage |
|--------|--------------|-------------------|
| `git/status.rs` (v2 parser) | Unit tests with fixture strings for every `# branch.*` header variant and file status combination | ~95% |
| `git/branch_info.rs` | Unit tests with fixture `for-each-ref` output: multiple branches, missing upstream, empty output | ~95% |
| `git/main_relationship.rs` | Unit tests for each `MainRelationship` variant detection, priority ordering, edge cases (orphan, same commit) | ~95% |
| `git/operations.rs` | Unit tests with `tempdir` — create/omit sentinel files (`rebase-merge/`, `MERGE_HEAD`) and assert detection | ~90% |
| `git/line_diff.rs` | Unit tests with fixture `diff --numstat` output: normal, binary files, empty diff, rename | ~95% |
| `commands/list/display.rs` | Unit tests capturing formatted output strings — verify alignment, colors (with `NO_COLOR`), symbol rendering per state | ~85% |
| `commands/list/json.rs` | Unit tests serializing `WorktreeInfo` structs and asserting JSON shape | ~90% |
| `commands/list/collect.rs` | Integration tests with real git repos (via `tempfile` + `assert_cmd`). Thin orchestration code — most logic is in the modules above | ~70% |
| `commands/list/mod.rs` | Integration tests via CLI (`assert_cmd`) for flag behavior (`--short`, `--json`, `--no-path`, `--no-color`) | ~75% |
| `cli.rs` (flag parsing) | Unit tests with `Cli::parse_from` for all new flag combinations | ~95% |

### Testing architecture: separate parsing from execution

Each git module follows the same pattern:

```rust
// Public API — thin wrapper, calls git and delegates to parser
pub fn batch_branch_info(dir: &Path) -> Result<HashMap<String, BranchInfo>> {
    let output = super::git(dir, &[...])?;
    Ok(parse_for_each_ref(&output))
}

// Parser — pure function, takes string, returns struct. Fully unit-testable.
fn parse_for_each_ref(output: &str) -> HashMap<String, BranchInfo> { ... }
```

The parsers contain 90%+ of the logic and are trivially testable with fixture strings. The public wrappers are 3-5 lines each (call git, delegate to parser). Integration tests in `tests/` cover the wrappers with real git repos.

### Integration tests

Extend `tests/test_list.rs` with scenarios:

- Worktree with staged + modified + untracked files (dirty indicators)
- Worktree ahead of upstream (create local commit, don't push)
- Worktree behind upstream (advance remote, don't pull)
- Default branch identification (`^` marker)
- `--short` flag produces minimal output (no upstream/main columns)
- `--json` flag produces valid parseable JSON
- `--no-path` flag omits path lines
- Prunable worktree (delete directory after creating worktree)
- Locked worktree (`git worktree lock`)

## Approach

Hybrid A+B: Use `git for-each-ref` for batch branch metadata (one call for all branches), then sequential per-worktree calls for status, diffs, and main relationship. `git status --porcelain=v2 --branch` combines dirty detection and upstream ahead/behind into a single call per worktree.
