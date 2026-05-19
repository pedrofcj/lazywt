use serde::{Deserialize, Serialize};
use std::path::Path;

/// Global config -- all fields have defaults.
/// Deserialized from ~/.config/lazywt/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    pub command_name: String,
    pub worktree_folder: Option<String>,
    /// Custom bare repo directory name (None = use ".git" default)
    pub bare_dir: Option<String>,
    pub auto_update: bool,
    pub auto_navigate: bool,
    // v2.0 fields (D-48)
    pub branch_prefix: String,
    pub copy_files: Vec<String>,
    pub worktree_location: String,
}

impl Default for GlobalConfig {
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

/// Per-repo config -- all fields are Option<T>.
/// Deserialized from <bare-root>/lazywt.json
/// None = inherit from global. Some(value) = override.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    pub command_name: Option<String>,
    pub worktree_folder: Option<String>,
    /// Custom bare repo directory name (None = inherit from global)
    pub bare_dir: Option<String>,
    pub auto_update: Option<bool>,
    pub auto_navigate: Option<bool>,
    pub branch_prefix: Option<String>,
    pub copy_files: Option<Vec<String>>,
    pub worktree_location: Option<String>,
}

/// Load global config from the platform config directory.
/// Missing file = default. Malformed JSON = warn + default (Pitfall 2).
pub fn load_global() -> GlobalConfig {
    let path = super::global_config_path();
    match path.and_then(|p| std::fs::read_to_string(&p).ok()) {
        Some(content) => serde_json::from_str(&content).unwrap_or_default(),
        None => GlobalConfig::default(),
    }
}

/// Load global config from a specific path (for testing).
pub fn load_global_from(path: &Path) -> GlobalConfig {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => GlobalConfig::default(),
    }
}

/// Load per-repo config from a project directory.
/// Missing/malformed = None.
pub fn load_repo(project_dir: &Path) -> Option<RepoConfig> {
    let path = super::repo_config_path(project_dir);
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write a global config to the platform config directory, creating parent dirs.
pub fn write_global(config: &GlobalConfig) -> anyhow::Result<()> {
    let path = super::global_config_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    write_config(&path, config)
}

/// Write a global config to a specific path (for testing and migration).
pub fn write_global_to(path: &Path, config: &GlobalConfig) -> anyhow::Result<()> {
    write_config(path, config)
}

/// Write a per-repo config to a project directory.
pub fn write_repo(project_dir: &Path, config: &RepoConfig) -> anyhow::Result<()> {
    let path = super::repo_config_path(project_dir);
    write_config(&path, config)
}

/// Write any serializable config to a path, creating parent directories.
fn write_config<T: Serialize>(path: &Path, config: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_config_default_values() {
        let config = GlobalConfig::default();
        assert_eq!(config.command_name, "wt");
        assert_eq!(config.worktree_folder, None);
        assert!(config.auto_update);
        assert!(!config.auto_navigate);
        assert_eq!(config.branch_prefix, "");
        assert!(config.copy_files.is_empty());
        assert_eq!(config.worktree_location, "inside");
    }

    #[test]
    fn global_config_deserializes_with_missing_keys() {
        let json = r#"{"command_name": "mycmd"}"#;
        let config: GlobalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command_name, "mycmd");
        // Missing keys should get defaults
        assert_eq!(config.worktree_folder, None);
        assert!(config.auto_update);
        assert!(!config.auto_navigate);
        assert_eq!(config.branch_prefix, "");
        assert!(config.copy_files.is_empty());
        assert_eq!(config.worktree_location, "inside");
    }

    #[test]
    fn global_config_ignores_unknown_keys() {
        let json = r#"{"command_name": "wt", "unknown_future_key": 42, "another_unknown": "value"}"#;
        let config: GlobalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command_name, "wt");
    }

    #[test]
    fn global_config_serde_round_trip() {
        let config = GlobalConfig {
            command_name: "mycmd".to_string(),
            worktree_folder: Some("trees".to_string()),
            bare_dir: None,
            auto_update: false,
            auto_navigate: true,
            branch_prefix: "feat/".to_string(),
            copy_files: vec![".envrc".to_string(), ".tool-versions".to_string()],
            worktree_location: "outside".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: GlobalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.command_name, "mycmd");
        assert_eq!(deserialized.worktree_folder, Some("trees".to_string()));
        assert!(!deserialized.auto_update);
        assert!(deserialized.auto_navigate);
        assert_eq!(deserialized.branch_prefix, "feat/");
        assert_eq!(deserialized.copy_files, vec![".envrc", ".tool-versions"]);
        assert_eq!(deserialized.worktree_location, "outside");
    }

    #[test]
    fn repo_config_all_none_from_empty_json() {
        let json = r#"{}"#;
        let config: RepoConfig = serde_json::from_str(json).unwrap();
        assert!(config.command_name.is_none());
        assert!(config.worktree_folder.is_none());
        assert!(config.auto_update.is_none());
        assert!(config.auto_navigate.is_none());
        assert!(config.branch_prefix.is_none());
        assert!(config.copy_files.is_none());
        assert!(config.worktree_location.is_none());
    }

    #[test]
    fn repo_config_partial_fields() {
        let json = r#"{"command_name": "custom", "auto_update": false}"#;
        let config: RepoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command_name, Some("custom".to_string()));
        assert_eq!(config.auto_update, Some(false));
        // Rest should be None
        assert!(config.worktree_folder.is_none());
        assert!(config.auto_navigate.is_none());
    }

    #[test]
    fn load_global_from_nonexistent_path_returns_default() {
        let config = load_global_from(std::path::Path::new("/nonexistent/path/config.json"));
        assert_eq!(config.command_name, "wt");
        assert!(config.auto_update);
    }

    #[test]
    fn load_global_from_malformed_json_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "this is not json {{{").unwrap();
        let config = load_global_from(&path);
        assert_eq!(config.command_name, "wt");
        assert!(config.auto_update);
    }

    #[test]
    fn load_global_from_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"command_name": "mycmd", "auto_update": false}"#).unwrap();
        let config = load_global_from(&path);
        assert_eq!(config.command_name, "mycmd");
        assert!(!config.auto_update);
    }

    #[test]
    fn load_repo_from_nonexistent_returns_none() {
        let config = load_repo(std::path::Path::new("/nonexistent/project"));
        assert!(config.is_none());
    }

    #[test]
    fn load_repo_from_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lazywt.json"),
            r#"{"command_name": "custom"}"#,
        )
        .unwrap();
        let config = load_repo(dir.path()).unwrap();
        assert_eq!(config.command_name, Some("custom".to_string()));
    }

    #[test]
    fn write_config_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("config.json");
        let config = GlobalConfig::default();
        write_global_to(&path, &config).unwrap();
        assert!(path.exists());
        // Verify it's valid JSON
        let content = std::fs::read_to_string(&path).unwrap();
        let _: GlobalConfig = serde_json::from_str(&content).unwrap();
    }

    #[test]
    fn write_config_produces_pretty_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = GlobalConfig::default();
        write_global_to(&path, &config).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // Pretty JSON has newlines
        assert!(content.contains('\n'));
        assert!(content.contains("  "));
    }

    // --- bare_dir tests ---

    #[test]
    fn global_config_bare_dir_default_is_none() {
        let config = GlobalConfig::default();
        assert_eq!(config.bare_dir, None);
    }

    #[test]
    fn global_config_bare_dir_from_json() {
        let json = r#"{"bare_dir": ".repo"}"#;
        let config: GlobalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.bare_dir, Some(".repo".to_string()));
    }

    #[test]
    fn global_config_bare_dir_missing_key() {
        let json = r#"{"command_name": "wt"}"#;
        let config: GlobalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.bare_dir, None);
    }

    #[test]
    fn global_config_bare_dir_round_trip() {
        let config = GlobalConfig {
            bare_dir: Some(".repo".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: GlobalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.bare_dir, Some(".repo".to_string()));
    }

    #[test]
    fn repo_config_bare_dir_from_json() {
        let json = r#"{"bare_dir": ".repo"}"#;
        let config: RepoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.bare_dir, Some(".repo".to_string()));
    }

    #[test]
    fn repo_config_bare_dir_empty_json() {
        let json = r#"{}"#;
        let config: RepoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.bare_dir, None);
    }
}
