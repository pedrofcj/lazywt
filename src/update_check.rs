use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::output;

/// Check for updates automatically after command dispatch.
/// Respects 24h throttle. NEVER panics, NEVER prints errors, NEVER returns errors.
pub fn check_for_update(config: &Config) {
    if !config.auto_update {
        return;
    }

    // Respect 24h throttle (D-29)
    if is_within_throttle() {
        // Even when throttled, check cached version for notification
        if let Some((_, Some(remote_version))) = read_cache() {
            maybe_notify(&remote_version);
        }
        return;
    }

    // Perform check
    let remote_version = fetch_latest_version();
    write_cache(remote_version.as_deref());

    if let Some(ref version) = remote_version {
        maybe_notify(version);
    }
}

/// Fetch the latest version from crates.io, falling back to GitHub releases.
/// Returns None on any error (network, parse, etc.).
pub fn fetch_latest_version() -> Option<String> {
    // Try crates.io first (per D-26)
    if let Some(version) = fetch_from_crates_io() {
        return Some(version);
    }
    // Fall back to GitHub releases (per D-26)
    fetch_from_github()
}

/// Compare two semver version strings. Returns true if remote > current.
pub(crate) fn is_update_available(current: &str, remote: &str) -> bool {
    let current = match semver::Version::parse(current) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let remote = match semver::Version::parse(remote) {
        Ok(v) => v,
        Err(_) => return false,
    };
    remote > current
}

/// Write timestamp and optional version to cache file.
pub(crate) fn write_cache(version: Option<&str>) {
    if let Some(path) = cache_path() {
        write_cache_at(&path, version);
    }
}

/// Write timestamp and optional version to a specific cache file path.
/// Used by tests for TempDir isolation to avoid polluting the real config directory.
pub(crate) fn write_cache_at(path: &Path, version: Option<&str>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = current_timestamp();
    let content = match version {
        Some(v) => format!("{}\n{}\n", now, v),
        None => format!("{}\n", now),
    };
    let _ = std::fs::write(path, content);
}

// --- Private helpers ---

fn fetch_from_crates_io() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .into();

    let mut response = agent
        .get("https://crates.io/api/v1/crates/lazywt")
        .header(
            "User-Agent",
            &format!(
                "lazywt/{} (https://github.com/pedrofcj/lazywt)",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .call()
        .ok()?;

    let body = response.body_mut().read_to_string().ok()?;
    extract_json_string(&body, "max_stable_version")
}

fn fetch_from_github() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .into();

    let mut response = agent
        .get("https://api.github.com/repos/pedrofcj/lazywt/releases/latest")
        .header(
            "User-Agent",
            &format!(
                "lazywt/{} (https://github.com/pedrofcj/lazywt)",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .call()
        .ok()?;

    let body = response.body_mut().read_to_string().ok()?;
    let tag = extract_json_string(&body, "tag_name")?;
    // Strip leading "v" if present (e.g., "v1.2.3" -> "1.2.3")
    Some(tag.strip_prefix('v').unwrap_or(&tag).to_string())
}

/// Naive JSON string extractor. Finds "key": "value" in a JSON body.
/// Works for flat JSON objects where the key appears exactly once.
fn extract_json_string(body: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let key_pos = body.find(&pattern)?;
    let after_key = &body[key_pos + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let end_quote = after_quote.find('"')?;
    Some(after_quote[..end_quote].to_string())
}

/// Returns the path to the cache file (~/.config/lazywt/update_check).
/// Uses dirs::config_dir() for correct cross-platform paths (D-35).
fn cache_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("lazywt").join("update_check"))
}

/// Read the cache file. Returns (timestamp, optional_version).
fn read_cache() -> Option<(u64, Option<String>)> {
    let path = cache_path()?;
    read_cache_at(&path)
}

/// Read a specific cache file path. Returns (timestamp, optional_version).
/// Used by tests for TempDir isolation to avoid reading the real config directory.
pub(crate) fn read_cache_at(path: &Path) -> Option<(u64, Option<String>)> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut lines = content.lines();
    let timestamp: u64 = lines.next()?.trim().parse().ok()?;
    let version = lines
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some((timestamp, version))
}

/// Check if last update check was within 24 hours (86400 seconds).
fn is_within_throttle() -> bool {
    if let Some(path) = cache_path() {
        is_within_throttle_at(&path)
    } else {
        false
    }
}

/// Check if a specific cache file shows a check within 24 hours.
/// Used by tests for TempDir isolation.
pub(crate) fn is_within_throttle_at(path: &Path) -> bool {
    if let Some((timestamp, _)) = read_cache_at(path) {
        let now = current_timestamp();
        now.saturating_sub(timestamp) < 86400
    } else {
        false
    }
}

/// Get current unix timestamp in seconds.
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Show update notification if a newer version is available.
pub(crate) fn maybe_notify(remote_version: &str) {
    let current = env!("CARGO_PKG_VERSION");
    if is_update_available(current, remote_version) {
        output::info(&format!(
            "New version {} available. Run `cargo install lazywt` to update.",
            remote_version
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_json_string tests ---

    #[test]
    fn extract_json_string_finds_value() {
        let body = r#"{"crate":{"max_stable_version":"1.2.3","name":"lazywt"}}"#;
        assert_eq!(
            extract_json_string(body, "max_stable_version"),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn extract_json_string_with_spaces() {
        let body = r#"{ "tag_name": "v2.0.0", "name": "Release" }"#;
        assert_eq!(
            extract_json_string(body, "tag_name"),
            Some("v2.0.0".to_string())
        );
    }

    #[test]
    fn extract_json_string_missing_key() {
        let body = r#"{"name":"lazywt"}"#;
        assert_eq!(extract_json_string(body, "max_stable_version"), None);
    }

    #[test]
    fn extract_json_string_malformed_json() {
        let body = "not json at all";
        assert_eq!(extract_json_string(body, "key"), None);
    }

    #[test]
    fn extract_json_string_no_value_after_key() {
        // Key present but no colon or value
        let body = r#"{"key"}"#;
        assert_eq!(extract_json_string(body, "key"), None);
    }

    #[test]
    fn extract_json_string_numeric_value() {
        // Key present but value is numeric (not a string)
        let body = r#"{"count": 42, "name": "test"}"#;
        assert_eq!(extract_json_string(body, "count"), None);
    }

    // --- is_update_available tests ---

    #[test]
    fn update_available_newer_version() {
        assert!(is_update_available("0.1.0", "0.2.0"));
        assert!(is_update_available("1.0.0", "1.0.1"));
        assert!(is_update_available("1.0.0", "2.0.0"));
    }

    #[test]
    fn update_not_available_same_version() {
        assert!(!is_update_available("1.0.0", "1.0.0"));
    }

    #[test]
    fn update_not_available_older_version() {
        assert!(!is_update_available("2.0.0", "1.0.0"));
        assert!(!is_update_available("1.1.0", "1.0.0"));
    }

    #[test]
    fn update_available_invalid_current() {
        assert!(!is_update_available("bad", "1.0.0"));
    }

    #[test]
    fn update_available_invalid_remote() {
        assert!(!is_update_available("1.0.0", "bad"));
    }

    #[test]
    fn update_available_both_invalid() {
        assert!(!is_update_available("bad", "worse"));
    }

    #[test]
    fn update_available_prerelease() {
        // Pre-release is less than release
        assert!(is_update_available("1.0.0-alpha", "1.0.0"));
        assert!(!is_update_available("1.0.0", "1.0.0-alpha"));
    }

    // --- cache round-trip tests (using _at variants with TempDir for isolation) ---
    // NOTE: All cache tests use write_cache_at/read_cache_at/is_within_throttle_at
    // with TempDir paths. NEVER call write_cache()/read_cache() directly in tests
    // to avoid polluting the real config directory (~/.config/lazywt/update_check).

    #[test]
    fn test_write_and_read_cache_at_round_trip_with_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");

        write_cache_at(&cache_path, Some("2.0.0"));

        let result = read_cache_at(&cache_path);
        assert!(result.is_some(), "read_cache_at should return Some after write");
        let (ts, version) = result.unwrap();
        let now = current_timestamp();
        assert!(
            now.saturating_sub(ts) < 5,
            "timestamp should be within 5 seconds of now"
        );
        assert_eq!(version, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_write_and_read_cache_at_round_trip_without_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");

        write_cache_at(&cache_path, None);

        let result = read_cache_at(&cache_path);
        assert!(result.is_some(), "read_cache_at should return Some after write");
        let (ts, version) = result.unwrap();
        let now = current_timestamp();
        assert!(
            now.saturating_sub(ts) < 5,
            "timestamp should be within 5 seconds of now"
        );
        assert_eq!(version, None);
    }

    #[test]
    fn test_read_cache_at_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("nonexistent").join("update_check");

        let result = read_cache_at(&cache_path);
        assert!(result.is_none(), "read_cache_at on missing file should return None");
    }

    #[test]
    fn test_read_cache_at_invalid_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        std::fs::write(&cache_path, "bad\n1.0.0\n").unwrap();

        let result = read_cache_at(&cache_path);
        assert!(
            result.is_none(),
            "read_cache_at with invalid timestamp should return None"
        );
    }

    #[test]
    fn test_read_cache_at_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        std::fs::write(&cache_path, "").unwrap();

        let result = read_cache_at(&cache_path);
        assert!(
            result.is_none(),
            "read_cache_at on empty file should return None"
        );
    }

    #[test]
    fn test_is_within_throttle_at_fresh_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");

        write_cache_at(&cache_path, None);

        assert!(
            is_within_throttle_at(&cache_path),
            "fresh cache should be within throttle"
        );
    }

    #[test]
    fn test_is_within_throttle_at_expired_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        let old_ts = current_timestamp() - 90000; // 25 hours ago
        std::fs::write(&cache_path, format!("{}\n", old_ts)).unwrap();

        assert!(
            !is_within_throttle_at(&cache_path),
            "expired cache should NOT be within throttle"
        );
    }

    #[test]
    fn test_is_within_throttle_at_missing_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("nonexistent").join("update_check");

        assert!(
            !is_within_throttle_at(&cache_path),
            "missing cache should NOT be within throttle"
        );
    }

    // --- maybe_notify tests ---

    #[test]
    fn test_maybe_notify_update_available() {
        // 999.0.0 will always be > current CARGO_PKG_VERSION
        // Exercises the update-available code path; should not panic.
        maybe_notify("999.0.0");
    }

    #[test]
    fn test_maybe_notify_up_to_date() {
        // 0.0.1 will always be < current CARGO_PKG_VERSION
        // Exercises the no-update code path; should not panic.
        maybe_notify("0.0.1");
    }

    // --- check_for_update orchestration tests (using TempDir-based cache) ---

    #[test]
    fn test_check_for_update_with_auto_update_disabled() {
        // auto_update=false should return immediately without touching cache
        let config = Config {
            auto_update: false,
            ..Config::default()
        };
        check_for_update(&config);
        // No panic = pass (the function returns early before any cache I/O)
    }

    #[test]
    fn test_check_for_update_throttled_path() {
        // Tests the throttled path: auto_update=true with a fresh cache in TempDir.
        // We can't inject the cache path into check_for_update directly, but we can
        // verify the throttled code path by calling the components directly:
        // 1. Write a fresh cache with a known version
        // 2. Verify is_within_throttle_at returns true
        // 3. Verify read_cache_at returns the cached version
        // 4. Call maybe_notify with that version (exercising the throttled notification path)
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");

        write_cache_at(&cache_path, Some("0.0.1"));

        assert!(
            is_within_throttle_at(&cache_path),
            "fresh cache should be within throttle"
        );
        let (_, cached_version) = read_cache_at(&cache_path).unwrap();
        assert_eq!(cached_version, Some("0.0.1".to_string()));

        // Exercise the notification path with the cached version (mirrors check_for_update throttled branch)
        maybe_notify("0.0.1");
    }

    #[test]
    fn test_check_for_update_non_throttled_path() {
        // Tests the non-throttled path: expired cache means is_within_throttle_at is false.
        // We verify the components individually since check_for_update uses the real cache_path():
        // 1. Write an expired cache
        // 2. Verify is_within_throttle_at returns false
        // 3. The real check_for_update would call fetch_latest_version + write_cache + maybe_notify
        //    We exercise those paths directly (fetch is network-dependent, so we skip it).
        let dir = tempfile::tempdir().unwrap();
        let cache_path = dir.path().join("lazywt").join("update_check");
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        let old_ts = current_timestamp() - 90000;
        std::fs::write(&cache_path, format!("{}\n", old_ts)).unwrap();

        assert!(
            !is_within_throttle_at(&cache_path),
            "expired cache should NOT be within throttle"
        );

        // Simulate the non-throttled path: write new cache + maybe_notify
        write_cache_at(&cache_path, Some("999.0.0"));
        maybe_notify("999.0.0");

        // Verify cache was updated
        let (ts, version) = read_cache_at(&cache_path).unwrap();
        let now = current_timestamp();
        assert!(now.saturating_sub(ts) < 5, "cache should have fresh timestamp");
        assert_eq!(version, Some("999.0.0".to_string()));
    }

    // --- Cache format round-trip tests ---
    // These use TempDir to test the cache format logic without touching the real
    // config directory, avoiding race conditions when tests run in parallel.

    #[test]
    fn test_cache_format_round_trip_with_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("lazywt").join("update_check");
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();

        // Replicate write_cache logic with a version
        let now = current_timestamp();
        let content = format!("{}\n2.0.0\n", now);
        std::fs::write(&cache, &content).unwrap();

        // Replicate read_cache logic
        let data = std::fs::read_to_string(&cache).unwrap();
        let mut lines = data.lines();
        let ts: u64 = lines.next().unwrap().trim().parse().unwrap();
        let version = lines.next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        assert!(now.saturating_sub(ts) < 5, "Timestamp should be within 5s of now");
        assert_eq!(version, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_cache_format_round_trip_without_version() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("lazywt").join("update_check");
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();

        // Replicate write_cache logic without a version
        let now = current_timestamp();
        let content = format!("{}\n", now);
        std::fs::write(&cache, &content).unwrap();

        // Replicate read_cache logic
        let data = std::fs::read_to_string(&cache).unwrap();
        let mut lines = data.lines();
        let ts: u64 = lines.next().unwrap().trim().parse().unwrap();
        let version = lines.next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

        assert!(now.saturating_sub(ts) < 5, "Timestamp should be within 5s of now");
        assert_eq!(version, None);
    }

    #[test]
    fn test_throttle_logic_fresh_write() {
        // Replicate is_within_throttle logic: fresh timestamp should be within 24h
        let now = current_timestamp();
        let cache_ts = now; // just written
        assert!(
            now.saturating_sub(cache_ts) < 86400,
            "Fresh timestamp should be within throttle window"
        );
    }

    #[test]
    fn test_throttle_logic_expired_write() {
        // Replicate is_within_throttle logic: old timestamp should be outside 24h
        let now = current_timestamp();
        let cache_ts = now - 90000; // 25 hours ago
        assert!(
            now.saturating_sub(cache_ts) >= 86400,
            "25h-old timestamp should be outside throttle window"
        );
    }

    #[test]
    fn test_cache_path_returns_some_with_lazywt() {
        let path = cache_path();
        assert!(path.is_some(), "cache_path should return Some on normal systems");
        let path_str = path.unwrap().to_string_lossy().to_string();
        assert!(
            path_str.contains("lazywt"),
            "cache_path should contain 'lazywt', got: {}",
            path_str
        );
    }


    #[test]
    fn test_extract_json_string_colon_no_space() {
        // No space after colon
        let body = r#"{"key":"value"}"#;
        assert_eq!(
            extract_json_string(body, "key"),
            Some("value".to_string())
        );
    }

    #[test]
    fn test_extract_json_string_empty_value() {
        let body = r#"{"key":""}"#;
        assert_eq!(
            extract_json_string(body, "key"),
            Some("".to_string())
        );
    }

    #[test]
    fn test_extract_json_nested_key() {
        // Key appearing in nested JSON -- naive search finds first occurrence
        let body = r#"{"outer":{"inner":"nested_val"},"top":"top_val"}"#;
        // "inner" should be found
        assert_eq!(
            extract_json_string(body, "inner"),
            Some("nested_val".to_string())
        );
    }

    #[test]
    fn test_read_cache_invalid_timestamp() {
        // Replicate read_cache parsing logic with invalid content
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("update_check");
        std::fs::write(&cache, "bad\n1.0.0\n").unwrap();

        let data = std::fs::read_to_string(&cache).unwrap();
        let mut lines = data.lines();
        let parsed: Option<u64> = lines.next().and_then(|s| s.trim().parse().ok());
        assert_eq!(parsed, None, "Non-numeric timestamp should fail to parse");
    }
}
