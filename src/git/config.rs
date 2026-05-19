use std::path::Path;
use anyhow::Result;

/// Read a git config value. Returns None if key not found.
pub fn get_config(dir: &Path, key: &str) -> Result<Option<String>> {
    match super::git(dir, &["config", "--get", key]) {
        Ok(value) => Ok(Some(value.trim().to_string())),
        Err(_) => Ok(None), // key not found is not an error
    }
}

/// Set a git config value.
pub fn set_config(dir: &Path, key: &str, value: &str) -> Result<()> {
    super::git(dir, &["config", key, value])?;
    Ok(())
}
