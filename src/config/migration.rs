use std::fs;
use std::path::Path;

use super::schema::{self, GlobalConfig};
use super::Config;

/// Migrate legacy INI config (~/.wtconfig) to JSON format.
///
/// Auto-detects the old INI file and converts to JSON on first run (D-40).
/// Does nothing if:
/// - No INI file exists (clean install)
/// - JSON config already exists (already migrated)
///
/// Migration is one-shot and silent except for a single notice line (D-42).
pub fn migrate_if_needed() {
    let ini_path = match dirs::home_dir() {
        Some(h) => h.join(".wtconfig"),
        None => return,
    };
    let json_path = match super::global_config_path() {
        Some(p) => p,
        None => return,
    };

    migrate_from(&ini_path, &json_path);

    // Also migrate the update check cache (D-41)
    if let (Some(home), Some(config_dir)) = (dirs::home_dir(), dirs::config_dir()) {
        let old_cache = home.join(".wt_update_check");
        let new_cache = config_dir.join("lazywt").join("update_check");
        migrate_cache_from(&old_cache, &new_cache);
    }
}

/// Core migration logic with explicit paths (testable without touching real home dir).
///
/// Pitfall 5: Write JSON FIRST, then delete INI. If write fails, INI is preserved.
fn migrate_from(ini_path: &Path, json_path: &Path) {
    // 1. Check if INI file exists. If not, nothing to migrate.
    if !ini_path.exists() {
        return;
    }

    // 2. Check if JSON config already exists. If yes, already migrated.
    if json_path.exists() {
        return;
    }

    // 3. Read INI content
    let content = match fs::read_to_string(ini_path) {
        Ok(c) => c,
        Err(_) => return, // Silently ignore read errors
    };

    // 4. Parse INI content using the existing hand-rolled parser
    let config = Config::parse(&content);

    // 5. Convert to GlobalConfig (new v2.0 fields get defaults)
    let global = GlobalConfig {
        command_name: config.command_name,
        worktree_folder: config.worktree_folder,
        bare_dir: None,
        auto_update: config.auto_update,
        auto_navigate: config.auto_navigate,
        branch_prefix: String::new(),
        copy_files: Vec::new(),
        worktree_location: "inside".to_string(),
    };

    // 6. Write JSON FIRST (Pitfall 5: write before delete)
    if schema::write_global_to(json_path, &global).is_err() {
        // If JSON write fails, do NOT delete INI. Preserve the user's config.
        return;
    }

    // 7. Delete INI file (best-effort)
    let _ = fs::remove_file(ini_path);

    // 8. Print one-line notice (D-42)
    crate::output::info(&format!(
        "Migrated config to {}",
        dunce::simplified(json_path).display()
    ));
}

/// Migrate the update check cache file from home dir to config dir (D-41).
fn migrate_cache_from(old_cache: &Path, new_cache: &Path) {
    // Only migrate if old exists and new does not
    if !old_cache.exists() || new_cache.exists() {
        return;
    }

    // Create parent directory
    if let Some(parent) = new_cache.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    // Copy then delete (safer than rename across filesystems)
    if fs::copy(old_cache, new_cache).is_ok() {
        let _ = fs::remove_file(old_cache);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_does_nothing_when_no_ini_exists() {
        let dir = tempfile::tempdir().unwrap();
        let ini_path = dir.path().join(".wtconfig");
        let json_path = dir.path().join("config.json");

        migrate_from(&ini_path, &json_path);

        assert!(!json_path.exists());
    }

    #[test]
    fn migrate_does_nothing_when_json_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let ini_path = dir.path().join(".wtconfig");
        let json_path = dir.path().join("config.json");

        // Create both files
        fs::write(&ini_path, "command_name = old").unwrap();
        fs::write(&json_path, r#"{"command_name": "new"}"#).unwrap();

        migrate_from(&ini_path, &json_path);

        // JSON should remain unchanged
        let content = fs::read_to_string(&json_path).unwrap();
        assert!(content.contains("new"));
        // INI should still exist (not deleted because JSON existed)
        assert!(ini_path.exists());
    }

    #[test]
    fn migrate_reads_ini_writes_json_deletes_ini() {
        let dir = tempfile::tempdir().unwrap();
        let ini_path = dir.path().join(".wtconfig");
        let json_path = dir.path().join("nested").join("config.json");

        fs::write(
            &ini_path,
            "command_name = mycmd\nauto_update = false\nworktree_folder = trees\nauto_navigate = true",
        )
        .unwrap();

        migrate_from(&ini_path, &json_path);

        // JSON should exist with migrated values
        assert!(json_path.exists());
        // INI should be deleted
        assert!(!ini_path.exists());

        // Verify JSON content
        let content = fs::read_to_string(&json_path).unwrap();
        let global: GlobalConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(global.command_name, "mycmd");
        assert!(!global.auto_update);
        assert_eq!(global.worktree_folder, Some("trees".to_string()));
        assert!(global.auto_navigate);
        // v2.0 fields should have defaults
        assert_eq!(global.branch_prefix, "");
        assert!(global.copy_files.is_empty());
        assert_eq!(global.worktree_location, "inside");
    }

    #[test]
    fn migrate_produces_valid_json_parseable_as_global_config() {
        let dir = tempfile::tempdir().unwrap();
        let ini_path = dir.path().join(".wtconfig");
        let json_path = dir.path().join("config.json");

        fs::write(&ini_path, "command_name = test").unwrap();

        migrate_from(&ini_path, &json_path);

        let content = fs::read_to_string(&json_path).unwrap();
        let result: Result<GlobalConfig, _> = serde_json::from_str(&content);
        assert!(result.is_ok());
    }

    #[test]
    fn migrate_ini_with_all_four_keys() {
        let dir = tempfile::tempdir().unwrap();
        let ini_path = dir.path().join(".wtconfig");
        let json_path = dir.path().join("config.json");

        fs::write(
            &ini_path,
            "command_name = gw\nworktree_folder = wt\nauto_update = true\nauto_navigate = false",
        )
        .unwrap();

        migrate_from(&ini_path, &json_path);

        let content = fs::read_to_string(&json_path).unwrap();
        let global: GlobalConfig = serde_json::from_str(&content).unwrap();
        assert_eq!(global.command_name, "gw");
        assert_eq!(global.worktree_folder, Some("wt".to_string()));
        assert!(global.auto_update);
        assert!(!global.auto_navigate);
    }

    #[test]
    fn migrate_ini_with_unknown_keys_ignores_them() {
        let dir = tempfile::tempdir().unwrap();
        let ini_path = dir.path().join(".wtconfig");
        let json_path = dir.path().join("config.json");

        fs::write(
            &ini_path,
            "command_name = wt\nunknown_future_key = value\nanother_key = 42",
        )
        .unwrap();

        migrate_from(&ini_path, &json_path);

        let content = fs::read_to_string(&json_path).unwrap();
        // Should not contain unknown keys
        assert!(!content.contains("unknown_future_key"));
        assert!(!content.contains("another_key"));
    }

    #[test]
    fn migrate_preserves_ini_on_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let ini_path = dir.path().join(".wtconfig");
        // Point to an impossible path (no parent dir creation possible)
        let json_path = if cfg!(windows) {
            std::path::PathBuf::from("Z:\\nonexistent\\deeply\\nested\\config.json")
        } else {
            std::path::PathBuf::from("/proc/nonexistent/config.json")
        };

        fs::write(&ini_path, "command_name = preserved").unwrap();

        migrate_from(&ini_path, &json_path);

        // INI should NOT be deleted since JSON write failed (Pitfall 5)
        assert!(ini_path.exists());
        let content = fs::read_to_string(&ini_path).unwrap();
        assert!(content.contains("preserved"));
    }

    #[test]
    fn migrate_cache_does_nothing_if_old_cache_missing() {
        let dir = tempfile::tempdir().unwrap();
        let old_cache = dir.path().join(".wt_update_check");
        let new_cache = dir.path().join("lazywt").join("update_check");

        migrate_cache_from(&old_cache, &new_cache);

        assert!(!new_cache.exists());
    }

    #[test]
    fn migrate_cache_moves_old_to_new() {
        let dir = tempfile::tempdir().unwrap();
        let old_cache = dir.path().join(".wt_update_check");
        let new_cache = dir.path().join("lazywt").join("update_check");

        fs::write(&old_cache, "1234567890\n1.0.0\n").unwrap();

        migrate_cache_from(&old_cache, &new_cache);

        // New cache should exist with same content
        assert!(new_cache.exists());
        let content = fs::read_to_string(&new_cache).unwrap();
        assert!(content.contains("1234567890"));
        assert!(content.contains("1.0.0"));
        // Old cache should be deleted
        assert!(!old_cache.exists());
    }

    #[test]
    fn migrate_cache_does_nothing_if_new_cache_exists() {
        let dir = tempfile::tempdir().unwrap();
        let old_cache = dir.path().join(".wt_update_check");
        let new_dir = dir.path().join("lazywt");
        fs::create_dir_all(&new_dir).unwrap();
        let new_cache = new_dir.join("update_check");

        fs::write(&old_cache, "old content").unwrap();
        fs::write(&new_cache, "new content").unwrap();

        migrate_cache_from(&old_cache, &new_cache);

        // Old cache should still exist (not deleted)
        assert!(old_cache.exists());
        // New cache should be unchanged
        let content = fs::read_to_string(&new_cache).unwrap();
        assert_eq!(content, "new content");
    }
}
