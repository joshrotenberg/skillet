//! Persistent disk cache for SkillIndex.
//!
//! Each registry gets its own cache file so a single stale registry
//! doesn't invalidate others. The cache stores skill entries as a flat
//! `Vec` (since `HashMap<(String, String), _>` doesn't serialize cleanly
//! to JSON) and reconstructs the full `SkillIndex` on load.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::git;
use crate::state::{SkillEntry, SkillIndex};

/// Bump this to invalidate all caches when the format changes.
const CACHE_VERSION: u32 = 1;

/// Identifies the source of a registry for cache path derivation.
#[derive(Debug)]
pub enum RegistrySource {
    /// A local filesystem registry.
    Local(PathBuf),
    /// A remote (git-backed) registry with its URL and local checkout path.
    Remote { url: String, checkout: PathBuf },
}

/// Serialized cache file.
#[derive(Serialize, Deserialize)]
struct CachedIndex {
    version: u32,
    git_head: Option<String>,
    cached_at: u64,
    skills: Vec<SkillEntry>,
    categories: BTreeMap<String, usize>,
}

/// Compute the cache file path for a registry source.
fn cache_path(source: &RegistrySource) -> PathBuf {
    let base = cache_dir();
    match source {
        RegistrySource::Local(path) => {
            let hex = short_hash(&path.to_string_lossy());
            base.join(format!("local_{hex}.json"))
        }
        RegistrySource::Remote { url, .. } => {
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
            base.join(format!("{slug}.json"))
        }
    }
}

/// Try to load a cached index for the given source.
///
/// Returns `None` on any failure (missing, corrupt, expired, version
/// mismatch, HEAD mismatch). Cache reads are best-effort.
pub fn load(source: &RegistrySource, ttl: Duration) -> Option<SkillIndex> {
    let path = cache_path(source);
    let data = std::fs::read_to_string(&path).ok()?;
    let cached: CachedIndex = serde_json::from_str(&data).ok()?;

    // Version check
    if cached.version != CACHE_VERSION {
        tracing::debug!("Cache version mismatch, ignoring");
        return None;
    }

    // TTL check
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if ttl > Duration::ZERO && now.saturating_sub(cached.cached_at) > ttl.as_secs() {
        tracing::debug!("Cache TTL expired");
        return None;
    }

    // Git HEAD check
    if let Some(ref cached_head) = cached.git_head {
        let repo_path = match source {
            RegistrySource::Local(p) => p.as_path(),
            RegistrySource::Remote { checkout, .. } => checkout.as_path(),
        };
        if repo_path.join(".git").exists()
            && let Ok(current_head) = git::head(repo_path)
            && &current_head != cached_head
        {
            tracing::debug!(
                cached = %cached_head,
                current = %current_head,
                "Cache HEAD mismatch"
            );
            return None;
        }
    }

    // Reconstruct SkillIndex
    let skills = cached
        .skills
        .into_iter()
        .map(|e| ((e.owner.clone(), e.name.clone()), e))
        .collect();
    let index = SkillIndex {
        skills,
        categories: cached.categories,
    };

    tracing::debug!(path = %path.display(), "Loaded index from cache");
    Some(index)
}

/// Write a cached index for the given source.
///
/// Logs warnings on failure but does not propagate errors -- cache
/// writes are best-effort.
pub fn write(source: &RegistrySource, index: &SkillIndex) {
    let path = cache_path(source);

    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(error = %e, "Failed to create cache directory");
        return;
    }

    let git_head = match source {
        RegistrySource::Local(p) => {
            if p.join(".git").exists() {
                git::head(p).ok()
            } else {
                None
            }
        }
        RegistrySource::Remote { checkout, .. } => git::head(checkout).ok(),
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let cached = CachedIndex {
        version: CACHE_VERSION,
        git_head,
        cached_at: now,
        skills: index.skills.values().cloned().collect(),
        categories: index.categories.clone(),
    };

    match serde_json::to_string(&cached) {
        Ok(data) => {
            if let Err(e) = std::fs::write(&path, data) {
                tracing::warn!(error = %e, path = %path.display(), "Failed to write cache");
            } else {
                tracing::debug!(path = %path.display(), "Wrote index cache");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to serialize index cache");
        }
    }
}

/// Remove all index cache files.
pub fn clear() -> anyhow::Result<()> {
    let dir = cache_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// The index cache directory: `~/.cache/skillet/index/`.
fn cache_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".cache")
            .join("skillet")
            .join("index")
    } else {
        PathBuf::from("/tmp").join("skillet").join("index")
    }
}

/// Produce a short hex hash of a string (first 16 chars of SHA-256).
fn short_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SkillEntry, SkillFile, SkillMetadata, SkillVersion};
    use std::collections::HashMap;
    use std::path::Path;

    /// Build a minimal SkillIndex for testing.
    fn test_index() -> SkillIndex {
        let entry = SkillEntry {
            owner: "test-owner".to_string(),
            name: "test-skill".to_string(),
            versions: vec![SkillVersion {
                version: "0.1.0".to_string(),
                metadata: SkillMetadata {
                    skill: crate::state::SkillInfo {
                        name: "test-skill".to_string(),
                        owner: "test-owner".to_string(),
                        version: "0.1.0".to_string(),
                        description: "A test skill".to_string(),
                        trigger: None,
                        license: None,
                        author: None,
                        classification: None,
                        compatibility: None,
                    },
                },
                skill_md: "# Test".to_string(),
                skill_toml_raw: "[skill]\nname = \"test-skill\"".to_string(),
                yanked: false,
                files: HashMap::new(),
                published: None,
                has_content: true,
                content_hash: None,
                integrity_ok: None,
            }],
        };

        let mut index = SkillIndex::default();
        index
            .skills
            .insert(("test-owner".to_string(), "test-skill".to_string()), entry);
        index
    }

    fn temp_source(dir: &Path) -> RegistrySource {
        RegistrySource::Local(dir.to_path_buf())
    }

    #[test]
    fn test_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        // Override cache dir by writing directly to a known path
        let index = test_index();
        let source = temp_source(tmp.path());

        write(&source, &index);
        let loaded = load(&source, Duration::from_secs(300)).unwrap();

        assert_eq!(loaded.skills.len(), 1);
        let key = ("test-owner".to_string(), "test-skill".to_string());
        let entry = loaded.skills.get(&key).unwrap();
        assert_eq!(entry.owner, "test-owner");
        assert_eq!(entry.name, "test-skill");
        assert_eq!(entry.versions.len(), 1);
        assert_eq!(entry.versions[0].version, "0.1.0");
        assert_eq!(entry.versions[0].skill_md, "# Test");
    }

    #[test]
    fn test_version_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let source = temp_source(tmp.path());
        let index = test_index();

        // Write a valid cache
        write(&source, &index);

        // Tamper with the version
        let path = cache_path(&source);
        let mut cached: CachedIndex =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        cached.version = 999;
        std::fs::write(&path, serde_json::to_string(&cached).unwrap()).unwrap();

        assert!(load(&source, Duration::from_secs(300)).is_none());
    }

    #[test]
    fn test_expired_ttl() {
        let tmp = tempfile::tempdir().unwrap();
        let source = temp_source(tmp.path());
        let index = test_index();

        // Write a valid cache
        write(&source, &index);

        // Set cached_at to the past
        let path = cache_path(&source);
        let mut cached: CachedIndex =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        cached.cached_at = 0; // epoch = very old
        std::fs::write(&path, serde_json::to_string(&cached).unwrap()).unwrap();

        // With a short TTL, should be expired
        assert!(load(&source, Duration::from_secs(1)).is_none());
    }

    #[test]
    fn test_head_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("repo");

        // Create a real git repo so git::head works
        std::fs::create_dir_all(&git_dir).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&git_dir)
            .output()
            .unwrap();
        // Set identity for CI
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(&git_dir)
            .output();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "test@test"])
            .current_dir(&git_dir)
            .output();
        std::fs::write(git_dir.join("file.txt"), "a").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&git_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "first"])
            .current_dir(&git_dir)
            .output()
            .unwrap();

        let source = RegistrySource::Local(git_dir.clone());
        let index = test_index();

        // Write cache (captures current HEAD)
        write(&source, &index);

        // Make a new commit so HEAD changes
        std::fs::write(git_dir.join("file.txt"), "b").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&git_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "second"])
            .current_dir(&git_dir)
            .output()
            .unwrap();

        // Cache should be invalidated by HEAD mismatch
        assert!(load(&source, Duration::from_secs(300)).is_none());
    }

    #[test]
    fn test_cache_hit_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let source = temp_source(tmp.path());
        let index = test_index();

        write(&source, &index);

        // Should hit: fresh cache, no git HEAD to check
        let loaded = load(&source, Duration::from_secs(300));
        assert!(loaded.is_some());
    }

    #[test]
    fn test_clear_nonexistent_is_ok() {
        // clear() should not error even if the cache dir doesn't exist.
        // We don't call clear() with real cached data because it would
        // race with concurrent tests using the shared cache directory.
        let dir = cache_dir();
        if !dir.exists() {
            assert!(clear().is_ok());
        }
    }

    #[test]
    fn test_missing_cache_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("nonexistent");
        let source = temp_source(&nonexistent);
        assert!(load(&source, Duration::from_secs(300)).is_none());
    }

    #[test]
    fn test_corrupt_cache_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let source = temp_source(tmp.path());

        // Write garbage to the cache path
        let path = cache_path(&source);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, "not json").unwrap();

        assert!(load(&source, Duration::from_secs(300)).is_none());
    }

    #[test]
    fn test_zero_ttl_always_revalidates() {
        let tmp = tempfile::tempdir().unwrap();
        let source = temp_source(tmp.path());
        let index = test_index();

        write(&source, &index);

        // TTL=0 means always revalidate; no git so no HEAD check,
        // but the TTL check is skipped for Duration::ZERO
        let loaded = load(&source, Duration::ZERO);
        assert!(loaded.is_some());
    }

    #[test]
    fn test_skill_files_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let source = temp_source(tmp.path());

        let mut files = HashMap::new();
        files.insert(
            "scripts/lint.sh".to_string(),
            SkillFile {
                content: "#!/bin/bash\necho hello".to_string(),
                mime_type: "text/x-shellscript".to_string(),
            },
        );

        let entry = SkillEntry {
            owner: "owner".to_string(),
            name: "with-files".to_string(),
            versions: vec![SkillVersion {
                version: "1.0.0".to_string(),
                metadata: SkillMetadata {
                    skill: crate::state::SkillInfo {
                        name: "with-files".to_string(),
                        owner: "owner".to_string(),
                        version: "1.0.0".to_string(),
                        description: "Skill with extra files".to_string(),
                        trigger: None,
                        license: None,
                        author: None,
                        classification: None,
                        compatibility: None,
                    },
                },
                skill_md: "# With Files".to_string(),
                skill_toml_raw: "".to_string(),
                yanked: false,
                files,
                published: Some("2025-01-01T00:00:00Z".to_string()),
                has_content: true,
                content_hash: Some("abc123".to_string()),
                integrity_ok: Some(true),
            }],
        };

        let mut index = SkillIndex::default();
        index
            .skills
            .insert(("owner".to_string(), "with-files".to_string()), entry);

        write(&source, &index);
        let loaded = load(&source, Duration::from_secs(300)).unwrap();

        let key = ("owner".to_string(), "with-files".to_string());
        let loaded_entry = loaded.skills.get(&key).unwrap();
        let v = &loaded_entry.versions[0];
        assert_eq!(v.files.len(), 1);
        assert_eq!(
            v.files.get("scripts/lint.sh").unwrap().content,
            "#!/bin/bash\necho hello"
        );
        assert_eq!(v.published, Some("2025-01-01T00:00:00Z".to_string()));
        assert_eq!(v.content_hash, Some("abc123".to_string()));
        assert_eq!(v.integrity_ok, Some(true));
    }
}
