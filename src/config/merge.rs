use super::schema::{GlobalConfig, RepoConfig};
use super::Config;

/// Merge global and optional per-repo config into a final Config.
/// Resolution order (D-36): env vars > per-repo > global > defaults.
/// Array values (copy_files): per-repo REPLACES global entirely (D-38).
pub fn merge(global: &GlobalConfig, repo: Option<&RepoConfig>) -> Config {
    let mut config = Config {
        command_name: global.command_name.clone(),
        worktree_folder: global.worktree_folder.clone(),
        bare_dir: global.bare_dir.clone(),
        auto_update: global.auto_update,
        auto_navigate: global.auto_navigate,
        branch_prefix: global.branch_prefix.clone(),
        copy_files: global.copy_files.clone(),
        worktree_location: global.worktree_location.clone(),
    };

    if let Some(repo) = repo {
        if let Some(ref v) = repo.command_name {
            config.command_name = v.clone();
        }
        if let Some(ref v) = repo.worktree_folder {
            config.worktree_folder = Some(v.clone());
        }
        if let Some(ref v) = repo.bare_dir {
            config.bare_dir = Some(v.clone());
        }
        if let Some(v) = repo.auto_update {
            config.auto_update = v;
        }
        if let Some(v) = repo.auto_navigate {
            config.auto_navigate = v;
        }
        if let Some(ref v) = repo.branch_prefix {
            config.branch_prefix = v.clone();
        }
        // D-38: copy_files replaces entirely, does not append
        if let Some(ref v) = repo.copy_files {
            config.copy_files = v.clone();
        }
        if let Some(ref v) = repo.worktree_location {
            config.worktree_location = v.clone();
        }
    }

    // Environment variables override everything (D-36)
    config.apply_env();
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        crate::config::ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn clear_env_vars() {
        env::remove_var("WT_RENAME");
        env::remove_var("WT_WORKTREE_FOLDER");
        env::remove_var("WT_BARE_DIR");
        env::remove_var("WT_AUTO_UPDATE");
        env::remove_var("WT_AUTO_NAVIGATE");
        env::remove_var("WT_BRANCH_PREFIX");
    }

    #[test]
    fn merge_with_no_repo_returns_global_values() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig {
            command_name: "mycmd".to_string(),
            worktree_folder: Some("trees".to_string()),
            bare_dir: Some(".repo".to_string()),
            auto_update: false,
            auto_navigate: true,
            branch_prefix: "feat/".to_string(),
            copy_files: vec![".envrc".to_string()],
            worktree_location: "outside".to_string(),
        };

        let config = merge(&global, None);
        assert_eq!(config.command_name, "mycmd");
        assert_eq!(config.worktree_folder, Some("trees".to_string()));
        assert_eq!(config.bare_dir, Some(".repo".to_string()));
        assert!(!config.auto_update);
        assert!(config.auto_navigate);
        assert_eq!(config.branch_prefix, "feat/");
        assert_eq!(config.copy_files, vec![".envrc"]);
        assert_eq!(config.worktree_location, "outside");
    }

    #[test]
    fn merge_repo_overrides_command_name() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig::default();
        let repo = RepoConfig {
            command_name: Some("custom".to_string()),
            ..Default::default()
        };

        let config = merge(&global, Some(&repo));
        assert_eq!(config.command_name, "custom");
    }

    #[test]
    fn merge_repo_none_fields_inherit_global() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig {
            command_name: "mycmd".to_string(),
            auto_update: false,
            ..Default::default()
        };
        let repo = RepoConfig::default(); // all None

        let config = merge(&global, Some(&repo));
        assert_eq!(config.command_name, "mycmd");
        assert!(!config.auto_update);
    }

    #[test]
    fn merge_repo_copy_files_replaces_global() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig {
            copy_files: vec![".envrc".to_string(), ".tool-versions".to_string()],
            ..Default::default()
        };
        let repo = RepoConfig {
            copy_files: Some(vec![".env".to_string()]),
            ..Default::default()
        };

        let config = merge(&global, Some(&repo));
        // D-38: replaces entirely, not append
        assert_eq!(config.copy_files, vec![".env"]);
    }

    #[test]
    fn merge_repo_copy_files_none_inherits_global() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig {
            copy_files: vec![".envrc".to_string()],
            ..Default::default()
        };
        let repo = RepoConfig {
            copy_files: None,
            ..Default::default()
        };

        let config = merge(&global, Some(&repo));
        assert_eq!(config.copy_files, vec![".envrc"]);
    }

    #[test]
    fn merge_with_full_repo_override() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig::default();
        let repo = RepoConfig {
            command_name: Some("repo_cmd".to_string()),
            worktree_folder: Some("wt".to_string()),
            bare_dir: Some(".bare".to_string()),
            auto_update: Some(false),
            auto_navigate: Some(true),
            branch_prefix: Some("fix/".to_string()),
            copy_files: Some(vec![".env".to_string()]),
            worktree_location: Some("outside".to_string()),
        };

        let config = merge(&global, Some(&repo));
        assert_eq!(config.command_name, "repo_cmd");
        assert_eq!(config.worktree_folder, Some("wt".to_string()));
        assert_eq!(config.bare_dir, Some(".bare".to_string()));
        assert!(!config.auto_update);
        assert!(config.auto_navigate);
        assert_eq!(config.branch_prefix, "fix/");
        assert_eq!(config.copy_files, vec![".env"]);
        assert_eq!(config.worktree_location, "outside");
    }

    #[test]
    fn merge_env_wt_rename_overrides() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_RENAME", "env_cmd");

        let global = GlobalConfig {
            command_name: "global_cmd".to_string(),
            ..Default::default()
        };
        let repo = RepoConfig {
            command_name: Some("repo_cmd".to_string()),
            ..Default::default()
        };

        let config = merge(&global, Some(&repo));
        assert_eq!(config.command_name, "env_cmd");

        clear_env_vars();
    }

    #[test]
    fn merge_env_wt_worktree_folder_empty_string() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_WORKTREE_FOLDER", "");

        let global = GlobalConfig::default();
        let config = merge(&global, None);
        assert_eq!(config.worktree_folder, Some("".to_string()));

        clear_env_vars();
    }

    #[test]
    fn merge_env_wt_auto_update_false() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_AUTO_UPDATE", "false");

        let global = GlobalConfig::default();
        let config = merge(&global, None);
        assert!(!config.auto_update);

        clear_env_vars();
    }

    #[test]
    fn merge_env_wt_auto_navigate_true() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_AUTO_NAVIGATE", "true");

        let global = GlobalConfig::default();
        let config = merge(&global, None);
        assert!(config.auto_navigate);

        clear_env_vars();
    }

    #[test]
    fn merge_env_wt_branch_prefix_overrides() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_BRANCH_PREFIX", "env-prefix/");

        let global = GlobalConfig {
            branch_prefix: "global-prefix/".to_string(),
            ..Default::default()
        };
        let config = merge(&global, None);
        assert_eq!(config.branch_prefix, "env-prefix/");

        clear_env_vars();
    }

    // --- bare_dir merge tests ---

    #[test]
    fn merge_bare_dir_from_global() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig {
            bare_dir: Some(".repo".to_string()),
            ..Default::default()
        };
        let config = merge(&global, None);
        assert_eq!(config.bare_dir, Some(".repo".to_string()));
    }

    #[test]
    fn merge_repo_bare_dir_overrides_global() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig {
            bare_dir: Some(".git".to_string()),
            ..Default::default()
        };
        let repo = RepoConfig {
            bare_dir: Some(".repo".to_string()),
            ..Default::default()
        };
        let config = merge(&global, Some(&repo));
        assert_eq!(config.bare_dir, Some(".repo".to_string()));
    }

    #[test]
    fn merge_repo_bare_dir_none_inherits_global() {
        let _lock = lock_env();
        clear_env_vars();

        let global = GlobalConfig {
            bare_dir: Some(".repo".to_string()),
            ..Default::default()
        };
        let repo = RepoConfig {
            bare_dir: None,
            ..Default::default()
        };
        let config = merge(&global, Some(&repo));
        assert_eq!(config.bare_dir, Some(".repo".to_string()));
    }
}
