use anyhow::Result;
use crate::output;
use crate::update_check;

/// Run explicit update check (ignores throttle, shows full output).
/// Per D-31: always checks, does not respect 24h throttle.
pub fn run() -> Result<()> {
    run_with_fetcher(update_check::fetch_latest_version, None)
}

/// Testable run function that accepts an injectable fetcher and optional cache path.
/// The fetcher returns Option<String> -- same signature as fetch_latest_version.
/// When cache_path is None, writes to the default global cache location.
fn run_with_fetcher<F>(fetcher: F, cache_path: Option<&std::path::Path>) -> Result<()>
where
    F: FnOnce() -> Option<String>,
{
    output::info("Checking for updates...");

    match fetcher() {
        Some(remote_version) => {
            let current = env!("CARGO_PKG_VERSION");
            if update_check::is_update_available(current, &remote_version) {
                // Per D-32: notify-only with cargo install hint
                output::success(&format!(
                    "New version {} available (current: {}). Run `cargo install lazywt` to update.",
                    remote_version, current
                ));
            } else {
                output::success(&format!("You are on the latest version ({}).", current));
            }
            // Update cache with latest check
            match cache_path {
                Some(p) => update_check::write_cache_at(p, Some(&remote_version)),
                None => update_check::write_cache(Some(&remote_version)),
            }
        }
        None => {
            output::info("Could not check for updates. You may be offline or the package is not yet published.");
            // Still write timestamp to cache so we don't spam retries
            match cache_path {
                Some(p) => update_check::write_cache_at(p, None),
                None => update_check::write_cache(None),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_run_update_available() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        let result = run_with_fetcher(|| Some("999.0.0".to_string()), Some(&cache));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_up_to_date() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        let result = run_with_fetcher(|| Some(env!("CARGO_PKG_VERSION").to_string()), Some(&cache));
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_fetch_failed() {
        let tmp = TempDir::new().unwrap();
        let cache = tmp.path().join("cache");
        let result = run_with_fetcher(|| None, Some(&cache));
        assert!(result.is_ok());
    }
}
