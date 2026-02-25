//! Registry management: initialization, loading, and utility functions.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::SkilletConfig;
use crate::state::SkillIndex;
use crate::{git, index};

/// Parse a human-friendly duration string like "5m", "1h", "30s", or "0".
pub fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let s = s.trim();
    if s == "0" {
        return Ok(Duration::ZERO);
    }

    let (num, suffix) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let num: u64 = num
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid duration number: {s}"))?;

    let secs = match suffix {
        "s" | "" => num,
        "m" => num * 60,
        "h" => num * 3600,
        _ => anyhow::bail!("Unknown duration suffix: {suffix} (use s, m, or h)"),
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
/// Creates a git repo with `config.toml`, `README.md`, and `.gitignore`,
/// then makes an initial commit.
pub fn init_registry(path: &Path, name: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(path)?;

    // config.toml
    let config = format!("[registry]\nname = \"{name}\"\nversion = 1\n", name = name);
    std::fs::write(path.join("config.toml"), config)?;

    // README.md
    let readme = format!(
        "# {name}\n\
         \n\
         A skill registry for [skillet](https://github.com/joshrotenberg/grimoire).\n\
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git init failed: {stderr}");
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git add failed: {stderr}");
    }

    let output = std::process::Command::new("git")
        .args(["commit", "-m", "Initialize skill registry"])
        .current_dir(path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git commit failed: {stderr}");
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
) -> anyhow::Result<(SkillIndex, Vec<PathBuf>)> {
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

    if local_paths.is_empty() && remote_urls.is_empty() {
        anyhow::bail!(
            "No registries configured. Use --registry, --remote, \
             or add registries to ~/.config/skillet/config.toml"
        );
    }

    let cache_base = default_cache_dir();
    let mut registry_paths = Vec::new();

    // Add local registries
    for path in &local_paths {
        registry_paths.push(path.clone());
    }

    // Clone/pull remote registries
    for url in &remote_urls {
        let target = cache_dir_for_url(&cache_base, url);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        git::clone_or_pull(url, &target)?;
        let path = match subdir {
            Some(sub) => target.join(sub),
            None => target,
        };
        registry_paths.push(path);
    }

    // Load and merge all indexes
    let mut merged = SkillIndex::default();
    for path in &registry_paths {
        let idx = index::load_index(path)?;
        merged.merge(idx);
    }

    Ok((merged, registry_paths))
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

        init_registry(&registry_path, "my-registry").unwrap();

        // config.toml exists with correct name
        let config = std::fs::read_to_string(registry_path.join("config.toml")).unwrap();
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
}
