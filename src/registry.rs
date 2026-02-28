//! Registry management: initialization, loading, and utility functions.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cache::{self, RegistrySource};
use crate::config::SkilletConfig;
use crate::error::Error;
use crate::repo::RepoCatalog;
use crate::state::SkillIndex;
use crate::{git, index};

/// The official default registry, used when no registries are configured.
pub const DEFAULT_REGISTRY_URL: &str = "https://github.com/joshrotenberg/skillet.git";

/// Subdirectory within the official registry repo that contains skills.
pub const DEFAULT_REGISTRY_SUBDIR: &str = "registry";

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

/// Default cache directory for cloned remote registries.
pub fn default_cache_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache").join("skillet")
    } else {
        PathBuf::from("/tmp").join("skillet")
    }
}

/// Initialize a new skill registry at the given path.
///
/// Creates a git repo with registry config, `README.md`, and `.gitignore`,
/// then makes an initial commit. If `description` is provided, it is
/// included in the config. Maintainer info is auto-populated from the
/// user's git config when available.
///
/// Generates a `skillet.toml` with a `[registry]` section.
pub fn init_registry(
    path: &Path,
    name: &str,
    description: Option<&str>,
) -> crate::error::Result<()> {
    std::fs::create_dir_all(path)?;

    // Build registry config content
    let mut config = format!("[registry]\nname = \"{name}\"\nversion = 1\n");

    if let Some(desc) = description {
        config.push_str(&format!("description = \"{desc}\"\n"));
    }

    // Auto-populate maintainer from git config
    let git_name = std::process::Command::new("git")
        .args(["config", "--global", "user.name"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let git_email = std::process::Command::new("git")
        .args(["config", "--global", "user.email"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let github_user = std::process::Command::new("git")
        .args(["config", "--global", "github.user"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    if git_name.is_some() || git_email.is_some() || github_user.is_some() {
        config.push_str("\n[registry.maintainer]\n");
        if let Some(ref n) = git_name {
            config.push_str(&format!("name = \"{n}\"\n"));
        }
        if let Some(ref gh) = github_user {
            config.push_str(&format!("github = \"{gh}\"\n"));
        }
        if let Some(ref e) = git_email {
            config.push_str(&format!("email = \"{e}\"\n"));
        }
    }

    std::fs::write(path.join("skillet.toml"), config)?;

    // README.md
    let readme = format!(
        "# {name}\n\
         \n\
         A skill registry for [skillet](https://github.com/joshrotenberg/skillet).\n\
         \n\
         ## Adding skills\n\
         \n\
         Create a directory for your skill:\n\
         \n\
         ```\n\
         mkdir -p your-name/skill-name\n\
         ```\n\
         \n\
         Add the two required files:\n\
         \n\
         - `skill.toml` -- metadata (name, description, categories, tags)\n\
         - `SKILL.md` -- the skill prompt (Agent Skills spec compatible)\n\
         \n\
         Validate with `skillet validate your-name/skill-name`.\n\
         \n\
         ## Serving\n\
         \n\
         ```bash\n\
         # Local\n\
         skillet --registry .\n\
         \n\
         # Remote (after pushing to git)\n\
         skillet --remote <git-url>\n\
         ```\n",
        name = name
    );
    std::fs::write(path.join("README.md"), readme)?;

    // .gitignore
    std::fs::write(path.join(".gitignore"), ".DS_Store\n")?;

    // git init
    let output = std::process::Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: "init".to_string(),
            stderr,
        });
    }

    // Set local git config if no global identity exists (e.g. in CI)
    let has_identity = std::process::Command::new("git")
        .args(["config", "user.name"])
        .current_dir(path)
        .output()
        .is_ok_and(|o| o.status.success());

    if !has_identity {
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "skillet"])
            .current_dir(path)
            .output();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "skillet@localhost"])
            .current_dir(path)
            .output();
    }

    // initial commit
    let output = std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: "add".to_string(),
            stderr,
        });
    }

    let output = std::process::Command::new("git")
        .args([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Initialize skill registry",
        ])
        .current_dir(path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(Error::Git {
            operation: "commit".to_string(),
            stderr,
        });
    }

    Ok(())
}

/// Load and merge registries from CLI flags and/or config file.
///
/// Priority: if any flags are provided (`registry_flags` or `remote_flags`),
/// use only those. Otherwise fall back to the config file's registries.
/// Errors if no registries are available from either source.
///
/// Returns the merged skill index and the list of registry paths used
/// (needed for registry identification in the installation manifest).
pub fn load_registries(
    registry_flags: &[PathBuf],
    remote_flags: &[String],
    config: &SkilletConfig,
    subdir: Option<&Path>,
) -> crate::error::Result<(SkillIndex, Vec<PathBuf>)> {
    let (index, paths, _catalog) =
        load_registries_with_repos(registry_flags, remote_flags, &[], config, subdir)?;
    Ok((index, paths))
}

/// Load and merge registries, resolving repo short names from the catalog.
///
/// `repo_flags` are short names (e.g. `anthropics/skills`) resolved against
/// the catalog loaded from the official registry.
///
/// Returns `(merged_index, registry_paths, catalog)`. The catalog is loaded
/// from the official registry and can be stored for later use (e.g. on
/// `AppState` for MCP resource templates).
pub fn load_registries_with_repos(
    registry_flags: &[PathBuf],
    remote_flags: &[String],
    repo_flags: &[String],
    config: &SkilletConfig,
    subdir: Option<&Path>,
) -> crate::error::Result<(SkillIndex, Vec<PathBuf>, RepoCatalog)> {
    let has_flags = !registry_flags.is_empty() || !remote_flags.is_empty();

    let (local_paths, remote_urls): (Vec<PathBuf>, Vec<&str>) = if has_flags {
        let locals: Vec<PathBuf> = registry_flags
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
            .registries
            .local
            .iter()
            .map(|p| match subdir {
                Some(sub) => p.join(sub),
                None => p.clone(),
            })
            .collect();
        let remotes: Vec<&str> = config
            .registries
            .remote
            .iter()
            .map(|s| s.as_str())
            .collect();
        (locals, remotes)
    };

    // Fall back to the official registry if nothing is configured
    let default_remote;
    let remote_urls = if local_paths.is_empty() && remote_urls.is_empty() {
        default_remote = DEFAULT_REGISTRY_URL.to_string();
        vec![default_remote.as_str()]
    } else {
        remote_urls
    };

    let cache_base = default_cache_dir();
    let mut registry_paths = Vec::new();

    let cache_enabled = config.cache.enabled;
    let cache_ttl = if cache_enabled {
        parse_duration(&config.cache.ttl).unwrap_or(Duration::from_secs(300))
    } else {
        Duration::ZERO
    };

    let mut merged = SkillIndex::default();

    // Load local registries
    for path in &local_paths {
        registry_paths.push(path.clone());
        let source = RegistrySource::Local(path.clone());

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

    // Clone/pull remote registries
    // Track the first registry root for catalog loading
    let mut official_registry_root: Option<PathBuf> = None;
    for url in &remote_urls {
        let target = cache_dir_for_url(&cache_base, url);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        git::clone_or_pull(url, &target)?;
        let path = match subdir {
            Some(sub) => target.join(sub),
            None if *url == DEFAULT_REGISTRY_URL => target.join(DEFAULT_REGISTRY_SUBDIR),
            None => target.clone(),
        };
        registry_paths.push(path.clone());

        // Remember the official registry root for catalog loading
        if *url == DEFAULT_REGISTRY_URL && official_registry_root.is_none() {
            official_registry_root = Some(path.clone());
        }

        let source = RegistrySource::Remote {
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

    // Load repo catalog from the official registry.
    // If the official registry was already cloned (because it's in the URL
    // list), use that path. Otherwise, clone it separately when we need the
    // catalog to resolve repo short names.
    let has_repos = !repo_flags.is_empty() || !config.registries.repos.is_empty();
    let catalog = if let Some(ref root) = official_registry_root {
        crate::repo::load_repos_catalog(root).unwrap_or_default()
    } else if has_repos {
        let target = cache_dir_for_url(&cache_base, DEFAULT_REGISTRY_URL);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if git::clone_or_pull(DEFAULT_REGISTRY_URL, &target).is_ok() {
            let path = target.join(DEFAULT_REGISTRY_SUBDIR);
            crate::repo::load_repos_catalog(&path).unwrap_or_default()
        } else {
            RepoCatalog::default()
        }
    } else {
        RepoCatalog::default()
    };

    // Resolve repo short names from catalog.
    // Repo names come from CLI flags (if any) or config file.
    let repo_names: Vec<&str> = if !repo_flags.is_empty() {
        repo_flags.iter().map(|s| s.as_str()).collect()
    } else if !config.registries.repos.is_empty() {
        config.registries.repos.iter().map(|s| s.as_str()).collect()
    } else {
        Vec::new()
    };

    resolve_repos(
        &repo_names,
        &catalog,
        &cache_base,
        cache_enabled,
        cache_ttl,
        &mut merged,
        &mut registry_paths,
    );

    Ok((merged, registry_paths, catalog))
}

/// Clone/pull repos from the catalog and merge their indexes.
///
/// Failed clones and unknown repo names are logged and skipped -- this
/// keeps both the CLI and MCP server paths resilient to transient
/// network errors or stale config entries.
pub fn resolve_repos(
    repo_names: &[&str],
    catalog: &RepoCatalog,
    cache_base: &Path,
    cache_enabled: bool,
    cache_ttl: Duration,
    merged: &mut SkillIndex,
    registry_paths: &mut Vec<PathBuf>,
) {
    for repo_name in repo_names {
        let Some(entry) = catalog.find(repo_name) else {
            tracing::warn!(repo = %repo_name, "Unknown repo in catalog, skipping");
            continue;
        };

        let target = cache_dir_for_url(cache_base, &entry.url);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = git::clone_or_pull(&entry.url, &target) {
            tracing::warn!(repo = %repo_name, error = %e, "Failed to clone repo, skipping");
            continue;
        }

        let path = match &entry.subdir {
            Some(sub) => target.join(sub),
            None => target.clone(),
        };
        registry_paths.push(path.clone());

        let source = RegistrySource::Remote {
            url: entry.url.clone(),
            checkout: target,
        };

        if cache_enabled && let Some(idx) = cache::load(&source, cache_ttl) {
            merged.merge(idx);
            continue;
        }

        match index::load_index(&path) {
            Ok(idx) => {
                if cache_enabled {
                    cache::write(&source, &idx);
                }
                merged.merge(idx);
            }
            Err(e) => {
                tracing::warn!(
                    repo = %repo_name,
                    error = %e,
                    "Failed to load repo index, skipping"
                );
            }
        }
    }
}

/// Identify a registry for manifest entries.
///
/// Returns the git URL as-is for remotes, `local:<abs_path>` for local registries.
pub fn registry_id(path: &Path, remote_urls: &[String]) -> String {
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
    fn test_init_registry() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("my-registry");

        init_registry(&registry_path, "my-registry", None).unwrap();

        // skillet.toml exists with correct name (new default)
        let config = std::fs::read_to_string(registry_path.join("skillet.toml")).unwrap();
        assert!(config.contains("name = \"my-registry\""));
        assert!(config.contains("version = 1"));

        // README.md exists
        assert!(registry_path.join("README.md").exists());

        // .gitignore exists
        assert!(registry_path.join(".gitignore").exists());

        // git repo initialized with a commit
        let output = std::process::Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(&registry_path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let log = String::from_utf8_lossy(&output.stdout);
        assert!(log.contains("Initialize skill registry"));

        // Can be loaded as a valid registry
        let loaded_config = crate::index::load_config(&registry_path).unwrap();
        assert_eq!(loaded_config.registry.name, "my-registry");
    }

    #[test]
    fn test_init_registry_with_description() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("desc-registry");

        init_registry(
            &registry_path,
            "desc-registry",
            Some("A test registry with skills"),
        )
        .unwrap();

        let config = std::fs::read_to_string(registry_path.join("skillet.toml")).unwrap();
        assert!(config.contains("description = \"A test registry with skills\""));
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
    fn test_registry_id_local() {
        let path = PathBuf::from("/home/user/my-registry");
        let id = registry_id(&path, &[]);
        assert_eq!(id, "local:/home/user/my-registry");
    }

    #[test]
    fn test_registry_id_remote() {
        let url = "https://github.com/owner/repo.git".to_string();
        let cache_base = default_cache_dir();
        let cached_path = cache_dir_for_url(&cache_base, &url);

        let id = registry_id(&cached_path, std::slice::from_ref(&url));
        assert_eq!(id, url);
    }

    #[test]
    fn test_default_registry_url_is_set() {
        assert!(!DEFAULT_REGISTRY_URL.is_empty());
        assert!(DEFAULT_REGISTRY_URL.ends_with(".git"));
    }
}
