use std::path::{Path, PathBuf};
use anyhow::Result;
use crate::config::Config;

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutType {
    Classic, // .bare file + trees/ subfolder (leaf name != ".git")
    Modern,  // hidden .git/ bare repo + root-level worktrees (leaf name == ".git")
    NonBare, // regular .git directory (non-bare repository)
}

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub project_dir: PathBuf,      // path to bare git directory
    pub project_root: PathBuf,     // root of the project
    pub layout_type: LayoutType,
    pub project_name: String,      // leaf of project_root
    pub worktree_folder: String,   // effective worktree subfolder
    pub worktree_parent: PathBuf,  // where worktrees are created
}

/// Detect layout and build ProjectContext.
/// project_dir: the repo path from find_repo_root().
/// For bare repos, this is the bare git directory.
/// For non-bare repos, this is the repository root.
/// config: loaded Config for worktree_folder override.
pub fn detect_layout(project_dir: &Path, config: &Config) -> Result<ProjectContext> {
    // Step 1: Check git config "wt.layout" first
    let layout = match crate::git::config::get_config(project_dir, "wt.layout")? {
        Some(val) if val == "classic" => LayoutType::Classic,
        Some(val) if val == "modern" => LayoutType::Modern,
        Some(val) if val == "nonbare" => LayoutType::NonBare,
        _ => {
            // Step 2: Heuristic fallback
            let dir_name = project_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Step 2a: Check wt.baredir git config for non-.git modern layouts (LAY-01, D-04)
            // Only match when dir name equals the configured bare_dir name.
            // If wt.baredir is set but doesn't match, fall through (Pitfall 2 from RESEARCH).
            if let Ok(Some(ref bare_dir_name)) = crate::git::config::get_config(project_dir, "wt.baredir") {
                if dir_name == bare_dir_name.as_str() {
                    return Ok(build_context(project_dir, LayoutType::Modern, config));
                }
            }

            // Step 2b: Standard .git heuristic (LAY-02, D-04 step 3)
            if dir_name == ".git" {
                LayoutType::Modern
            } else {
                // Step 2c: Bare repo check (D-04 step 4)
                let is_bare = crate::git::git(project_dir, &["rev-parse", "--is-bare-repository"])
                    .map(|s| s.trim() == "true")
                    .unwrap_or(false);
                if is_bare {
                    LayoutType::Classic
                } else {
                    LayoutType::NonBare
                }
            }
        }
    };

    Ok(build_context(project_dir, layout, config))
}

/// Build ProjectContext from known values (for testing and when git config is already known).
/// This allows unit tests to test path computation logic without requiring a real git repo.
pub fn build_context(project_dir: &Path, layout: LayoutType, config: &Config) -> ProjectContext {
    // Compute project_root
    let project_root = match layout {
        LayoutType::Modern => project_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| project_dir.to_path_buf()),
        LayoutType::Classic => project_dir.to_path_buf(),
        LayoutType::NonBare => {
            // For non-bare, project_dir IS the repo root (not .git)
            project_dir.to_path_buf()
        }
    };

    // Project name from root directory name
    let project_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Worktree folder: config override > layout default
    // D-94: NonBare repos use .wts for "inside" mode or sibling dir for "outside" mode
    let worktree_folder = match &config.worktree_folder {
        Some(folder) => folder.clone(),
        None => match layout {
            LayoutType::Classic => "trees".to_string(),
            LayoutType::Modern => String::new(),
            LayoutType::NonBare => {
                if config.worktree_location == "outside" {
                    // For outside mode, worktree_folder is empty;
                    // worktree_parent will be computed as sibling below
                    String::new()
                } else {
                    ".wts".to_string()
                }
            }
        },
    };

    // Worktree parent
    let worktree_parent = match layout {
        LayoutType::NonBare if config.worktree_location == "outside" && config.worktree_folder.is_none() => {
            // D-94: Sibling directory mode -- worktrees stored next to repo
            // e.g., /code/myproject -> /code/myproject-wts/
            if let Some(parent) = project_root.parent() {
                parent.join(format!("{}-wts", project_name))
            } else {
                project_root.join(".wts")
            }
        }
        _ => {
            if worktree_folder.is_empty() {
                project_root.clone()
            } else {
                project_root.join(&worktree_folder)
            }
        }
    };

    ProjectContext {
        project_dir: project_dir.to_path_buf(),
        project_root,
        layout_type: layout,
        project_name,
        worktree_folder,
        worktree_parent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> Config {
        Config::default()
    }

    fn config_with_folder(folder: &str) -> Config {
        Config {
            worktree_folder: Some(folder.to_string()),
            ..Config::default()
        }
    }

    fn config_with_location(location: &str) -> Config {
        Config {
            worktree_location: location.to_string(),
            ..Config::default()
        }
    }

    #[test]
    fn layout_type_classic_and_modern_are_distinct() {
        assert_ne!(LayoutType::Classic, LayoutType::Modern);
    }

    #[test]
    fn modern_layout_project_root_is_parent_of_project_dir() {
        let project_dir = PathBuf::from("/home/user/myproject/.git");
        let ctx = build_context(&project_dir, LayoutType::Modern, &default_config());
        assert_eq!(ctx.project_root, PathBuf::from("/home/user/myproject"));
    }

    #[test]
    fn modern_layout_worktree_parent_equals_project_root_when_no_folder() {
        let project_dir = PathBuf::from("/home/user/myproject/.git");
        let ctx = build_context(&project_dir, LayoutType::Modern, &default_config());
        assert_eq!(ctx.worktree_parent, ctx.project_root,
            "Modern layout without worktree_folder config should have worktree_parent == project_root");
    }

    #[test]
    fn classic_layout_project_root_equals_project_dir() {
        let project_dir = PathBuf::from("/home/user/myproject/.bare");
        let ctx = build_context(&project_dir, LayoutType::Classic, &default_config());
        assert_eq!(ctx.project_root, project_dir);
    }

    #[test]
    fn classic_layout_worktree_parent_is_project_root_trees() {
        let project_dir = PathBuf::from("/home/user/myproject/.bare");
        let ctx = build_context(&project_dir, LayoutType::Classic, &default_config());
        assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject/.bare/trees"));
    }

    #[test]
    fn config_worktree_folder_overrides_default() {
        let project_dir = PathBuf::from("/home/user/myproject/.bare");
        let ctx = build_context(&project_dir, LayoutType::Classic, &config_with_folder("custom"));
        assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject/.bare/custom"));
    }

    #[test]
    fn empty_worktree_folder_means_project_root() {
        let project_dir = PathBuf::from("/home/user/myproject/.bare");
        let ctx = build_context(&project_dir, LayoutType::Classic, &config_with_folder(""));
        assert_eq!(ctx.worktree_parent, ctx.project_root,
            "Empty worktree_folder should mean worktree_parent == project_root");
    }

    #[test]
    fn project_name_is_leaf_of_project_root() {
        let project_dir = PathBuf::from("/home/user/myproject/.git");
        let ctx = build_context(&project_dir, LayoutType::Modern, &default_config());
        assert_eq!(ctx.project_name, "myproject");
    }

    #[test]
    fn project_name_classic_layout() {
        let project_dir = PathBuf::from("/home/user/myproject/.bare");
        let ctx = build_context(&project_dir, LayoutType::Classic, &default_config());
        assert_eq!(ctx.project_name, ".bare");
    }

    #[test]
    fn worktree_folder_default_classic_is_trees() {
        let project_dir = PathBuf::from("/home/user/myproject/.bare");
        let ctx = build_context(&project_dir, LayoutType::Classic, &default_config());
        assert_eq!(ctx.worktree_folder, "trees");
    }

    #[test]
    fn worktree_folder_default_modern_is_empty() {
        let project_dir = PathBuf::from("/home/user/myproject/.git");
        let ctx = build_context(&project_dir, LayoutType::Modern, &default_config());
        assert_eq!(ctx.worktree_folder, "");
    }

    // --- NonBare layout tests ---

    #[test]
    fn nonbare_is_distinct_from_classic_and_modern() {
        assert_ne!(LayoutType::NonBare, LayoutType::Classic);
        assert_ne!(LayoutType::NonBare, LayoutType::Modern);
    }

    #[test]
    fn nonbare_inside_worktree_parent_is_wts() {
        let project_dir = PathBuf::from("/home/user/myproject");
        let ctx = build_context(&project_dir, LayoutType::NonBare, &config_with_location("inside"));
        assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject/.wts"));
    }

    #[test]
    fn nonbare_outside_worktree_parent_is_sibling() {
        let project_dir = PathBuf::from("/home/user/myproject");
        let ctx = build_context(&project_dir, LayoutType::NonBare, &config_with_location("outside"));
        assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject-wts"));
    }

    #[test]
    fn nonbare_default_location_uses_inside() {
        let project_dir = PathBuf::from("/home/user/myproject");
        // Default config has worktree_location = "inside"
        let ctx = build_context(&project_dir, LayoutType::NonBare, &default_config());
        assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject/.wts"));
    }

    #[test]
    fn nonbare_project_root_is_repo_root() {
        let project_dir = PathBuf::from("/home/user/myproject");
        let ctx = build_context(&project_dir, LayoutType::NonBare, &default_config());
        assert_eq!(ctx.project_root, PathBuf::from("/home/user/myproject"));
    }

    #[test]
    fn nonbare_project_name_is_dir_name() {
        let project_dir = PathBuf::from("/home/user/myproject");
        let ctx = build_context(&project_dir, LayoutType::NonBare, &default_config());
        assert_eq!(ctx.project_name, "myproject");
    }

    #[test]
    fn nonbare_worktree_folder_default_inside_is_wts() {
        let project_dir = PathBuf::from("/home/user/myproject");
        let ctx = build_context(&project_dir, LayoutType::NonBare, &default_config());
        assert_eq!(ctx.worktree_folder, ".wts");
    }

    #[test]
    fn nonbare_config_folder_overrides_default() {
        let project_dir = PathBuf::from("/home/user/myproject");
        let ctx = build_context(&project_dir, LayoutType::NonBare, &config_with_folder("custom"));
        assert_eq!(ctx.worktree_parent, PathBuf::from("/home/user/myproject/custom"));
    }

    // --- wt.baredir layout detection tests ---

    #[test]
    fn detect_layout_wt_bare_dir_matches_returns_modern() {
        let tmp = tempfile::tempdir().unwrap();
        let bare_dir = tmp.path().join("test-project").join(".repo");
        std::fs::create_dir_all(&bare_dir).unwrap();

        // Initialize bare repo
        let status = std::process::Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        // Set wt.baredir config
        let status = std::process::Command::new("git")
            .args(["config", "wt.baredir", ".repo"])
            .current_dir(&bare_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        let ctx = detect_layout(&bare_dir, &default_config()).unwrap();
        assert_eq!(ctx.layout_type, LayoutType::Modern);
    }

    #[test]
    fn detect_layout_wt_bare_dir_mismatch_does_not_match() {
        let tmp = tempfile::tempdir().unwrap();
        let bare_dir = tmp.path().join("test-project").join(".other");
        std::fs::create_dir_all(&bare_dir).unwrap();

        // Initialize bare repo
        let status = std::process::Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        // Set wt.baredir to a DIFFERENT name (mismatch)
        let status = std::process::Command::new("git")
            .args(["config", "wt.baredir", ".repo"])
            .current_dir(&bare_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        // Since dir_name ".other" != ".repo" AND ".other" != ".git",
        // it falls through to bare check and returns Classic
        let ctx = detect_layout(&bare_dir, &default_config()).unwrap();
        assert_eq!(ctx.layout_type, LayoutType::Classic);
    }

    #[test]
    fn detect_layout_no_wt_bare_dir_dot_git_returns_modern() {
        let tmp = tempfile::tempdir().unwrap();
        let bare_dir = tmp.path().join("test-project").join(".git");
        std::fs::create_dir_all(&bare_dir).unwrap();

        // Initialize bare repo
        let status = std::process::Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        // Do NOT set wt.baredir -- backward compat test (LAY-02)
        let ctx = detect_layout(&bare_dir, &default_config()).unwrap();
        assert_eq!(ctx.layout_type, LayoutType::Modern);
    }

    #[test]
    fn detect_layout_no_wt_bare_dir_non_git_bare_returns_classic() {
        let tmp = tempfile::tempdir().unwrap();
        let bare_dir = tmp.path().join("test-project").join(".bare");
        std::fs::create_dir_all(&bare_dir).unwrap();

        // Initialize bare repo
        let status = std::process::Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&bare_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        // Do NOT set wt.baredir -- backward compat test (LAY-02)
        let ctx = detect_layout(&bare_dir, &default_config()).unwrap();
        assert_eq!(ctx.layout_type, LayoutType::Classic);
    }
}
