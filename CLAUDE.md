<!-- GSD:project-start source:PROJECT.md -->
## Project

**lazywt**

A Rust CLI tool for managing git worktrees with bare repositories. It is a 1:1 port of the existing multi-shell [worktree scripts](https://github.com/pedrofcj/worktree) (PowerShell, Bash, Zsh, Nushell) into a single cross-platform binary. Built for developers who use git worktrees as their primary branching workflow.

**Core Value:** One binary, identical behavior everywhere — replace four shell scripts with a single Rust CLI that produces the exact same output on every platform.

### Constraints

- **Tech stack**: Rust — chosen for cross-platform single binary distribution
- **Git interaction**: Shell out to `git` CLI — matches existing behavior, avoids libgit2 dependency complexity
- **Output fidelity**: Must produce identical terminal output (colors, symbols, messages) to the existing shell scripts
- **Config format**: Must read existing `~/.wtconfig` INI format — no migration burden on users
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Recommended Stack
### CLI Framework
| Choice | Crate | Confidence |
|--------|-------|------------|
| **Argument parsing** | `clap` (derive) | HIGH |
### Terminal Output
| Choice | Crate | Confidence |
|--------|-------|------------|
| **Colors** | `owo-colors` | HIGH |
| **TTY detection** | `is-terminal` | HIGH |
### Process Execution
| Choice | Crate | Confidence |
|--------|-------|------------|
| **Git subprocess** | `std::process::Command` | HIGH |
### Configuration
| Choice | Crate | Confidence |
|--------|-------|------------|
| **INI parsing** | Hand-rolled parser | HIGH |
### Error Handling
| Choice | Crate | Confidence |
|--------|-------|------------|
| **Library errors** | `thiserror` | HIGH |
| **Application errors** | `anyhow` | MEDIUM |
### Cross-Platform
| Choice | Crate | Confidence |
|--------|-------|------------|
| **Path canonicalization** | `dunce` | HIGH |
| **Home directory** | `dirs` or `home` | HIGH |
### Testing
| Choice | Crate | Confidence |
|--------|-------|------------|
| **Temp directories** | `tempfile` | HIGH |
| **CLI assertions** | `assert_cmd` | MEDIUM |
| **Output assertions** | `predicates` | MEDIUM |
### Version Checking
| Choice | Crate | Confidence |
|--------|-------|------------|
| **HTTP client** | `ureq` (blocking) | MEDIUM |
| **Version comparison** | `semver` | HIGH |
## Cargo.toml Summary
## What NOT to Use
| Crate | Why Not |
|-------|---------|
| `libgit2` / `git2` | PROJECT.md explicitly requires shelling out to git CLI. libgit2 adds native build dependency complexity and behavior differences. |
| `crossterm` | No TUI, no cursor control. Only need colored text output. |
| `reqwest` | Massive dependency tree. `ureq` does the one HTTP call we need. |
| `colored` | Heap allocations per styled string. `owo-colors` is zero-alloc. |
## Build & Distribution
- **Build:** `cargo build --release`
- **Install:** `cargo install lazywt` (binary name configurable post-install via `~/.wtconfig`)
- **Cross-compile:** Standard `cross` or `cargo-zigbuild` for Linux/macOS/Windows targets
- **CI:** GitHub Actions with `actions-rs/toolchain` for multi-platform builds
- **No runtime dependencies** beyond `git` being available on `$PATH`
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd:quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd:debug` for investigation and bug fixing
- `/gsd:execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd:profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
