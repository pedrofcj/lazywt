# junktree v2.0 Design Spec

## Overview

v2.0 evolves junktree from a 1:1 shell-script port into a full-featured worktree manager. The milestone introduces a JSON config system, decoupled branch/worktree naming, GitHub PR integration, automated pruning, remote sync, non-bare repo support, and shell integration improvements.

No backward compatibility with v1.0 is required — the author is the sole user.

## Decisions

| Decision | Rationale |
|----------|-----------|
| Drop `~/.wtconfig` INI, replace with JSON | Enables per-repo config, richer data types (arrays, nested objects). No migration burden — sole user. |
| Global + per-repo config (two files) | Clean separation. Global sets defaults, per-repo overrides. Easy to reason about. |
| Env vars still override everything | Consistent with v1.0. Useful for CI/scripting. |
| `gh` CLI with git-remote fallback for PR | Best UX when `gh` is available (auth, fork support), graceful degradation without it. |
| `JUNKTREE_PREV` env var for `switch -` | No file I/O, scoped to shell session (correct behavior, same as `cd -`). |
| Non-bare support with `.wts/` default | Meets users where they are. Inside-repo default keeps things self-contained. |
| Defer `merge` command | Too complex for this milestone. Users can `git merge` directly. |
| Defer installation scripts | Not needed until public release. |

## Phasing

| Phase | Focus | Features |
|-------|-------|----------|
| 1 | Config System + `add` Rework | JSON config, `config` command, `add` branch/worktree decoupling |
| 2 | UX Improvements | `switch` direct + `-`, `init` enhancements, `list` orphan + merged |
| 3 | Power Commands | `pr`, `prune`, `sync` |
| 4 | Non-bare + Automation | Non-bare repo support, `setup`, copy-files on add/remove |

---

## Phase 1: Config System + `add` Rework

### JSON Config Architecture

Two-tier config with environment variable overrides.

**Global config:** `~/.config/junktree/config.json`

```json
{
  "branch_prefix": "feature/",
  "auto_update": true,
  "auto_navigate": false,
  "command_name": "wt"
}
```

**Per-repo config:** `.junktree.json` at the repo root (bare repo root or regular repo root)

```json
{
  "branch_prefix": "bug/",
  "copy_files": [".env*", "config/*.local"],
  "worktree_location": "inside"
}
```

**Resolution order (highest wins):**
1. Environment variables (`WT_*`)
2. Per-repo `.junktree.json`
3. Global `~/.config/junktree/config.json`
4. Hardcoded defaults

Missing keys fall through to the next tier. Unknown keys are silently ignored.

**Global config keys:**

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `branch_prefix` | string | `""` (empty) | Prefix for branch names when creating worktrees |
| `auto_update` | bool | `true` | Check for updates automatically |
| `auto_navigate` | bool | `false` | Auto-cd into new worktrees without prompting |
| `command_name` | string | `"wt"` | Alias name used in shell init and user-facing messages |

**Per-repo config keys (in addition to global keys which can be overridden):**

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `copy_files` | string[] | `[]` | Glob patterns for files to copy into new worktrees |
| `worktree_location` | `"inside"` \| `"sibling"` | `"inside"` | Where worktrees are stored for non-bare repos |

### `add` Rework

Current behavior:
- `add <name> [type]` — positional `type` argument sets branch prefix.

New behavior:
- `add <worktree-name>` — branch name is `{branch_prefix}{worktree-name}` if prefix is configured, else just `worktree-name`.
- `add <worktree-name> -b <full-branch-name>` / `--branch` — worktree name and branch name are fully independent. Prefix is ignored.
- The positional `type` argument is removed.
- `--from <worktree>` stays as-is.

Examples (assuming `branch_prefix: "feature/"` in config):
- `add auth` → worktree `auth`, branch `feature/auth`
- `add fixing-naming -b bug/TTR-3022/fix-naming` → worktree `fixing-naming`, branch `bug/TTR-3022/fix-naming`

Examples (assuming no `branch_prefix` configured):
- `add auth` → worktree `auth`, branch `auth`

### `config` Command

- `config` (no args) — prompts which scope to edit (global or repo), then walks through that scope's keys interactively. Shows current value or default as placeholder. Creates/updates the config file.
- `config --global` — edit global config directly, skip scope prompt.
- `config --repo` — edit per-repo config directly, skip scope prompt.
- First run for a given scope creates the file with chosen values.
- Subsequent runs pre-fill current values so user can edit incrementally.

---

## Phase 2: UX Improvements

### `switch` Improvements

- `switch` (no args) — current behavior, interactive selection list.
- `switch <name>` — validates the worktree exists, cd's directly. No prompt.
  - If the name doesn't match, show a "did you mean?" suggestion based on fuzzy matching against existing worktree names.
- `switch -` — return to previous worktree. The shell wrapper (emitted by `init`) manages a `JUNKTREE_PREV` environment variable, updated on every successful switch.

### `init` Enhancements

Current state: hidden command, emits a shell function for eval.

Enhancements:
- **Auto-detect shell** if no argument given. Check `$SHELL`, `$PSVersionTable`, parent process name, etc. Keep the explicit argument for forcing output for a different shell.
- **Emitted function includes:**
  - `cd` wrapper for `switch` and `add`.
  - `JUNKTREE_PREV` tracking on every successful switch.
  - Alias based on `command_name` from config.
- **`init --add`** — appends the eval line to the appropriate profile file:
  - Bash: `~/.bashrc`
  - Zsh: `~/.zshrc`
  - PowerShell: `$PROFILE`
  - Nushell: `$nu.config-path`
  - Warns if the eval line is already present. Does not duplicate.
- **Supported shells:** Bash, Zsh, PowerShell, Nushell.
- The command is no longer hidden — it becomes a first-class command.

### `list` Enhancements

- **Orphan detection:** Check if each worktree's directory exists on disk. If not, display in red with `(orphan)` label. With `--no-color`, the `(orphan)` text still appears.
- **Merged status:** Run `git branch --merged <default-branch>` for each worktree's branch. If merged, append `(merged)` tag in dimmed/gray.
- Both indicators are included in `--short`, `--json`, and `--no-color` output modes.
- JSON output adds `"orphan": true/false` and `"merged": true/false` fields.

---

## Phase 3: Power Commands

### `pr` Command

Fetch a GitHub pull request into a new worktree for review.

**Usage:**
- `pr <number> [worktree-name]`
- `pr <github-url> [worktree-name]`

**Behavior:**
- Worktree name is optional. Default: `pr-<number>`.
- Examples:
  - `pr 272` → worktree `pr-272`
  - `pr 272 review-mess` → worktree `review-mess`
  - `pr https://github.com/owner/repo/pull/272` → worktree `pr-272`
- The worktree tracks the remote branch (not detached HEAD) so the reviewer can pull updates.
- Branch prefix from config is **not** applied — the branch already exists on the remote.

**Remote discovery (ordered):**
1. Try `gh pr view <number> --json headRefName` — gets the branch name directly. Handles forks.
2. If `gh` fails or isn't installed, parse `origin` remote URL to extract owner/repo, fetch PR metadata via GitHub API with `ureq`.

### `prune` Command

Remove worktrees whose branches are merged or whose directories are orphaned.

**Behavior:**
- Scans all worktrees and identifies:
  1. **Merged** — branch is reachable from the default branch (`git branch --merged <default>`). This is transitive: if branch10 merged into branch9, and branch9 merged into main, both are "merged."
  2. **Orphan** — worktree directory doesn't exist on disk.
- Shows a summary of what will be removed, asks for confirmation.
- Dirty worktrees (uncommitted changes) are listed but skipped with a warning.

**Flags:**
- `--yes` / `-y` — skip confirmation prompt.
- `--force` / `-f` — also remove dirty worktrees. Without this, dirty worktrees are warned about and skipped.

**Removal** reuses the same logic as the existing `remove` command.

### `sync` Command

Fetch remotes and update branches.

**`sync` (no flags):**
- Fetch from all remotes.
- Fast-forward the default branch.
- Report worktrees tracking deleted remote branches (warning).
- Report worktrees that are behind their remote (info).

**`sync --all`:**
- Also attempts to fast-forward every worktree's branch.
- Skips with warning: dirty worktrees, branches that can't fast-forward (diverged).
- Operates on git refs directly — no need to cd into each worktree.

---

## Phase 4: Non-bare Support + `setup` + Copy-files

### Non-bare Repo Support

The tool detects whether it's in a bare repo or a regular repo and adapts.

**Worktree storage for regular repos** (controlled by `worktree_location` in per-repo config):

| Mode | Path | `.gitignore` |
|------|------|--------------|
| `"inside"` (default) | `<repo>/.wts/<name>` | Tool adds `.wts` to `.gitignore` automatically on first worktree creation |
| `"sibling"` | `<repo>-wts/<name>` | No change needed |

**Impact on existing commands:**
All commands (`add`, `remove`, `list`, `switch`, `prune`, `sync`, `pr`) work in both bare and non-bare modes. The difference is only where worktrees are physically placed and how the repo root is discovered.

**Implementation focus:** `layout.rs` / `ProjectContext` expands from two layout variants (classic bare, modern bare) to three (+ non-bare). This is the main structural change.

### `setup` Command

Convert an existing regular (non-bare) clone into a bare repo with worktree structure.

**Behavior:**
- Run inside a regular clone.
- Moves `.git` to become the bare repo.
- Creates the default worktree from the current checkout.
- Preserves remotes, config, hooks.

**Flags:**
- Confirmation required (destructive operation).
- `--yes` — skip confirmation.
- `--dry-run` — show what would happen without doing it.

### Copy-files on Worktree Creation

**On `add`:**
- Read `copy_files` patterns from per-repo config.
- Glob-match against the default worktree's files.
- Copy matching files into the new worktree.

**On `remove`:**
- If any copy-files-tracked files exist in the worktree being removed and differ from the default worktree's version, warn the user before proceeding.
- No auto-update of the original — just a "these files differ, proceed?" prompt.
- `remove --yes` / `remove --force` skips the prompt.

---

## Deferred to Future Milestones

| Feature | Reason |
|---------|--------|
| `merge` command | Too complex. Users can `git merge` directly for now. |
| Installation scripts (`curl \| sh`) | Not needed until public release. |

## References

- [Grove](https://github.com/captainsafia/grove) — shell-init approach, installation scripts
- [worktree-cli](https://github.com/johnlindquist/worktree-cli)
- [wtp](https://github.com/satococoa/wtp)
- [newt](https://github.com/cdzombak/newt)
- [treekanga](https://github.com/garrettkrohn/treekanga)
