"""Bounded, offline list/JSON benchmark using the benchmark-only worker."""

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

COMMIT_TIME_UTC = 946_684_800


def bounded(limit):
    def parse(value):
        number = int(value)
        if not 1 <= number <= limit:
            raise argparse.ArgumentTypeError(f"must be between 1 and {limit}")
        return number

    return parse


def isolated_env(root):
    # Keep only executable/OS essentials, never inherited Git or WT overrides.
    allowed = {"PATH", "SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"}
    env = {k: v for k, v in os.environ.items() if k.upper() in allowed}
    home = root / "home"
    home.mkdir()
    config = home / "config"
    config.mkdir()
    empty = home / "empty"
    empty.mkdir()
    env.update(
        HOME=str(home),
        LAZYWT_HOME=str(home),
        USERPROFILE=str(home),
        APPDATA=str(config),
        LOCALAPPDATA=str(config),
        XDG_CONFIG_HOME=str(config),
        XDG_CACHE_HOME=str(home / "cache"),
        TMPDIR=str(root),
        TMP=str(root),
        TEMP=str(root),
        WT_AUTO_UPDATE="false",
        GIT_CONFIG_NOSYSTEM="1",
        GIT_CONFIG_GLOBAL=os.devnull,
        GIT_TEMPLATE_DIR=str(empty),
        GIT_TERMINAL_PROMPT="0",
        GIT_ALLOW_PROTOCOL="",
        GIT_AUTHOR_NAME="Benchmark",
        GIT_AUTHOR_EMAIL="benchmark@example.invalid",
        GIT_COMMITTER_NAME="Benchmark",
        GIT_COMMITTER_EMAIL="benchmark@example.invalid",
        GIT_AUTHOR_DATE="2000-01-01T00:00:00+0000",
        GIT_COMMITTER_DATE="2000-01-01T00:00:00+0000",
        LC_ALL="C",
        TZ="UTC",
    )
    return env


def run(args, cwd, env):
    result = subprocess.run(
        [str(a) for a in args],
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=30,
        check=True,
    )
    if result.stderr.strip():
        # Git setup progress is expected; lazywt diagnostics are not.
        if Path(str(args[0])).name != "git":
            raise RuntimeError(result.stderr)
    return result.stdout


def fixture(root, count, env):
    project = root / "project"
    project.mkdir()
    main = project / "main"
    run(["git", "init", "--initial-branch=main", main], root, env)
    run(["git", "-C", main, "config", "core.autocrlf", "false"], root, env)
    run(["git", "-C", main, "config", "core.filemode", "false"], root, env)
    (main / "tracked.txt").write_text("base\n", encoding="utf-8")
    run(["git", "-C", main, "add", "tracked.txt"], root, env)
    run(["git", "-C", main, "commit", "-m", "benchmark base"], root, env)
    paths = [main]
    for i in range(1, count):
        path = project / f"wt-{i:03}"
        run(
            ["git", "-C", main, "worktree", "add", "-b", path.name, path, "main"],
            root,
            env,
        )
        paths.append(path)
    # A repeatable mix of clean, staged, modified and untracked worktrees.
    for i, path in enumerate(paths):
        state = i % 4
        if state in (1, 2):
            (path / "tracked.txt").write_text("changed\n", encoding="utf-8")
        if state == 1:
            run(["git", "-C", path, "add", "tracked.txt"], root, env)
        if state == 3:
            (path / "untracked.txt").write_text("untracked\n", encoding="utf-8")
    if run(["git", "-C", main, "remote"], root, env).strip():
        raise RuntimeError("fixture must have no remotes")
    return paths


def validate(output, paths, short, before, after):
    items = json.loads(output)
    if not isinstance(items, list) or len(items) != len(paths):
        raise RuntimeError(f"expected exactly {len(paths)} JSON worktrees")
    actual = {}
    for item in items:
        name = item["name"]
        if name in actual:
            raise RuntimeError(f"duplicate worktree: {name}")
        # Commit ages are wall-clock metadata, not fixture state.
        if item["commit"] is not None:
            age = item["commit"]["age_secs"]
            if type(age) is not int or not before - COMMIT_TIME_UTC <= age <= after - COMMIT_TIME_UTC:
                raise RuntimeError(f"invalid commit age: {age}")
            item["commit"]["age_secs"] = 0
        actual[name] = item
    expected = {}
    for i, path in enumerate(paths):
        state = i % 4
        expected[path.name] = {
            "name": path.name,
            "branch": path.name,
            "path": path.as_posix(),
            "is_current": i == 0,
            "dirty": {
                "staged": state == 1,
                "modified": state == 2,
                "untracked": state == 3,
            },
            "upstream": None,
            "main_relationship": (
                "is_default" if short or i == 0 else "same_commit"
            ),
            "operation": None,
            "commit": (
                None if short else {"age_secs": 0, "message": "benchmark base"}
            ),
            "line_diff_head": (
                None
                if short
                else {
                    "insertions": int(state in (1, 2)),
                    "deletions": int(state in (1, 2)),
                }
            ),
            "line_diff_main": (
                None if short or i == 0 else {"insertions": 0, "deletions": 0}
            ),
            "is_locked": False,
            "is_prunable": False,
            "is_orphan": False,
            "is_merged": False,
        }
    if actual != expected:
        raise RuntimeError(
            "fixture JSON mismatch:\n"
            + json.dumps({"expected": expected, "actual": actual}, indent=2)
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--worktrees", type=bounded(32), default=8)
    parser.add_argument("--iterations", type=bounded(100), default=5)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if binary.stem != "benchmark-list":
        parser.error("--binary must point to benchmark-list, not the normal CLI")
    with tempfile.TemporaryDirectory(prefix="lazywt-list-bench-") as directory:
        root = Path(directory).resolve()
        env = isolated_env(root)
        paths = fixture(root, args.worktrees, env)
        for short in (False, True):
            command = [binary, "list", "--json"]
            if short:
                command.append("--short")
            # Exact oracle doubles as warm-up, before any timed sample.
            before = int(time.time())
            warmup = run(command, paths[0], env)
            after = int(time.time())
            validate(warmup, paths, short, before, after)
            samples = []
            for _ in range(args.iterations):
                before = int(time.time())
                start = time.perf_counter_ns()
                output = run(command, paths[0], env)
                samples.append((time.perf_counter_ns() - start) / 1_000_000)
                after = int(time.time())
                # Outside the timing: state/content must stay equivalent.
                validate(output, paths, short, before, after)
            print(
                f"mode={'short-json' if short else 'rich-json'} "
                f"worktrees={len(paths)} iterations={len(samples)} "
                f"median_ms={statistics.median(samples):.3f} "
                f"min_ms={min(samples):.3f} max_ms={max(samples):.3f}"
            )
    if root.exists():
        raise RuntimeError(f"fixture cleanup failed: {root}")
    print("fixture_cleanup=ok")


if __name__ == "__main__":
    main()
