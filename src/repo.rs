//! Repo management: initialization, loading, and utility functions.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cache::{self, RepoSource};
use crate::config::SkilletConfig;
use crate::error::Error;
use crate::state::SkillIndex;
use crate::{git, index};

/// The official default repo, used when no repos are configured.
pub const DEFAULT_REPO_URL: &str = "https://github.com/joshrotenberg/skillet.git";

/// Subdirectory within the official repo that contains skills.
pub const DEFAULT_REPO_SUBDIR: &str = "registry";

/// Parse a human-friendly duration string like "5m", "1h", "30s", or "0".
pub fn parse_duration(s: &str) -> crate::error::Result<Duration> {
    let s = s.trim();
    if s == "0" {
        return Ok(Duration::ZERO);
    }

    let (num, suffix) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let num: u64 = num
        .parse()
        .map_err(|_| Error::InvalidDuration(format!("invalid number: {s}")))?;

    let secs = match suffix {
        "s" | "" => num,
        "m" => num * 60,
        "h" => num * 3600,
        _ => {
            return Err(Error::InvalidDuration(format!(
                "unknown suffix: {suffix} (use s, m, or h)"
            )));
        }
    };

    Ok(Duration::from_secs(secs))
}

/// Derive a cache directory from the remote URL.
///
/// Turns `https://github.com/owner/repo.git` into `<base>/owner_repo`.
pub fn cache_dir_for_url(base: &Path, url: &str) -> PathBuf {
    let slug: String = url
        .trim_end_matches(".git")
        .rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("_");

    let slug = if slug.is_empty() {
        "default".to_string()
    } else {
        slug
    };

    base.join(slug)
}

/// Default cache directory for cloned remote repos.
pub fn default_cache_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache").join("skillet")
    } else {
        PathBuf::from("/tmp").join("skillet")
    }
}

/// Load and merge repos from CLI flags and/or config file.
///
/// Priority: if any flags are provided (`repo_flags` or `remote_flags`),
/// use only those. Otherwise fall back to the config file's repos.
/// Errors if no repos are available from either source.
///
/// Returns the merged skill index and the list of repo paths used
/// (needed for repo identification in the installation manifest).
pub fn load_repos(
    repo_flags: &[PathBuf],
    remote_flags: &[String],
    config: &SkilletConfig,
    subdir: Option<&Path>,
) -> crate::error::Result<(SkillIndex, Vec<PathBuf>)> {
    let has_flags = !repo_flags.is_empty() || !remote_flags.is_empty();

    let (local_paths, remote_urls): (Vec<PathBuf>, Vec<&str>) = if has_flags {
        let locals: Vec<PathBuf> = repo_flags
            .iter()
            .map(|p| match subdir {
                Some(sub) => p.join(sub),
                None => p.clone(),
            })
            .collect();
        let remotes: Vec<&str> = remote_flags.iter().map(|s| s.as_str()).collect();
        (locals, remotes)
    } else {
        let locals: Vec<PathBuf> = config
            .repos
            .local
            .iter()
            .map(|p| match subdir {
                Some(sub) => p.join(sub),
                None => p.clone(),
            })
            .collect();
        let remotes: Vec<&str> = config.repos.remote.iter().map(|s| s.as_str()).collect();
        (locals, remotes)
    };

    // Fall back to the official repo if nothing is configured
    let default_remote;
    let remote_urls = if local_paths.is_empty() && remote_urls.is_empty() {
        default_remote = DEFAULT_REPO_URL.to_string();
        vec![default_remote.as_str()]
    } else {
        remote_urls
    };

    let cache_base = default_cache_dir();
    let mut repo_paths = Vec::new();

    let cache_enabled = config.cache.enabled;
    let cache_ttl = if cache_enabled {
        parse_duration(&config.cache.ttl).unwrap_or(Duration::from_secs(300))
    } else {
        Duration::ZERO
    };

    let mut merged = SkillIndex::default();

    // Load local repos
    for path in &local_paths {
        repo_paths.push(path.clone());
        let source = RepoSource::Local(path.clone());

        if cache_enabled && let Some(idx) = cache::load(&source, cache_ttl) {
            merged.merge(idx);
            continue;
        }

        let idx = index::load_index(path)?;
        if cache_enabled {
            cache::write(&source, &idx);
        }
        merged.merge(idx);
    }

    // Clone/pull remote repos
    for url in &remote_urls {
        let target = cache_dir_for_url(&cache_base, url);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        git::clone_or_pull(url, &target)?;
        let path = match subdir {
            Some(sub) => target.join(sub),
            None if *url == DEFAULT_REPO_URL => target.join(DEFAULT_REPO_SUBDIR),
            None => target.clone(),
        };
        repo_paths.push(path.clone());

        let source = RepoSource::Remote {
            url: url.to_string(),
            checkout: target,
        };

        if cache_enabled && let Some(idx) = cache::load(&source, cache_ttl) {
            merged.merge(idx);
            continue;
        }

        let idx = index::load_index(&path)?;
        if cache_enabled {
            cache::write(&source, &idx);
        }
        merged.merge(idx);
    }

    Ok((merged, repo_paths))
}

/// Identify a repo for manifest entries.
///
/// Returns the git URL as-is for remotes, `local:<abs_path>` for local repos.
pub fn repo_id(path: &Path, remote_urls: &[String]) -> String {
    // Check if this path is a cached clone of a remote
    let cache_base = default_cache_dir();
    for url in remote_urls {
        let cached = cache_dir_for_url(&cache_base, url);
        if path.starts_with(&cached) {
            return url.clone();
        }
    }
    format!("local:{}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_duration_zero() {
        assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn test_parse_duration_bare_number() {
        assert_eq!(parse_duration("60").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn test_cache_dir_for_url_github() {
        let base = PathBuf::from("/tmp/skillet");
        let dir = cache_dir_for_url(&base, "https://github.com/owner/repo.git");
        assert_eq!(dir, PathBuf::from("/tmp/skillet/owner_repo"));
    }

    #[test]
    fn test_cache_dir_for_url_no_git_suffix() {
        let base = PathBuf::from("/tmp/skillet");
        let dir = cache_dir_for_url(&base, "https://github.com/owner/repo");
        assert_eq!(dir, PathBuf::from("/tmp/skillet/owner_repo"));
    }

    #[test]
    fn test_cache_dir_for_url_ssh() {
        let base = PathBuf::from("/tmp/skillet");
        let dir = cache_dir_for_url(&base, "git@github.com:owner/repo.git");
        // SSH URLs use ":" not "/", so rsplit('/') gets "git@github.com:owner_repo"
        assert_eq!(dir, PathBuf::from("/tmp/skillet/git@github.com:owner_repo"));
    }

    #[test]
    fn test_cache_dir_for_url_empty() {
        let base = PathBuf::from("/tmp/skillet");
        let dir = cache_dir_for_url(&base, "");
        assert_eq!(dir, PathBuf::from("/tmp/skillet/default"));
    }

    #[test]
    fn test_cache_dir_for_url_single_segment() {
        let base = PathBuf::from("/tmp/skillet");
        let dir = cache_dir_for_url(&base, "https://example.com/repo.git");
        // rsplit('/').take(2) => ["repo", "example.com"] => "example.com_repo"
        assert_eq!(dir, PathBuf::from("/tmp/skillet/example.com_repo"));
    }

    #[test]
    fn test_parse_duration_invalid_suffix() {
        let result = parse_duration("5d");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown suffix"));
    }

    #[test]
    fn test_parse_duration_not_a_number() {
        let result = parse_duration("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_duration_whitespace_trimmed() {
        assert_eq!(parse_duration("  30s  ").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_repo_id_local() {
        let path = PathBuf::from("/home/user/my-registry");
        let id = repo_id(&path, &[]);
        assert_eq!(id, "local:/home/user/my-registry");
    }

    #[test]
    fn test_repo_id_remote() {
        let url = "https://github.com/owner/repo.git".to_string();
        let cache_base = default_cache_dir();
        let cached_path = cache_dir_for_url(&cache_base, &url);

        let id = repo_id(&cached_path, std::slice::from_ref(&url));
        assert_eq!(id, url);
    }

    #[test]
    fn test_default_repo_url_is_set() {
        assert!(!DEFAULT_REPO_URL.is_empty());
        assert!(DEFAULT_REPO_URL.ends_with(".git"));
    }
}
