# List runtime benchmark

Build the benchmark-only worker locally, without registry access:

```sh
cargo build --release --offline --locked --features benchmarks --bin benchmark-list
python benchmarks/list.py --binary target/release/benchmark-list
```

On Windows use `target/release/benchmark-list.exe`. Python 3.9+ and Git supporting
`init --initial-branch` are required; no Python packages are needed.

Smoke run:

```sh
python benchmarks/list.py --binary target/release/benchmark-list --worktrees 4 --iterations 1
```

The benchmark creates one local repository and linked worktrees entirely inside
a disposable temporary directory. There are no remotes or network operations.
Child environments discard inherited Git/WT settings, isolate home/config/temp
directories, disable update checks and Git protocols, and ignore global/system
Git configuration and templates. The feature-gated worker calls the production
collection and JSON APIs directly; it never loads config or runs navigation or
update code. This matters on Windows, where `dirs` uses Known Folder APIs rather
than HOME/APPDATA environment overrides. Do not pass the normal `lazywt` binary.
Fixtures use a fixed commit date and a repeating
mix of clean, staged, modified and untracked states.

Both `list --json` (rich collection) and `list --short --json` (fast collection)
must match an independently constructed, exact JSON oracle before timing.
Only wall-clock commit age is normalized after its integer/range check.
Every iteration rechecks all fields outside timing. This is a warm-cache,
collection benchmark: worker startup, Git subprocesses, collection, serialization
and captured stdout are included; fixture creation and validation are excluded.
It does not measure normal CLI config/layout dispatch or text rendering.

Defaults are 8 worktrees and 5 measured iterations per mode; limits are 32
worktrees and 100 iterations, with a 30-second timeout per subprocess. The same
unchanged fixture is used for every sample. Median/min/max milliseconds are
reported without a machine-dependent pass/fail threshold. TemporaryDirectory
cleans up on normal completion and exceptions; successful runs also verify
removal and print `fixture_cleanup=ok`. Forced process termination may leave a
temporary directory. Use the same binary profile, Git version and fixture size
when comparing runs.
