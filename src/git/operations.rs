use std::fs;
use std::path::{Path, PathBuf};

/// Represents an active git operation in progress for a worktree.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveOperation {
    None,
    Rebase,
    Merge,
}

/// Resolve the actual git directory for a worktree path.
///
/// For linked worktrees, `.git` is a file containing `gitdir: <path>`.
/// For regular repos/bare clones used as worktree roots, `.git` may be a directory.
/// Returns None if the path doesn't exist or has no `.git` entry.
fn resolve_git_dir(worktree_path: &Path) -> Option<PathBuf> {
    if !worktree_path.exists() {
        return None;
    }

    let dot_git = worktree_path.join(".git");

    if dot_git.is_dir() {
        // Standard repo or bare clone — the .git directory is the git dir itself
        return Some(dot_git);
    }

    if dot_git.is_file() {
        // Linked worktree: read the gitdir pointer
        let contents = fs::read_to_string(&dot_git).ok()?;
        let pointer = contents
            .lines()
            .find_map(|line| line.strip_prefix("gitdir: "))?;
        let pointer = pointer.trim();
        // The pointer is relative to the worktree path
        let git_dir = if Path::new(pointer).is_absolute() {
            PathBuf::from(pointer)
        } else {
            worktree_path.join(pointer)
        };
        // Normalize the path without requiring it to exist on disk via dunce
        let resolved = dunce::canonicalize(&git_dir).unwrap_or(git_dir);
        return Some(resolved);
    }

    None
}

/// Detect whether a rebase or merge is currently in progress in the given worktree.
///
/// Resolution order:
/// 1. Checks for `rebase-merge/` or `rebase-apply/` directories → Rebase
/// 2. Checks for `MERGE_HEAD` file → Merge
/// 3. Rebase takes priority if both are present
/// 4. Returns `None` if the path doesn't exist or `.git` is missing
pub fn detect_operation(worktree_path: &Path) -> ActiveOperation {
    let git_dir = match resolve_git_dir(worktree_path) {
        Some(d) => d,
        None => return ActiveOperation::None,
    };

    let rebase = git_dir.join("rebase-merge").is_dir()
        || git_dir.join("rebase-apply").is_dir();

    let merge = git_dir.join("MERGE_HEAD").is_file();

    // Rebase takes priority over merge
    if rebase {
        ActiveOperation::Rebase
    } else if merge {
        ActiveOperation::Merge
    } else {
        ActiveOperation::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a minimal fake gitdir at `path` (just a directory with a HEAD file).
    fn make_gitdir(path: &Path) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    }

    /// Create a worktree with a real `.git` directory (not a linked worktree).
    fn setup_real_git_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        let git_dir = dir.path().join(".git");
        make_gitdir(&git_dir);
        dir
    }

    /// Create a linked worktree: a directory with a `.git` file pointing to a separate gitdir.
    fn setup_linked_worktree() -> (TempDir, TempDir) {
        let gitdir_parent = TempDir::new().unwrap();
        let actual_gitdir = gitdir_parent.path().join("worktrees").join("feature");
        make_gitdir(&actual_gitdir);

        let worktree_dir = TempDir::new().unwrap();
        let git_file_content = format!("gitdir: {}\n", actual_gitdir.display());
        fs::write(worktree_dir.path().join(".git"), git_file_content).unwrap();

        (worktree_dir, gitdir_parent)
    }

    #[test]
    fn no_operation_on_clean_linked_worktree() {
        let (wt, _gitdir) = setup_linked_worktree();
        assert_eq!(detect_operation(wt.path()), ActiveOperation::None);
    }

    #[test]
    fn rebase_merge_directory_returns_rebase() {
        let (wt, gitdir) = setup_linked_worktree();
        let actual_gitdir = gitdir.path().join("worktrees").join("feature");
        fs::create_dir_all(actual_gitdir.join("rebase-merge")).unwrap();
        assert_eq!(detect_operation(wt.path()), ActiveOperation::Rebase);
    }

    #[test]
    fn rebase_apply_directory_returns_rebase() {
        let (wt, gitdir) = setup_linked_worktree();
        let actual_gitdir = gitdir.path().join("worktrees").join("feature");
        fs::create_dir_all(actual_gitdir.join("rebase-apply")).unwrap();
        assert_eq!(detect_operation(wt.path()), ActiveOperation::Rebase);
    }

    #[test]
    fn merge_head_file_returns_merge() {
        let (wt, gitdir) = setup_linked_worktree();
        let actual_gitdir = gitdir.path().join("worktrees").join("feature");
        fs::write(actual_gitdir.join("MERGE_HEAD"), "abc1234\n").unwrap();
        assert_eq!(detect_operation(wt.path()), ActiveOperation::Merge);
    }

    #[test]
    fn rebase_takes_priority_over_merge() {
        let (wt, gitdir) = setup_linked_worktree();
        let actual_gitdir = gitdir.path().join("worktrees").join("feature");
        fs::create_dir_all(actual_gitdir.join("rebase-merge")).unwrap();
        fs::write(actual_gitdir.join("MERGE_HEAD"), "abc1234\n").unwrap();
        assert_eq!(detect_operation(wt.path()), ActiveOperation::Rebase);
    }

    #[test]
    fn nonexistent_path_returns_none() {
        let path = Path::new("/tmp/this-path-does-not-exist-lazywt-test");
        assert_eq!(detect_operation(path), ActiveOperation::None);
    }

    #[test]
    fn real_git_dir_clean_returns_none() {
        let dir = setup_real_git_dir();
        assert_eq!(detect_operation(dir.path()), ActiveOperation::None);
    }

    #[test]
    fn real_git_dir_with_merge_head_returns_merge() {
        let dir = setup_real_git_dir();
        fs::write(dir.path().join(".git").join("MERGE_HEAD"), "abc1234\n").unwrap();
        assert_eq!(detect_operation(dir.path()), ActiveOperation::Merge);
    }
}
