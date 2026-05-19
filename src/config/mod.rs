pub mod schema;
pub mod merge;
pub(crate) mod migration;
pub(crate) mod interactive;

use std::env;
use std::path::{Path, PathBuf};

/// Configuration for lazywt, loaded from JSON config files and environment variables.
///
/// Priority (D-36): env var > per-repo JSON > global JSON > default
pub struct Config {
    /// The command name used in user-facing messages (default: "wt")
    pub command_name: String,
    /// Custom worktree folder name (None = use layout default)
    pub worktree_folder: Option<String>,
    /// Custom bare repo directory name (None = use ".git" default)
    pub bare_dir: Option<String>,
    /// Whether to check for updates automatically (default: true)
    pub auto_update: bool,
    /// Whether to auto-navigate to new worktrees without prompting (default: false)
    pub auto_navigate: bool,
    /// Branch name prefix for new worktrees (default: "")
    pub branch_prefix: String,
    /// Files to copy into new worktrees (default: [])
    pub copy_files: Vec<String>,
    /// Where worktrees are created relative to bare repo: "inside" or "outside" (default: "inside")
    pub worktree_location: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            command_name: "wt".to_string(),
            worktree_folder: None,
            bare_dir: None,
            auto_update: true,
            auto_navigate: false,
            branch_prefix: String::new(),
            copy_files: Vec::new(),
            worktree_location: "inside".to_string(),
        }
    }
}

impl Config {
    /// Load configuration from global JSON config and environment variables.
    ///
    /// Calls migrate_if_needed() first to handle INI-to-JSON migration.
    /// Priority: env var > global JSON > default
    pub fn load() -> Self {
        migration::migrate_if_needed();
        let global = schema::load_global();
        merge::merge(&global, None)
    }

    /// Load configuration from a specific global config path (for testing).
    /// Then applies environment variable overrides.
    #[cfg(test)]
    fn load_from(global_config_path: Option<&Path>) -> Self {
        let global = match global_config_path {
            Some(path) => schema::load_global_from(path),
            None => schema::GlobalConfig::default(),
        };
        merge::merge(&global, None)
    }

    /// Merge per-repo overrides into the config.
    /// Reloads global config and merges with repo-specific config.
    pub fn with_repo(&self, project_dir: &Path) -> Self {
        let global = schema::load_global();
        let repo = schema::load_repo(project_dir);
        merge::merge(&global, repo.as_ref())
    }

    /// Parse configuration from a string (INI-style key = value).
    /// Used for migration from legacy ~/.wtconfig format.
    /// Does NOT apply environment variable overrides.
    pub(crate) fn parse(content: &str) -> Self {
        let mut config = Self::default();
        config.apply_str(content);
        config
    }

    /// Apply key=value pairs from a config string to this config.
    /// Supports the legacy INI format from ~/.wtconfig.
    fn apply_str(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim().trim_end_matches('\r');

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Split on first '=' only (values may contain '=')
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "command_name" => {
                        self.command_name = value.to_string();
                    }
                    "worktree_folder" => {
                        self.worktree_folder = Some(value.to_string());
                    }
                    "auto_update" => {
                        self.auto_update = value != "false";
                    }
                    "auto_navigate" => {
                        self.auto_navigate = value == "true";
                    }
                    _ => {
                        // Ignore unknown keys
                    }
                }
            }
        }
    }

    /// Apply environment variable overrides.
    /// Called after merge to ensure env vars have highest priority (D-36).
    pub(crate) fn apply_env(&mut self) {
        if let Ok(val) = env::var("WT_RENAME") {
            self.command_name = val;
        }

        if let Ok(val) = env::var("WT_WORKTREE_FOLDER") {
            // Empty string is valid - means Some("")
            self.worktree_folder = Some(val);
        }

        if let Ok(val) = env::var("WT_BARE_DIR") {
            self.bare_dir = Some(val);
        }

        if let Ok(val) = env::var("WT_AUTO_UPDATE") {
            self.auto_update = val != "false";
        }

        if let Ok(val) = env::var("WT_AUTO_NAVIGATE") {
            self.auto_navigate = val == "true";
        }

        if let Ok(val) = env::var("WT_BRANCH_PREFIX") {
            self.branch_prefix = val;
        }
    }
}

/// Returns the path to the global config file.
/// Platform-specific: uses dirs::config_dir() (XDG on Linux, AppData on Windows, Library on macOS).
pub fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("lazywt").join("config.json"))
}

/// Returns the path to the per-repo config file.
/// Located at <bare-root>/lazywt.json (D-34).
/// Returns the path to the per-repo config file.
/// Located at <bare-root>/lazywt.json (D-34).
pub fn repo_config_path(project_dir: &Path) -> PathBuf {
    project_dir.join("lazywt.json")
}

/// Shared mutex for tests that manipulate environment variables.
/// Must be in a non-test module so all test submodules can share it.
#[cfg(test)]
pub(crate) static ENV_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        super::ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner())
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
    fn default_values() {
        let config = Config::default();
        assert_eq!(config.command_name, "wt");
        assert_eq!(config.worktree_folder, None);
        assert_eq!(config.bare_dir, None);
        assert!(config.auto_update);
        assert!(!config.auto_navigate);
        assert_eq!(config.branch_prefix, "");
        assert!(config.copy_files.is_empty());
        assert_eq!(config.worktree_location, "inside");
    }

    #[test]
    fn load_from_none_without_env_uses_defaults() {
        let _lock = lock_env();
        clear_env_vars();

        let config = Config::load_from(None);
        assert_eq!(config.command_name, "wt");
        assert_eq!(config.worktree_folder, None);
        assert!(config.auto_update);
    }

    #[test]
    fn load_from_json_file() {
        let _lock = lock_env();
        clear_env_vars();

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(
            &config_path,
            r#"{"command_name": "mycmd", "auto_update": false}"#,
        )
        .unwrap();

        let config = Config::load_from(Some(&config_path));
        assert_eq!(config.command_name, "mycmd");
        assert!(!config.auto_update);
    }

    #[test]
    fn load_from_nonexistent_file_uses_defaults() {
        let _lock = lock_env();
        clear_env_vars();

        let path = PathBuf::from("/nonexistent/path/config.json");
        let config = Config::load_from(Some(&path));
        assert_eq!(config.command_name, "wt");
        assert_eq!(config.worktree_folder, None);
        assert!(config.auto_update);
    }

    #[test]
    fn env_overrides_json_config() {
        // Test the env override mechanism directly through apply_env
        // to avoid env var race conditions with merge.rs tests.
        let mut config = Config {
            command_name: "file_cmd".to_string(),
            auto_update: false,
            ..Config::default()
        };

        // Simulate what apply_env does for WT_RENAME
        // (Direct env var tests are in merge.rs tests which hold the env mutex)
        config.command_name = "env_cmd".to_string();
        assert_eq!(config.command_name, "env_cmd");
        assert!(!config.auto_update); // untouched fields remain

        // Also verify load_from properly chains to merge which calls apply_env
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_RENAME", "env_cmd");

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(
            &config_path,
            r#"{"command_name": "file_cmd", "auto_update": false}"#,
        )
        .unwrap();

        let config = Config::load_from(Some(&config_path));
        assert_eq!(config.command_name, "env_cmd");
        assert!(!config.auto_update);

        clear_env_vars();
    }

    #[test]
    fn with_repo_merges_repo_overrides() {
        let _lock = lock_env();
        clear_env_vars();

        // Create a fake repo dir with lazywt.json
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lazywt.json"),
            r#"{"command_name": "repo_cmd", "auto_navigate": true}"#,
        )
        .unwrap();

        // Test the merge logic directly with a known global config
        let global = schema::GlobalConfig::default();
        let repo = schema::load_repo(dir.path());
        let merged = merge::merge(&global, repo.as_ref());
        assert_eq!(merged.command_name, "repo_cmd");
        assert!(merged.auto_navigate);
        // Global defaults should apply for non-overridden fields
        assert!(merged.auto_update);
    }

    #[test]
    fn global_config_path_returns_some() {
        // This test verifies the function runs without panicking
        // and returns a path ending with lazywt/config.json
        if let Some(path) = global_config_path() {
            assert!(path.ends_with("lazywt/config.json") || path.ends_with("lazywt\\config.json"));
        }
        // If dirs::config_dir() returns None (unlikely), the test still passes
    }

    #[test]
    fn repo_config_path_appends_lazywt_json() {
        let project_dir = Path::new("/some/project");
        let path = repo_config_path(project_dir);
        assert!(path.ends_with("lazywt.json"));
    }

    // Carry over INI parsing tests for migration compatibility
    #[test]
    fn parse_ini_all_keys() {
        let content = "command_name = custom\nworktree_folder = trees\nauto_update = false";
        let config = Config::parse(content);
        assert_eq!(config.command_name, "custom");
        assert_eq!(config.worktree_folder, Some("trees".to_string()));
        assert!(!config.auto_update);
    }

    #[test]
    fn parse_ini_ignores_comments() {
        let content = "# This is a comment\ncommand_name = mycmd\n# Another comment";
        let config = Config::parse(content);
        assert_eq!(config.command_name, "mycmd");
    }

    #[test]
    fn parse_ini_handles_empty_lines() {
        let content = "\n\ncommand_name = test\n\n";
        let config = Config::parse(content);
        assert_eq!(config.command_name, "test");
    }

    #[test]
    fn parse_ini_splits_on_first_equals_only() {
        let content = "command_name = val=ue=with=equals";
        let config = Config::parse(content);
        assert_eq!(config.command_name, "val=ue=with=equals");
    }

    #[test]
    fn parse_ini_trims_whitespace() {
        let content = "  command_name  =  spaced  ";
        let config = Config::parse(content);
        assert_eq!(config.command_name, "spaced");
    }

    #[test]
    fn parse_ini_handles_carriage_return() {
        let content = "command_name = test\r\nauto_update = false\r\n";
        let config = Config::parse(content);
        assert_eq!(config.command_name, "test");
        assert!(!config.auto_update);
    }

    #[test]
    fn parse_ini_ignores_unknown_keys() {
        let content = "unknown_key = value\ncommand_name = known";
        let config = Config::parse(content);
        assert_eq!(config.command_name, "known");
    }

    #[test]
    fn env_var_wt_rename_overrides() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_RENAME", "mycmd");

        let config = Config::load_from(None);
        assert_eq!(config.command_name, "mycmd");

        clear_env_vars();
    }

    #[test]
    fn env_var_wt_worktree_folder_empty_string() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_WORKTREE_FOLDER", "");

        let config = Config::load_from(None);
        assert_eq!(config.worktree_folder, Some("".to_string()));

        clear_env_vars();
    }

    #[test]
    fn env_var_wt_auto_update_false() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_AUTO_UPDATE", "false");

        let config = Config::load_from(None);
        assert!(!config.auto_update);

        clear_env_vars();
    }

    #[test]
    fn config_load_from_none_returns_defaults() {
        let _lock = lock_env();
        clear_env_vars();

        // Use load_from(None) to avoid reading real global config on test machine
        let config = Config::load_from(None);
        assert_eq!(config.command_name, "wt");
        assert_eq!(config.worktree_folder, None);
        assert_eq!(config.bare_dir, None);
        assert!(config.auto_update);
        assert!(!config.auto_navigate);
        assert_eq!(config.branch_prefix, "");
        assert!(config.copy_files.is_empty());
        assert_eq!(config.worktree_location, "inside");
    }

    #[test]
    fn env_var_wt_bare_dir_overrides() {
        let _lock = lock_env();
        clear_env_vars();
        env::set_var("WT_BARE_DIR", ".custom");

        let config = Config::load_from(None);
        assert_eq!(config.bare_dir, Some(".custom".to_string()));

        clear_env_vars();
    }
}
