use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// Create a bare git repository with modern layout for testing.
/// Returns (TempDir, PathBuf to bare .git dir, PathBuf to project root).
pub fn create_modern_bare_repo() -> (TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("test-project");
    std::fs::create_dir_all(&root).unwrap();
    let git_dir = root.join(".git");

    // Initialize bare repo
    Command::new("git")
        .args(["init", "--bare"])
        .arg(&git_dir)
        .output()
        .expect("git init --bare failed");

    // Configure remote origin URL (needed for fetch operations)
    Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["config", "remote.origin.url", "https://example.com/test.git"])
        .output()
        .expect("git config failed");

    // Create a main worktree with a new branch.
    // Use -b instead of --orphan for broader git version compatibility.
    // On an empty bare repo, git infers --orphan automatically.
    let main_wt = root.join("main");
    Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args([
            "worktree",
            "add",
            main_wt.to_str().unwrap(),
            "-b",
            "main",
        ])
        .output()
        .expect("git worktree add failed");

    // Configure git user for commits (needed in CI and fresh environments)
    Command::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("git config user.email failed");

    Command::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .expect("git config user.name failed");

    Command::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "initial"])
        .output()
        .expect("git commit failed");

    // Point the bare repo's HEAD to main so that new worktrees can be
    // created from it (the default HEAD may point to master).
    Command::new("git")
        .arg("-C")
        .arg(&git_dir)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .output()
        .expect("git symbolic-ref HEAD failed");

    (dir, git_dir, root)
}

/// Create a bare git repository with classic layout for testing migration.
/// Classic layout: container/.bare is the bare repo, worktrees in container/.bare/trees/<name>.
/// Returns (TempDir, PathBuf to bare .bare dir, PathBuf to container root).
pub fn create_classic_bare_repo() -> (TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let container = dir.path().join("test-project");
    std::fs::create_dir_all(&container).unwrap();
    let bare_dir = container.join(".bare");

    // Initialize bare repo as .bare
    Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare_dir)
        .output()
        .expect("git init --bare failed");

    // Set layout config to classic
    Command::new("git")
        .arg("-C")
        .arg(&bare_dir)
        .args(["config", "wt.layout", "classic"])
        .output()
        .expect("git config wt.layout failed");

    // Configure remote origin URL
    Command::new("git")
        .arg("-C")
        .arg(&bare_dir)
        .args(["config", "remote.origin.url", "https://example.com/test.git"])
        .output()
        .expect("git config failed");

    // Create trees directory for worktrees
    let trees_dir = bare_dir.join("trees");
    std::fs::create_dir_all(&trees_dir).unwrap();

    // Create a main worktree inside trees/
    let main_wt = trees_dir.join("main");
    Command::new("git")
        .arg("-C")
        .arg(&bare_dir)
        .args([
            "worktree",
            "add",
            main_wt.to_str().unwrap(),
            "-b",
            "main",
        ])
        .output()
        .expect("git worktree add main failed");

    // Configure git user for commits
    Command::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("git config user.email failed");

    Command::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["config", "user.name", "Test User"])
        .output()
        .expect("git config user.name failed");

    Command::new("git")
        .arg("-C")
        .arg(&main_wt)
        .args(["commit", "--allow-empty", "-m", "initial"])
        .output()
        .expect("git commit failed");

    // Point HEAD to main
    Command::new("git")
        .arg("-C")
        .arg(&bare_dir)
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .output()
        .expect("git symbolic-ref HEAD failed");

    (dir, bare_dir, container)
}

/// Add a worktree inside the classic trees/ directory.
pub fn add_classic_worktree(git_dir: &Path, name: &str, branch: &str) -> PathBuf {
    let trees_dir = git_dir.join("trees");
    let wt_path = trees_dir.join(name);
    Command::new("git")
        .arg("-C")
        .arg(git_dir)
        .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", branch])
        .output()
        .expect("git worktree add failed");
    wt_path
}

/// Add a worktree with a new branch to an existing bare repo.
pub fn add_worktree(git_dir: &Path, parent: &Path, name: &str, branch: &str) -> PathBuf {
    let wt_path = parent.join(name);
    Command::new("git")
        .arg("-C")
        .arg(git_dir)
        .args(["worktree", "add", wt_path.to_str().unwrap(), "-b", branch])
        .output()
        .expect("git worktree add failed");
    wt_path
}
