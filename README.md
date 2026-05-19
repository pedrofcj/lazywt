# lazywt

A cross-platform git worktree manager for bare repositories. One Rust binary that replaces a stack of shell scripts (Bash, Zsh, PowerShell, Nushell) with identical behavior on every platform.

Built for developers who use worktrees as their primary branching workflow: every feature, fix, or experiment lives in its own directory, sharing a single bare repository underneath.

## Why

If you already use `git worktree`, you know the friction: verbose commands, manual directory layout, no built-in branch lifecycle, no consistent way to jump between worktrees. `lazywt` wraps the `git` CLI with opinionated defaults for the bare-repo workflow:

- Worktrees live in predictable directories next to a shared `.git/` bare repo
- Branch creation, removal, and cleanup happen as a single operation
- Shell integration auto-`cd`s into newly created worktrees
- GitHub PRs check out into their own worktree with one command

Shells out to `git` rather than linking `libgit2` — output and edge-case behavior match what you'd get typing the commands yourself.

## Install

```sh
cargo install lazywt
```

Requires `git` available on `$PATH`. No other runtime dependencies.

Pre-built binaries: see [Releases](https://github.com/pedrofcj/lazywt/releases).

## Quick start

```sh
# Clone a repo into a bare-worktree layout
lazywt clone https://github.com/owner/repo.git

# Create a worktree on a new branch
lazywt add my-feature

# List worktrees (current highlighted, dirty/merged status shown)
lazywt list

# Jump between worktrees
lazywt switch my-feature      # by name
lazywt switch                 # interactive picker
lazywt switch -               # previous worktree

# Pull down a GitHub PR into its own worktree
lazywt pr 1234

# Cleanup merged/orphaned worktrees
lazywt prune

# Remove a single worktree (and its branch)
lazywt remove my-feature
```

## Shell integration

To make `lazywt switch` and `lazywt add` actually change your shell's working directory, install the shell function:

```sh
# bash / zsh
eval "$(lazywt init bash)"
eval "$(lazywt init zsh)"

# PowerShell — append to $PROFILE
lazywt init pwsh

# Nushell
lazywt init nu
```

Or auto-install into your shell profile:

```sh
lazywt init --add
```

The wrapper writes to `~/.lazywt_cd` after each command; the shell function reads it and `cd`s. `LAZYWT_PREV` is set to support `switch -`.

## Commands

| Command       | Purpose                                                            |
|---------------|--------------------------------------------------------------------|
| `clone`       | Clone a remote as bare and set up the worktree layout              |
| `setup`       | Convert an existing regular repo into the bare worktree layout     |
| `migrate`     | Migrate classic (`.git/trees/`) layout to modern (root-level)      |
| `add`         | Create a worktree on a new or existing branch                      |
| `remove`      | Remove a worktree and delete its branch                            |
| `remove-all`  | Remove every non-default worktree                                  |
| `list`        | List worktrees with dirty / merged / orphan indicators             |
| `switch`      | Interactive picker, direct name, or `-` for previous               |
| `pr`          | Check out a GitHub pull request into a new worktree                |
| `prune`       | Remove merged and orphaned worktrees                               |
| `sync`        | Fetch all remotes and fast-forward local branches                  |
| `fix-fetch`   | Repair fetch refspec on a bare clone                               |
| `config`      | Interactive config editor                                          |
| `init`        | Print or install the shell wrapper                                 |
| `update`      | Check crates.io for a newer version (notification only)            |
| `version`     | Print the installed version                                        |

Run `lazywt <command> --help` for flags and arguments.

## Configuration

Two-tier JSON config: a global file plus optional per-repo overrides.

- **Global:** `~/.config/lazywt/config.json` (Linux/macOS) or `%APPDATA%\lazywt\config.json` (Windows)
- **Per-repo:** `<bare-root>/lazywt.json`

Edit interactively:

```sh
lazywt config
```

Available fields:

| Field               | Default     | Meaning                                                   |
|---------------------|-------------|-----------------------------------------------------------|
| `command_name`      | `"wt"`      | Name of the shell wrapper function                        |
| `worktree_folder`   | `null`      | Override the directory worktrees are created under        |
| `bare_dir`          | `null`      | Custom bare repo directory name (default `.git`)          |
| `auto_update`       | `true`      | Background check crates.io for newer versions             |
| `auto_navigate`     | `false`     | Skip the post-`add` "navigate to new worktree?" prompt    |
| `branch_prefix`     | `""`        | Prefix prepended to new branch names                      |
| `copy_files`        | `[]`        | Files copied into every new worktree                      |
| `worktree_location` | `"inside"`  | `"inside"` bare repo dir, or `"outside"` (sibling)        |

### Legacy migration

On first run, `lazywt` automatically migrates `~/.wtconfig` (the INI format used by the legacy shell scripts) into the new JSON format. The old file is left in place untouched.

## Repository layout

`lazywt` creates and expects this structure for each managed repo:

```
my-project/
├── .git/                # bare repository (or custom name via `bare_dir`)
├── main/                # worktree for the default branch
├── feature-x/           # worktree on branch `feature-x`
└── lazywt.json          # optional per-repo config
```

The `worktree_location = "outside"` variant places the bare dir as a sibling:

```
my-project.git/          # bare repository
my-project/
├── main/
└── feature-x/
```

## Building from source

```sh
git clone https://github.com/pedrofcj/lazywt.git
cd lazywt
cargo build --release
./target/release/lazywt --help
```

### Tests

```sh
cargo test
```

The integration suite shells out to `git` against temporary directories — no network access required.

## Acknowledgements

`lazywt` is the Rust port of the multi-shell [worktree scripts](https://github.com/pedrofcj/worktree). Behaviour and terminal output match the original 1:1.
