use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the project root (where Cargo.toml lives).
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Returns the dedicated workspace directory for E2E workflow testing.
fn workflow_dir() -> PathBuf {
    project_root().join("testing_workflow")
}

/// Run lazywt with the given arguments in `cwd`, with environment isolation.
fn lazywt(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lazywt"))
        .args(args)
        .current_dir(cwd)
        .env("WT_AUTO_UPDATE", "false")
        .env("WT_AUTO_NAVIGATE", "false")
        .env("WT_BRANCH_PREFIX", "")
        .env("WT_RENAME", "wt")
        .env("LAZYWT_HOME", workflow_dir().to_str().unwrap())
        .output()
        .expect("failed to run lazywt")
}

/// Run git with the given arguments in `cwd`.
fn git(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to run git")
}

/// Extract stdout as a String.
fn stdout_str(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Extract stderr as a String.
fn stderr_str(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Assert the command succeeded, printing context on failure.
fn assert_success(output: &std::process::Output, context: &str) {
    assert!(
        output.status.success(),
        "{} failed (exit {:?})\nstdout: {}\nstderr: {}",
        context,
        output.status.code(),
        stdout_str(output),
        stderr_str(output),
    );
}

/// Assert the command failed, printing context on unexpected success.
#[allow(dead_code)]
fn assert_failure(output: &std::process::Output, context: &str) {
    assert!(
        !output.status.success(),
        "{} unexpectedly succeeded\nstdout: {}\nstderr: {}",
        context,
        stdout_str(output),
        stderr_str(output),
    );
}

/// Assert a directory is a bare git repository.
fn assert_is_bare_repo(path: &Path) {
    let out = Command::new("git")
        .args(["rev-parse", "--is-bare-repository"])
        .current_dir(path)
        .output()
        .expect("git rev-parse failed");
    let val = stdout_str(&out).trim().to_string();
    assert_eq!(val, "true", "expected bare repo at {}", path.display());
}

/// Assert a branch exists in the bare repo.
fn assert_branch_exists(bare_dir: &Path, branch: &str) {
    let out = Command::new("git")
        .arg("-C")
        .arg(bare_dir)
        .args(["branch", "--list", branch])
        .output()
        .expect("git branch --list failed");
    let text = stdout_str(&out);
    assert!(
        text.contains(branch.rsplit('/').last().unwrap_or(branch)),
        "branch '{}' not found in {}.\ngit branch output: {}",
        branch,
        bare_dir.display(),
        text,
    );
}

/// Assert a worktree with the given name is registered in the bare repo.
fn assert_worktree_exists(bare_dir: &Path, name: &str) {
    let out = Command::new("git")
        .arg("-C")
        .arg(bare_dir)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .expect("git worktree list failed");
    let text = stdout_str(&out);
    assert!(
        text.contains(name),
        "worktree '{}' not found in git worktree list.\noutput: {}",
        name,
        text,
    );
}

/// Assert the bare repo has exactly `expected` worktrees (including the bare root entry).
#[allow(dead_code)]
fn assert_worktree_count(bare_dir: &Path, expected: usize) {
    let out = Command::new("git")
        .arg("-C")
        .arg(bare_dir)
        .args(["worktree", "list"])
        .output()
        .expect("git worktree list failed");
    let text = stdout_str(&out);
    let count = text.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(
        count, expected,
        "expected {} worktrees, got {} in {}.\noutput: {}",
        expected,
        count,
        bare_dir.display(),
        text,
    );
}

/// Remove and recreate the workspace directory, retrying on Windows lock errors.
fn ensure_clean_workspace(base: &Path) {
    if base.exists() {
        for attempt in 0..3 {
            if std::fs::remove_dir_all(base).is_ok() {
                break;
            }
            eprintln!(
                "ensure_clean_workspace: remove_dir_all attempt {} failed, retrying...",
                attempt + 1
            );
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    std::fs::create_dir_all(base).expect("failed to create testing_workflow");
}

#[test]
fn e2e_full_workflow() {
    let base = workflow_dir();
    ensure_clean_workspace(&base);

    // === Phase 1: Clone from a local remote ===
    println!("=== Phase 1: Clone from a local remote ===");

    // Create a local bare repo to act as the "origin" remote. This keeps the E2E
    // test fully offline -- CI runners cannot clone github.com without credentials,
    // and a network dependency makes the test flaky regardless.
    let origin_remote = base.join("origin.git");
    let out = git(&base, &["init", "--bare", origin_remote.to_str().unwrap()]);
    assert_success(&out, "git init --bare origin remote");

    // Seed the remote with an initial commit on `main` via a temporary worktree.
    let seed_wt = base.join("origin-seed");
    let out = git(
        &origin_remote,
        &["worktree", "add", seed_wt.to_str().unwrap(), "-b", "main"],
    );
    assert_success(&out, "git worktree add origin-seed");

    git(&seed_wt, &["config", "user.email", "e2e-test@lazywt.dev"]);
    git(&seed_wt, &["config", "user.name", "E2E Test"]);
    std::fs::write(seed_wt.join("README.md"), "# lazywt e2e seed\n")
        .expect("failed to write seed README.md");
    let out = git(&seed_wt, &["add", "README.md"]);
    assert_success(&out, "git add seed README.md");
    let out = git(&seed_wt, &["commit", "-m", "initial commit"]);
    assert_success(&out, "git commit in origin-seed");

    // Point the remote's HEAD at main and drop the temporary worktree.
    let out = git(&origin_remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    assert_success(&out, "git symbolic-ref HEAD on origin");
    std::env::set_current_dir(&base).ok();
    let out = git(
        &origin_remote,
        &["worktree", "remove", seed_wt.to_str().unwrap(), "--force"],
    );
    assert_success(&out, "git worktree remove origin-seed");

    // Clone from the local remote (offline, no credentials needed).
    let out = git(&base, &["clone", origin_remote.to_str().unwrap(), "lazywt-clone"]);
    assert_success(&out, "git clone");

    let clone_dir = base.join("lazywt-clone");
    assert!(clone_dir.exists() && clone_dir.is_dir(), "clone_dir must exist");
    assert!(clone_dir.join(".git").exists(), "must be a regular clone with .git");

    let out = git(&clone_dir, &["log", "--oneline", "-1"]);
    assert_success(&out, "git log in clone");

    // Configure git user identity for later commits
    git(&clone_dir, &["config", "user.email", "e2e-test@lazywt.dev"]);
    git(&clone_dir, &["config", "user.name", "E2E Test"]);

    // === Phase 2: Setup as bare repo ===
    println!("=== Phase 2: Setup as bare repo ===");

    let out = lazywt(&clone_dir, &["setup", "--yes"]);
    assert_success(&out, "lazywt setup --yes");
    assert!(
        stdout_str(&out).contains("Setup: Convert to bare repository"),
        "setup output should contain header. stdout: {}",
        stdout_str(&out),
    );

    // After setup: .git becomes bare, main worktree created
    let bare_dir = clone_dir.join(".git");
    let main_wt = clone_dir.join("main");

    assert_is_bare_repo(&bare_dir);
    assert!(main_wt.exists() && main_wt.is_dir(), "main worktree must exist");

    // Verify layout config
    let out = git(&bare_dir, &["config", "wt.layout"]);
    assert!(
        stdout_str(&out).contains("modern"),
        "wt.layout should be modern. got: {}",
        stdout_str(&out),
    );

    // Note: fetch refspec after bare clone may be narrow; fix-fetch corrects it in Phase 8.

    // Configure git user in main worktree (needed for Phase 6 commits)
    git(&main_wt, &["config", "user.email", "e2e-test@lazywt.dev"]);
    git(&main_wt, &["config", "user.name", "E2E Test"]);

    // === Phase 3: List (baseline) ===
    println!("=== Phase 3: List (baseline) ===");

    let out = lazywt(&main_wt, &["list"]);
    assert_success(&out, "lazywt list");
    assert!(
        stdout_str(&out).contains("main"),
        "list should contain 'main'. stdout: {}",
        stdout_str(&out),
    );

    let out = lazywt(&main_wt, &["list", "--short"]);
    assert_success(&out, "lazywt list --short");
    assert!(
        stdout_str(&out).contains("main"),
        "list --short should contain 'main'. stdout: {}",
        stdout_str(&out),
    );

    let out = lazywt(&main_wt, &["list", "--json"]);
    assert_success(&out, "lazywt list --json");
    let json: serde_json::Value =
        serde_json::from_str(stdout_str(&out).trim()).expect("list --json should be valid JSON");
    let arr = json.as_array().expect("JSON output should be an array");
    assert_eq!(arr.len(), 1, "baseline should have 1 worktree (main)");

    let out = lazywt(&main_wt, &["list", "--no-color"]);
    assert_success(&out, "lazywt list --no-color");
    assert!(
        !stdout_str(&out).contains("\x1b["),
        "no-color output should not contain ANSI escape codes",
    );

    // === Phase 4: Add worktrees ===
    println!("=== Phase 4: Add worktrees ===");

    let out = lazywt(&main_wt, &["add", "feature-1"]);
    assert_success(&out, "lazywt add feature-1");
    assert!(
        stdout_str(&out).contains("Adding worktree 'feature-1'"),
        "add output should mention feature-1. stdout: {}",
        stdout_str(&out),
    );

    let feature1_wt = clone_dir.join("feature-1");
    assert!(feature1_wt.exists(), "feature-1 worktree directory must exist");
    assert_branch_exists(&bare_dir, "feature-1");
    assert_worktree_exists(&bare_dir, "feature-1");

    let out = lazywt(&main_wt, &["add", "feature-2", "--branch", "custom/branch-name"]);
    assert_success(&out, "lazywt add feature-2 --branch custom/branch-name");

    let feature2_wt = clone_dir.join("feature-2");
    assert!(feature2_wt.exists(), "feature-2 worktree directory must exist");
    assert_branch_exists(&bare_dir, "custom/branch-name");

    let out = lazywt(&main_wt, &["add", "feature-3", "--from", "feature-1"]);
    assert_success(&out, "lazywt add feature-3 --from feature-1");

    let feature3_wt = clone_dir.join("feature-3");
    assert!(feature3_wt.exists(), "feature-3 worktree directory must exist");

    // Verify feature-3 starts from the same commit as feature-1
    let f1_head = stdout_str(&git(&feature1_wt, &["rev-parse", "HEAD"])).trim().to_string();
    let f3_head = stdout_str(&git(&feature3_wt, &["rev-parse", "HEAD"])).trim().to_string();
    assert_eq!(f1_head, f3_head, "feature-3 should start from feature-1's HEAD");

    // === Phase 5: List (with worktrees) ===
    println!("=== Phase 5: List (with worktrees) ===");

    let out = lazywt(&main_wt, &["list"]);
    assert_success(&out, "lazywt list after add");
    let list_out = stdout_str(&out);
    for name in &["main", "feature-1", "feature-2", "feature-3"] {
        assert!(
            list_out.contains(name),
            "list should contain '{}'. stdout: {}",
            name,
            list_out,
        );
    }

    let out = lazywt(&main_wt, &["list", "--json"]);
    assert_success(&out, "lazywt list --json after add");
    let json: serde_json::Value =
        serde_json::from_str(stdout_str(&out).trim()).expect("list --json should be valid JSON");
    let arr = json.as_array().expect("JSON output should be an array");
    assert_eq!(arr.len(), 4, "should have 4 worktrees after adding 3");

    // === Phase 6: Work in worktrees (create git state) ===
    println!("=== Phase 6: Work in worktrees (create git state) ===");

    // Create and commit a file in feature-1
    std::fs::write(feature1_wt.join("test-file.txt"), "e2e test content")
        .expect("failed to write test-file.txt");
    // Configure git user for feature-1 worktree
    git(&feature1_wt, &["config", "user.email", "e2e-test@lazywt.dev"]);
    git(&feature1_wt, &["config", "user.name", "E2E Test"]);
    let out = git(&feature1_wt, &["add", "test-file.txt"]);
    assert_success(&out, "git add in feature-1");
    let out = git(&feature1_wt, &["commit", "-m", "add test file in feature-1"]);
    assert_success(&out, "git commit in feature-1");

    // Create a dirty file in feature-2 (do NOT commit -- for later prune testing)
    std::fs::write(feature2_wt.join("dirty-file.txt"), "uncommitted")
        .expect("failed to write dirty-file.txt");

    // Merge feature-1 into main (makes feature-1 "merged" for prune testing)
    let out = git(&main_wt, &["merge", "feature-1", "--no-edit"]);
    assert_success(&out, "git merge feature-1 into main");

    // === Phase 7: Switch ===
    println!("=== Phase 7: Switch ===");

    // Test exact match switch to feature-1
    let out = lazywt(&main_wt, &["switch", "feature-1"]);
    assert_success(&out, "lazywt switch feature-1 (exact match)");

    // Check nav file was written with feature-1 path
    let nav_file = workflow_dir().join(".lazywt_cd");
    if nav_file.exists() {
        let nav_content = std::fs::read_to_string(&nav_file).unwrap_or_default();
        assert!(
            nav_content.contains("feature-1"),
            "nav file should contain feature-1 path. got: {}",
            nav_content,
        );
    }

    // Test exact match switch to feature-2
    let out = lazywt(&feature1_wt, &["switch", "feature-2"]);
    assert_success(&out, "lazywt switch feature-2 (exact match)");

    // Test exact match switch to feature-3
    let out = lazywt(&main_wt, &["switch", "feature-3"]);
    assert_success(&out, "lazywt switch feature-3 (exact match)");

    // === Phase 8: Fix-fetch ===
    println!("=== Phase 8: Fix-fetch ===");

    // Deliberately corrupt the fetch refspec
    git(
        &bare_dir,
        &["config", "remote.origin.fetch", "+refs/heads/main:refs/remotes/origin/main"],
    );

    let out = lazywt(&main_wt, &["fix-fetch"]);
    assert_success(&out, "lazywt fix-fetch");
    assert!(
        stdout_str(&out).contains("Fetch refspec fixed successfully"),
        "fix-fetch should report success. stdout: {}",
        stdout_str(&out),
    );

    // Verify the refspec was restored
    let out = git(&bare_dir, &["config", "remote.origin.fetch"]);
    assert!(
        stdout_str(&out).contains("+refs/heads/*:refs/remotes/origin/*"),
        "fetch refspec should be restored to wildcard. got: {}",
        stdout_str(&out),
    );

    // Run fix-fetch again on already-correct repo
    let out = lazywt(&main_wt, &["fix-fetch"]);
    assert_success(&out, "lazywt fix-fetch (already correct)");
    assert!(
        stdout_str(&out).contains("already correctly configured"),
        "fix-fetch should say already configured. stdout: {}",
        stdout_str(&out),
    );

    // === Phase 9: Sync ===
    println!("=== Phase 9: Sync ===");

    let out = lazywt(&main_wt, &["sync"]);
    assert_success(&out, "lazywt sync");
    let sync_out = stdout_str(&out);
    assert!(
        sync_out.contains("Syncing worktrees"),
        "sync should contain header. stdout: {}",
        sync_out,
    );
    assert!(
        sync_out.contains("Fetched all remotes"),
        "sync should confirm fetch. stdout: {}",
        sync_out,
    );
    assert!(
        sync_out.contains("Sync complete"),
        "sync should report complete. stdout: {}",
        sync_out,
    );

    let out = lazywt(&main_wt, &["sync", "--all"]);
    assert_success(&out, "lazywt sync --all");
    assert!(
        stdout_str(&out).contains("Sync complete"),
        "sync --all should report complete. stdout: {}",
        stdout_str(&out),
    );

    // === Phase 10: Prune ===
    println!("=== Phase 10: Prune ===");

    // State from Phase 6:
    // - feature-1: merged into main (but SHA = main HEAD after merge, so prune's false-positive
    //   filter may skip it since its tip equals default_sha)
    // - feature-2: dirty uncommitted file, branch "custom/branch-name", created from main
    //   (SHA may equal main's pre-merge SHA -- false-positive filter may apply)
    // - feature-3: created --from feature-1, so it's at feature-1's pre-merge commit.
    //   After merge, feature-3 is reachable from main (merged) and its SHA != main HEAD.
    //   This makes feature-3 a genuine prune candidate.
    //
    // The prune logic uses git branch --merged + a false-positive filter (skip if branch SHA
    // == default SHA). The actual candidates depend on the exact commit graph.

    // Run prune --yes
    let out = lazywt(&main_wt, &["prune", "--yes"]);
    assert_success(&out, "lazywt prune --yes");
    let prune_out = stdout_str(&out);
    println!("Prune output: {}", prune_out);

    // At least one worktree should be pruned (feature-3 is a confirmed merged candidate)
    assert!(
        prune_out.contains("Pruned") || prune_out.contains("Removed"),
        "prune should report pruned worktrees. stdout: {}",
        prune_out,
    );
    assert!(
        prune_out.contains("merged"),
        "prune should mention 'merged' reason. stdout: {}",
        prune_out,
    );

    // feature-3 should be pruned (merged, clean, SHA != main)
    assert!(!feature3_wt.exists(), "feature-3 should be removed after prune (merged)");

    // Run prune with --force --yes to handle any remaining dirty+merged candidates
    let out = lazywt(&main_wt, &["prune", "--force", "--yes"]);
    assert_success(&out, "lazywt prune --force --yes");
    let force_prune_out = stdout_str(&out);
    println!("Force prune output: {}", force_prune_out);

    // Check what remains via git worktree list
    let out = git(&bare_dir, &["worktree", "list"]);
    let wt_list_out = stdout_str(&out);
    println!("Worktree list after force prune: {}", wt_list_out);

    // Clean up any remaining non-main worktrees for the next phases
    if feature1_wt.exists() {
        let out = lazywt(&main_wt, &["remove", "feature-1", "--yes"]);
        assert_success(&out, "lazywt remove feature-1 cleanup");
    }
    if feature2_wt.exists() {
        let out = lazywt(&main_wt, &["remove", "feature-2", "--yes"]);
        assert_success(&out, "lazywt remove feature-2 cleanup");
    }

    // === Phase 11: Edge cases ===
    println!("=== Phase 11: Edge cases ===");

    // 11a. Orphan detection
    println!("  --- 11a: Orphan detection ---");
    let out = lazywt(&main_wt, &["add", "orphan-test"]);
    assert_success(&out, "lazywt add orphan-test");
    let orphan_wt = clone_dir.join("orphan-test");
    assert!(orphan_wt.exists(), "orphan-test worktree should exist");

    // Manually delete the worktree directory to create an orphan
    // First move CWD away from the orphan dir to avoid Windows lock
    std::env::set_current_dir(&base).ok();
    std::fs::remove_dir_all(&orphan_wt).expect("failed to delete orphan worktree dir");
    assert!(!orphan_wt.exists(), "orphan-test directory should be deleted");

    // Verify orphaned entry still appears in git worktree list
    let out = git(&bare_dir, &["worktree", "list"]);
    assert!(
        stdout_str(&out).contains("orphan-test"),
        "orphaned worktree should still appear in git worktree list. output: {}",
        stdout_str(&out),
    );

    // Run prune to detect and clean the orphan
    let out = lazywt(&main_wt, &["prune", "--yes"]);
    assert_success(&out, "lazywt prune --yes (orphan detection)");
    let prune_orphan_out = stdout_str(&out);
    assert!(
        prune_orphan_out.contains("orphan") || prune_orphan_out.contains("orphan-test"),
        "prune should detect orphan. stdout: {}",
        prune_orphan_out,
    );

    // Verify orphan-test no longer appears in git worktree list
    let out = git(&bare_dir, &["worktree", "list", "--porcelain"]);
    assert!(
        !stdout_str(&out).contains("orphan-test"),
        "orphan-test should no longer appear in git worktree list. output: {}",
        stdout_str(&out),
    );

    // 11b. Default worktree protection
    println!("  --- 11b: Default worktree protection ---");
    let out = lazywt(&main_wt, &["remove", "main", "--yes"]);
    assert_failure(&out, "lazywt remove main --yes (should fail)");
    let remove_out = stdout_str(&out).to_lowercase() + &stderr_str(&out).to_lowercase();
    assert!(
        remove_out.contains("cannot remove default") || remove_out.contains("default branch"),
        "removing default worktree should produce error about default. output: {}",
        remove_out,
    );

    // 11c. remove-all
    println!("  --- 11c: remove-all ---");
    // Add a couple temp worktrees
    let out = lazywt(&main_wt, &["add", "temp-1"]);
    assert_success(&out, "lazywt add temp-1");
    let out = lazywt(&main_wt, &["add", "temp-2"]);
    assert_success(&out, "lazywt add temp-2");

    // Verify both exist
    assert!(clone_dir.join("temp-1").exists(), "temp-1 should exist");
    assert!(clone_dir.join("temp-2").exists(), "temp-2 should exist");

    // remove-all --yes
    let out = lazywt(&main_wt, &["remove-all", "--yes"]);
    assert_success(&out, "lazywt remove-all --yes");
    let ra_out = stdout_str(&out);
    assert!(
        ra_out.contains("worktrees removed") || ra_out.contains("Removed"),
        "remove-all should report removal. stdout: {}",
        ra_out,
    );

    // Verify only main worktree remains
    assert!(!clone_dir.join("temp-1").exists(), "temp-1 should be removed");
    assert!(!clone_dir.join("temp-2").exists(), "temp-2 should be removed");
    assert!(main_wt.exists(), "main worktree should still exist");

    // Verify worktree count: bare entry + main = 2 lines in git worktree list
    assert_worktree_count(&bare_dir, 2);

    // 11d. remove-all with nothing to remove
    println!("  --- 11d: remove-all (nothing to remove) ---");
    let out = lazywt(&main_wt, &["remove-all", "--yes"]);
    assert_success(&out, "lazywt remove-all --yes (nothing to remove)");
    assert!(
        stdout_str(&out).contains("No worktrees to remove"),
        "remove-all should say nothing to remove. stdout: {}",
        stdout_str(&out),
    );

    // 11e. Setup on already-bare repo
    println!("  --- 11e: Setup on already-bare repo ---");
    let out = lazywt(&bare_dir, &["setup", "--yes"]);
    assert_failure(&out, "lazywt setup --yes on bare repo (should fail)");
    let setup_out = stdout_str(&out).to_lowercase() + &stderr_str(&out).to_lowercase();
    assert!(
        setup_out.contains("already bare") || setup_out.contains("not a regular"),
        "setup on bare repo should report error. output: {}",
        setup_out,
    );

    // === Phase 12: Second lifecycle with --bare-dir ===
    println!("=== Phase 12: Second lifecycle with --bare-dir ===");

    // Create a local bare repo as a "remote" (avoids network clone)
    let second_remote = base.join("second-remote.git");
    let out = git(&base, &["init", "--bare", second_remote.to_str().unwrap()]);
    assert_success(&out, "git init --bare second-remote");

    // Create a temp worktree, configure git user, make initial commit, point HEAD to main
    let tmp_wt = base.join("tmp-wt-for-remote");
    let out = git(
        &second_remote,
        &["worktree", "add", tmp_wt.to_str().unwrap(), "-b", "main"],
    );
    assert_success(&out, "git worktree add tmp-wt");

    git(&tmp_wt, &["config", "user.email", "e2e-test@lazywt.dev"]);
    git(&tmp_wt, &["config", "user.name", "E2E Test"]);
    let out = git(&tmp_wt, &["commit", "--allow-empty", "-m", "initial commit"]);
    assert_success(&out, "git commit in tmp-wt");

    let out = git(
        &second_remote,
        &["symbolic-ref", "HEAD", "refs/heads/main"],
    );
    assert_success(&out, "git symbolic-ref HEAD");

    // Remove the temp worktree
    // Move CWD away first (Windows lock)
    std::env::set_current_dir(&base).ok();
    let out = git(
        &second_remote,
        &["worktree", "remove", tmp_wt.to_str().unwrap(), "--force"],
    );
    assert_success(&out, "git worktree remove tmp-wt");

    // Clone with --bare-dir .repo
    let out = lazywt(
        &base,
        &["clone", second_remote.to_str().unwrap(), "second-test", "--bare-dir", ".repo"],
    );
    assert_success(&out, "lazywt clone --bare-dir .repo");

    let second_dir = base.join("second-test");
    let second_bare = second_dir.join(".repo");

    // Verify .repo exists and is a bare repo
    assert!(second_bare.exists(), ".repo directory should exist");
    assert_is_bare_repo(&second_bare);

    // Verify wt.baredir is set correctly
    let out = git(&second_bare, &["config", "wt.baredir"]);
    assert!(
        stdout_str(&out).trim() == ".repo",
        "wt.baredir should be .repo. got: {}",
        stdout_str(&out).trim(),
    );

    // Run list --json from the main worktree
    let second_main_wt = second_dir.join("main");
    let out = lazywt(&second_main_wt, &["list", "--json"]);
    assert_success(&out, "lazywt list --json in second lifecycle");
    let json: serde_json::Value =
        serde_json::from_str(stdout_str(&out).trim()).expect("list --json should be valid JSON");
    assert!(json.is_array(), "list --json should return an array");

    // Add a worktree in the second lifecycle
    let out = lazywt(&second_main_wt, &["add", "test-wt"]);
    assert_success(&out, "lazywt add test-wt in second lifecycle");
    assert!(second_dir.join("test-wt").exists(), "test-wt worktree should exist");

    // Remove the worktree
    let out = lazywt(&second_main_wt, &["remove", "test-wt", "--yes"]);
    assert_success(&out, "lazywt remove test-wt in second lifecycle");
    assert!(!second_dir.join("test-wt").exists(), "test-wt should be removed");

    // === Phase 13: Migrate (classic to modern layout) ===
    println!("=== Phase 13: Migrate (classic to modern layout) ===");

    // Create a classic layout repo manually
    let migrate_dir = base.join("migrate-test");
    std::fs::create_dir_all(&migrate_dir).unwrap();
    let migrate_bare = migrate_dir.join(".bare");

    // Initialize bare repo
    let out = git(&base, &["init", "--bare", migrate_bare.to_str().unwrap()]);
    assert_success(&out, "git init --bare migrate-test/.bare");

    // Set classic layout config
    git(&migrate_bare, &["config", "wt.layout", "classic"]);
    git(
        &migrate_bare,
        &["config", "remote.origin.url", "https://example.com/test.git"],
    );

    // Create trees directory and add main worktree
    let trees_dir = migrate_bare.join("trees");
    std::fs::create_dir_all(&trees_dir).unwrap();

    let migrate_main_wt = trees_dir.join("main");
    let out = git(
        &migrate_bare,
        &[
            "worktree",
            "add",
            migrate_main_wt.to_str().unwrap(),
            "-b",
            "main",
        ],
    );
    assert_success(&out, "git worktree add main in migrate-test");

    // Configure git user and make initial commit
    git(
        &migrate_main_wt,
        &["config", "user.email", "e2e-test@lazywt.dev"],
    );
    git(&migrate_main_wt, &["config", "user.name", "E2E Test"]);
    let out = git(
        &migrate_main_wt,
        &["commit", "--allow-empty", "-m", "initial commit for migration test"],
    );
    assert_success(&out, "git commit in migrate-test/main");

    // Point HEAD to main
    git(
        &migrate_bare,
        &["symbolic-ref", "HEAD", "refs/heads/main"],
    );

    // Test dry run
    let out = lazywt(&migrate_main_wt, &["migrate", "--dry-run", "--yes"]);
    assert_success(&out, "lazywt migrate --dry-run --yes");
    let dry_out = stdout_str(&out);
    assert!(
        dry_out.contains("Dry run") || dry_out.contains("dry run"),
        "dry run should mention 'dry run'. stdout: {}",
        dry_out,
    );

    // Verify no changes were made (classic layout still intact)
    assert!(migrate_bare.exists(), ".bare should still exist after dry run");
    assert!(migrate_main_wt.exists(), "trees/main should still exist after dry run");

    // Execute actual migration
    let out = lazywt(&migrate_main_wt, &["migrate", "--yes"]);
    assert_success(&out, "lazywt migrate --yes");
    let migrate_out = stdout_str(&out);
    assert!(
        migrate_out.contains("Migration complete") || migrate_out.contains("modern layout"),
        "migrate should report success. stdout: {}",
        migrate_out,
    );

    // Verify modern layout: bare repo moved to .git, worktrees are siblings
    let migrate_new_bare = migrate_dir.join(".git");
    let migrate_new_main = migrate_dir.join("main");

    assert!(
        migrate_new_bare.exists(),
        "migrate_dir/.git should exist after migration"
    );
    assert!(
        migrate_new_main.exists(),
        "migrate_dir/main should exist after migration"
    );

    // Verify wt.layout is now modern
    let out = git(&migrate_new_bare, &["config", "wt.layout"]);
    assert!(
        stdout_str(&out).contains("modern"),
        "wt.layout should be modern after migration. got: {}",
        stdout_str(&out),
    );

    // === Phase 14: Cleanup ===
    println!("=== Phase 14: Cleanup ===");

    // Move CWD away from testing_workflow before cleanup
    std::env::set_current_dir(project_root()).ok();

    if base.exists() {
        for _ in 0..5 {
            if std::fs::remove_dir_all(&base).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    assert!(!base.exists(), "testing_workflow should be deleted after test");
}
