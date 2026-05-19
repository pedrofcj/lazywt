use thiserror::Error;

#[derive(Error, Debug)]
pub enum LazywtError {
    #[error("not inside a git repository")]
    NotInRepo,

    #[error("this command requires a bare repository")]
    NotBareRepo,

    #[error("worktree '{0}' not found")]
    WorktreeNotFound(String),

    #[error("worktree '{0}' already exists")]
    WorktreeExists(String),

    #[error("cannot remove default branch worktree '{0}'")]
    CannotRemoveDefault(String),

    #[error("branch '{0}' already exists")]
    BranchExists(String),

    #[error("git command failed: {command}")]
    GitFailed {
        command: String,
        stderr: String,
        exit_code: Option<i32>,
    },

    #[error("invalid worktree name: {0}")]
    InvalidName(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("directory '{0}' already exists")]
    DirectoryExists(String),

    #[error("cannot prompt for confirmation -- use --yes to skip")]
    NonInteractiveStdin,

    #[error("failed to detect default branch")]
    NoDefaultBranch,

    #[error("source worktree '{0}' not found")]
    SourceWorktreeNotFound(String),

    #[error("already using modern layout")]
    AlreadyModernLayout,

    #[error("migration target '{0}' already exists")]
    MigrateTargetExists(String),

    #[error("migration failed: {0}")]
    MigrateFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_in_repo_display() {
        let err = LazywtError::NotInRepo;
        assert_eq!(err.to_string(), "not inside a git repository");
    }

    #[test]
    fn not_bare_repo_display() {
        let err = LazywtError::NotBareRepo;
        assert_eq!(err.to_string(), "this command requires a bare repository");
    }

    #[test]
    fn worktree_not_found_display() {
        let err = LazywtError::WorktreeNotFound("foo".to_string());
        assert_eq!(err.to_string(), "worktree 'foo' not found");
    }

    #[test]
    fn worktree_exists_display() {
        let err = LazywtError::WorktreeExists("bar".to_string());
        assert_eq!(err.to_string(), "worktree 'bar' already exists");
    }

    #[test]
    fn cannot_remove_default_display() {
        let err = LazywtError::CannotRemoveDefault("main".to_string());
        assert_eq!(
            err.to_string(),
            "cannot remove default branch worktree 'main'"
        );
    }

    #[test]
    fn branch_exists_display() {
        let err = LazywtError::BranchExists("feature/x".to_string());
        assert_eq!(err.to_string(), "branch 'feature/x' already exists");
    }

    #[test]
    fn git_failed_display() {
        let err = LazywtError::GitFailed {
            command: "status".to_string(),
            stderr: "fatal error".to_string(),
            exit_code: Some(128),
        };
        assert_eq!(err.to_string(), "git command failed: status");
    }

    #[test]
    fn invalid_name_display() {
        let err = LazywtError::InvalidName("..bad".to_string());
        assert_eq!(err.to_string(), "invalid worktree name: ..bad");
    }

    #[test]
    fn config_error_display() {
        let err = LazywtError::Config("parse failure".to_string());
        assert_eq!(err.to_string(), "config error: parse failure");
    }

    #[test]
    fn directory_exists_display() {
        let err = LazywtError::DirectoryExists("foo".to_string());
        assert_eq!(err.to_string(), "directory 'foo' already exists");
    }

    #[test]
    fn non_interactive_stdin_display() {
        let err = LazywtError::NonInteractiveStdin;
        assert_eq!(
            err.to_string(),
            "cannot prompt for confirmation -- use --yes to skip"
        );
    }

    #[test]
    fn no_default_branch_display() {
        let err = LazywtError::NoDefaultBranch;
        assert_eq!(err.to_string(), "failed to detect default branch");
    }

    #[test]
    fn source_worktree_not_found_display() {
        let err = LazywtError::SourceWorktreeNotFound("dev".to_string());
        assert_eq!(err.to_string(), "source worktree 'dev' not found");
    }

    #[test]
    fn already_modern_layout_display() {
        let err = LazywtError::AlreadyModernLayout;
        assert_eq!(err.to_string(), "already using modern layout");
    }

    #[test]
    fn migrate_target_exists_display() {
        let err = LazywtError::MigrateTargetExists(".git".to_string());
        assert_eq!(err.to_string(), "migration target '.git' already exists");
    }

    #[test]
    fn migrate_failed_display() {
        let err = LazywtError::MigrateFailed("rename failed".to_string());
        assert_eq!(err.to_string(), "migration failed: rename failed");
    }
}
