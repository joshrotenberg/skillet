//! CLI configuration: config file loading and shared utilities.
//!
//! The skillet config file lives at `~/.config/skillet/config.toml` and controls
//! repos, server behavior, and other CLI settings.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Top-level skillet CLI configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkilletConfig {
    #[serde(alias = "registries")]
    pub repos: ReposConfig,
    pub cache: CacheConfig,
    pub server: ServerConfig,
}

/// `[server]` section: MCP server tool/resource exposure control.
///
/// Empty lists (the default) mean "expose all". When non-empty, only the
/// listed capabilities are registered.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Tool short names to expose. Empty = all.
    pub tools: Vec<String>,
    /// Resource short names to expose. Empty = all.
    pub resources: Vec<String>,
}

/// `[cache]` section: disk cache for the skill index.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Whether disk caching is enabled.
    pub enabled: bool,
    /// Time-to-live for cached index files (e.g. "5m", "1h", "0").
    pub ttl: String,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl: "5m".to_string(),
        }
    }
}

/// `[repos]` section: default local and remote repos.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReposConfig {
    pub local: Vec<PathBuf>,
    pub remote: Vec<String>,
    /// Whether to follow `[[suggest]]` entries from loaded repos (default: true).
    #[serde(default = "default_true")]
    pub follow_suggestions: bool,
    /// Maximum depth for following suggestion links (default: 1).
    #[serde(default = "default_suggest_depth")]
    pub suggest_depth: u32,
}

impl Default for ReposConfig {
    fn default() -> Self {
        Self {
            local: Vec::new(),
            remote: Vec::new(),
            follow_suggestions: true,
            suggest_depth: 1,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_suggest_depth() -> u32 {
    1
}

/// Load CLI configuration from `~/.config/skillet/config.toml`.
///
/// Returns defaults if the file is absent. Errors if present but malformed.
pub fn load_config() -> crate::error::Result<SkilletConfig> {
    let path = config_dir().join("config.toml");
    if !path.is_file() {
        return Ok(SkilletConfig::default());
    }
    load_config_from(&path)
}

/// Load CLI configuration from a specific path (for testing).
pub fn load_config_from(path: &Path) -> crate::error::Result<SkilletConfig> {
    let raw = std::fs::read_to_string(path).map_err(|e| Error::ConfigRead {
        path: path.to_path_buf(),
        source: e,
    })?;
    let config: SkilletConfig = toml::from_str(&raw).map_err(|e| Error::ConfigParse {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(config)
}

/// The skillet config directory: `~/.config/skillet/`.
pub fn config_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("skillet")
    } else {
        PathBuf::from("/tmp").join("skillet").join("config")
    }
}

/// Add a remote repo URL to the config. Returns false if already present.
pub fn add_remote(config: &mut SkilletConfig, url: &str) -> bool {
    if config.repos.remote.iter().any(|r| r == url) {
        return false;
    }
    config.repos.remote.push(url.to_string());
    true
}

/// Add a local repo path to the config. Returns false if already present.
pub fn add_local(config: &mut SkilletConfig, path: &Path) -> bool {
    let canonical = path.to_path_buf();
    if config.repos.local.iter().any(|p| p == &canonical) {
        return false;
    }
    config.repos.local.push(canonical);
    true
}

/// Remove a remote repo URL from the config. Returns false if not found.
pub fn remove_remote(config: &mut SkilletConfig, url: &str) -> bool {
    let before = config.repos.remote.len();
    config.repos.remote.retain(|r| r != url);
    config.repos.remote.len() < before
}

/// Remove a local repo path from the config. Returns false if not found.
pub fn remove_local(config: &mut SkilletConfig, path: &Path) -> bool {
    let before = config.repos.local.len();
    config.repos.local.retain(|p| p != path);
    config.repos.local.len() < before
}

/// Write a `SkilletConfig` to the default config path. Returns the path written.
pub fn write_config(config: &SkilletConfig) -> crate::error::Result<PathBuf> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::Io {
        context: format!("create config dir {}", dir.display()),
        source: e,
    })?;

    let path = dir.join("config.toml");
    let content = toml::to_string_pretty(config).map_err(|e| Error::Config(e.to_string()))?;
    std::fs::write(&path, content).map_err(|e| Error::Io {
        context: format!("write config to {}", path.display()),
        source: e,
    })?;
    Ok(path)
}

/// Current time as ISO 8601 string (UTC).
///
/// Uses `std::time` to avoid adding a chrono dependency.
pub fn now_iso8601() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from days since epoch (algorithm from Howard Hinnant)
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_defaults_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let config = SkilletConfig::default();
        assert!(config.repos.local.is_empty());
        assert!(config.repos.remote.is_empty());
        assert!(load_config_from(&path).is_err());
    }

    #[test]
    fn test_parse_full_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[registries]
local = ["/path/to/local"]
remote = ["https://github.com/owner/repo.git"]
"#,
        )
        .unwrap();

        let config = load_config_from(&path).unwrap();
        assert_eq!(config.repos.local, vec![PathBuf::from("/path/to/local")]);
        assert_eq!(
            config.repos.remote,
            vec!["https://github.com/owner/repo.git"]
        );
    }

    #[test]
    fn test_malformed_toml_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml {{{").unwrap();
        assert!(load_config_from(&path).is_err());
    }

    #[test]
    fn test_now_iso8601_format() {
        let ts = now_iso8601();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn test_repos_config_suggestion_defaults() {
        let config = ReposConfig::default();
        assert!(config.follow_suggestions);
        assert_eq!(config.suggest_depth, 1);
    }

    #[test]
    fn test_repos_config_suggestion_from_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[repos]
follow_suggestions = false
suggest_depth = 3
"#,
        )
        .unwrap();

        let config = load_config_from(&path).unwrap();
        assert!(!config.repos.follow_suggestions);
        assert_eq!(config.repos.suggest_depth, 3);
    }

    #[test]
    fn test_server_config_defaults_empty() {
        let config = SkilletConfig::default();
        assert!(config.server.tools.is_empty());
        assert!(config.server.resources.is_empty());
    }

    #[test]
    fn test_server_config_parses() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[server]
tools = ["search", "categories"]
resources = ["skills", "metadata"]
"#,
        )
        .unwrap();

        let config = load_config_from(&path).unwrap();
        assert_eq!(config.server.tools, vec!["search", "categories"]);
        assert_eq!(config.server.resources, vec!["skills", "metadata"]);
    }

    #[test]
    fn test_add_remote_deduplicates() {
        let mut config = SkilletConfig::default();
        assert!(add_remote(&mut config, "https://github.com/a/b.git"));
        assert!(!add_remote(&mut config, "https://github.com/a/b.git"));
        assert_eq!(config.repos.remote.len(), 1);
    }

    #[test]
    fn test_add_local_deduplicates() {
        let mut config = SkilletConfig::default();
        let path = PathBuf::from("/tmp/repo");
        assert!(add_local(&mut config, &path));
        assert!(!add_local(&mut config, &path));
        assert_eq!(config.repos.local.len(), 1);
    }

    #[test]
    fn test_remove_remote() {
        let mut config = SkilletConfig::default();
        add_remote(&mut config, "https://example.com/repo.git");
        assert!(remove_remote(&mut config, "https://example.com/repo.git"));
        assert!(config.repos.remote.is_empty());
        assert!(!remove_remote(&mut config, "https://example.com/repo.git"));
    }

    #[test]
    fn test_remove_local() {
        let mut config = SkilletConfig::default();
        let path = PathBuf::from("/tmp/repo");
        add_local(&mut config, &path);
        assert!(remove_local(&mut config, &path));
        assert!(config.repos.local.is_empty());
        assert!(!remove_local(&mut config, &path));
    }
}
