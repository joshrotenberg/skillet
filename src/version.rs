//! CLI version check against the latest GitHub release.
//!
//! Checks once per day (cached) using `gh api` so there's no additional HTTP
//! dependency. Silently skips if `gh` is not installed or the check fails.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::registry::default_cache_dir;

/// How often to check for a new version (24 hours).
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// GitHub repo to check for releases.
const GITHUB_REPO: &str = "joshrotenberg/grimoire";

#[derive(Debug, Serialize, Deserialize)]
struct VersionCache {
    /// Unix timestamp of the last check.
    checked_at_unix: u64,
    /// Latest release tag (e.g. "v0.2.0" or "0.2.0").
    latest_tag: String,
}

/// Check if a newer version is available and print a message if so.
///
/// This is best-effort: it silently returns if `gh` is missing, the network
/// is down, there are no releases, or the cache can't be written.
pub fn check_and_notify() {
    let current = env!("CARGO_PKG_VERSION");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Try to read the cache first
    if let Some(cache) = read_cache()
        && now.saturating_sub(cache.checked_at_unix) < CHECK_INTERVAL.as_secs()
    {
        // Cache is fresh -- use it
        if let Some(msg) = upgrade_message(current, &cache.latest_tag) {
            eprintln!("{msg}");
        }
        return;
    }

    // Cache is stale or missing -- check GitHub
    let latest_tag = match fetch_latest_release() {
        Some(tag) => tag,
        None => return,
    };

    // Write cache
    let _ = write_cache(&VersionCache {
        checked_at_unix: now,
        latest_tag: latest_tag.clone(),
    });

    if let Some(msg) = upgrade_message(current, &latest_tag) {
        eprintln!("{msg}");
    }
}

/// Fetch the latest release tag from GitHub using `gh api`.
fn fetch_latest_release() -> Option<String> {
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{GITHUB_REPO}/releases/latest"),
            "--jq",
            ".tag_name",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let tag = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if tag.is_empty() {
        return None;
    }
    Some(tag)
}

/// Compare the current version against the latest tag and return an upgrade
/// message if the latest is newer.
fn upgrade_message(current: &str, latest_tag: &str) -> Option<String> {
    let latest = latest_tag.strip_prefix('v').unwrap_or(latest_tag);

    if latest == current {
        return None;
    }

    let current_parts = parse_version(current)?;
    let latest_parts = parse_version(latest)?;

    if latest_parts > current_parts {
        Some(format!(
            "\nA new version of skillet is available: v{latest} (current: v{current})\n\
             Run `cargo install --git https://github.com/{GITHUB_REPO}.git` to upgrade."
        ))
    } else {
        None
    }
}

/// Parse a version string like "0.2.1" into comparable numeric parts.
fn parse_version(v: &str) -> Option<Vec<u64>> {
    v.split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()
}

fn cache_path() -> Option<PathBuf> {
    Some(default_cache_dir().join("version-check.json"))
}

fn read_cache() -> Option<VersionCache> {
    let path = cache_path()?;
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_cache(cache: &VersionCache) -> Option<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok()?;
    }
    let data = serde_json::to_string(cache).ok()?;
    fs::write(path, data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version() {
        assert_eq!(parse_version("0.1.0"), Some(vec![0, 1, 0]));
        assert_eq!(parse_version("1.2.3"), Some(vec![1, 2, 3]));
        assert_eq!(parse_version("invalid"), None);
    }

    #[test]
    fn test_upgrade_message_newer() {
        let msg = upgrade_message("0.1.0", "v0.2.0");
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("v0.2.0"));
    }

    #[test]
    fn test_upgrade_message_same() {
        assert!(upgrade_message("0.1.0", "v0.1.0").is_none());
    }

    #[test]
    fn test_upgrade_message_older() {
        assert!(upgrade_message("0.2.0", "v0.1.0").is_none());
    }

    #[test]
    fn test_upgrade_message_no_prefix() {
        let msg = upgrade_message("0.1.0", "0.2.0");
        assert!(msg.is_some());
    }

    #[test]
    fn test_upgrade_message_patch_version() {
        let msg = upgrade_message("0.1.0", "v0.1.1");
        assert!(msg.is_some());
    }

    #[test]
    fn test_upgrade_message_major_version() {
        let msg = upgrade_message("0.1.0", "v1.0.0");
        assert!(msg.is_some());
    }
}
