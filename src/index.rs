//! Index loading from a local registry directory
//!
//! Walks the registry directory tree looking for `owner/skill-name/skill.toml`
//! files. Each skill directory is expected to contain:
//! - `skill.toml` (required)
//! - `SKILL.md` (required)

use std::path::Path;

use anyhow::{Context, bail};

use crate::integrity;
use crate::state::{
    RegistryConfig, SkillEntry, SkillFile, SkillIndex, SkillMetadata, SkillVersion,
    VersionsManifest,
};

/// Load registry configuration from `config.toml` at the registry root.
///
/// If the file is absent, returns sensible defaults. If present but
/// malformed, returns an error (fail loud rather than silently defaulting).
pub fn load_config(registry_path: &Path) -> anyhow::Result<RegistryConfig> {
    let config_path = registry_path.join("config.toml");
    if !config_path.is_file() {
        tracing::debug!("No config.toml found, using defaults");
        return Ok(RegistryConfig::default());
    }

    let raw = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: RegistryConfig = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    tracing::info!(name = %config.registry.name, "Loaded registry config");
    Ok(config)
}

/// Load a skill index from a registry directory.
///
/// The directory structure is:
/// ```text
/// registry/
///   owner1/
///     skill-a/
///       skill.toml
///       SKILL.md
///     skill-b/
///       skill.toml
///       SKILL.md
///   owner2/
///     ...
/// ```
pub fn load_index(registry_path: &Path) -> anyhow::Result<SkillIndex> {
    let mut index = SkillIndex::default();

    if !registry_path.is_dir() {
        bail!(
            "Registry path does not exist or is not a directory: {}",
            registry_path.display()
        );
    }

    // Iterate over owner directories
    let mut owners: Vec<_> = std::fs::read_dir(registry_path)
        .with_context(|| {
            format!(
                "Failed to read registry directory: {}",
                registry_path.display()
            )
        })?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    owners.sort_by_key(|e| e.file_name());

    for owner_entry in owners {
        let owner_name = owner_entry.file_name().to_string_lossy().to_string();

        // Skip hidden directories
        if owner_name.starts_with('.') {
            continue;
        }

        // Iterate over skill directories within this owner
        let mut skills: Vec<_> = std::fs::read_dir(owner_entry.path())
            .with_context(|| {
                format!(
                    "Failed to read owner directory: {}",
                    owner_entry.path().display()
                )
            })?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        skills.sort_by_key(|e| e.file_name());

        for skill_entry in skills {
            let skill_dir = skill_entry.path();
            let skill_name = skill_entry.file_name().to_string_lossy().to_string();

            if skill_name.starts_with('.') {
                continue;
            }

            match load_skill(&owner_name, &skill_name, &skill_dir) {
                Ok(entry) => {
                    // Update category counts
                    if let Some(v) = entry.latest()
                        && let Some(ref c) = v.metadata.skill.classification
                    {
                        for cat in &c.categories {
                            *index.categories.entry(cat.clone()).or_insert(0) += 1;
                        }
                    }
                    index.skills.insert((owner_name.clone(), skill_name), entry);
                }
                Err(e) => {
                    tracing::warn!(
                        owner = %owner_name,
                        skill = %skill_name,
                        error = %e,
                        "Skipping skill with invalid metadata"
                    );
                }
            }
        }
    }

    tracing::info!(
        skills = index.skills.len(),
        categories = index.categories.len(),
        "Loaded skill index"
    );

    Ok(index)
}

/// Load a single skill from its directory.
///
/// If `versions.toml` exists, builds a multi-version `SkillEntry` with one
/// `SkillVersion` per record. Only the latest version (last entry) has full
/// content loaded from disk; historical versions are placeholders with
/// `has_content = false`.
///
/// Without `versions.toml`, behaves exactly as before (single version).
fn load_skill(owner: &str, name: &str, dir: &Path) -> anyhow::Result<SkillEntry> {
    let toml_path = dir.join("skill.toml");
    let md_path = dir.join("SKILL.md");

    let skill_toml_raw = std::fs::read_to_string(&toml_path)
        .with_context(|| format!("Failed to read {}", toml_path.display()))?;

    let skill_md = std::fs::read_to_string(&md_path)
        .with_context(|| format!("Failed to read {}", md_path.display()))?;

    let metadata: SkillMetadata = toml::from_str(&skill_toml_raw)
        .with_context(|| format!("Failed to parse {}", toml_path.display()))?;

    // Validate owner/name match directory structure
    if metadata.skill.owner != owner {
        bail!(
            "Owner mismatch: skill.toml says '{}' but directory is '{}'",
            metadata.skill.owner,
            owner
        );
    }
    if metadata.skill.name != name {
        bail!(
            "Name mismatch: skill.toml says '{}' but directory is '{}'",
            metadata.skill.name,
            name
        );
    }

    // Collect extra files from scripts/, references/, assets/
    let files = load_extra_files(dir)?;

    let versions_path = dir.join("versions.toml");
    let versions = if versions_path.is_file() {
        load_versions_manifest(&versions_path, &metadata)?
    } else {
        // Compute content hashes
        let computed = integrity::compute_hashes(&skill_toml_raw, &skill_md, &files);
        let (content_hash, integrity_ok) = verify_manifest(dir, &computed);

        // Single-version backward compat
        vec![SkillVersion {
            version: metadata.skill.version.clone(),
            metadata,
            skill_md,
            skill_toml_raw,
            yanked: false,
            files,
            published: None,
            has_content: true,
            content_hash: Some(content_hash),
            integrity_ok,
        }]
    };

    Ok(SkillEntry {
        owner: owner.to_string(),
        name: name.to_string(),
        versions,
    })
}

/// Parse `versions.toml` and build the version list.
///
/// Returns a vec of `SkillVersion` ordered chronologically (oldest first).
/// The last entry gets full content from the on-disk `skill.toml` + `SKILL.md`.
/// Earlier entries are metadata-only placeholders.
fn load_versions_manifest(
    path: &Path,
    current_metadata: &SkillMetadata,
) -> anyhow::Result<Vec<SkillVersion>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let manifest: VersionsManifest =
        toml::from_str(&raw).with_context(|| format!("Failed to parse {}", path.display()))?;

    if manifest.versions.is_empty() {
        bail!("versions.toml has no entries in {}", path.display());
    }

    // The last entry's version must match skill.toml's version
    let last = manifest.versions.last().unwrap();
    if last.version != current_metadata.skill.version {
        bail!(
            "Version mismatch: last entry in versions.toml is '{}' but skill.toml says '{}'",
            last.version,
            current_metadata.skill.version
        );
    }

    // We need the on-disk content for the last entry. Re-read from the parent
    // directory (path is `dir/versions.toml`, so parent is the skill dir).
    let skill_dir = path.parent().unwrap();
    let toml_path = skill_dir.join("skill.toml");
    let md_path = skill_dir.join("SKILL.md");

    let skill_toml_raw = std::fs::read_to_string(&toml_path)
        .with_context(|| format!("Failed to read {}", toml_path.display()))?;
    let skill_md = std::fs::read_to_string(&md_path)
        .with_context(|| format!("Failed to read {}", md_path.display()))?;
    let files = load_extra_files(skill_dir)?;

    let total = manifest.versions.len();
    let mut versions = Vec::with_capacity(total);

    for (i, record) in manifest.versions.into_iter().enumerate() {
        let is_last = i == total - 1;

        if is_last {
            // Latest version: full content from disk, compute + verify hashes
            let computed = integrity::compute_hashes(&skill_toml_raw, &skill_md, &files);
            let (content_hash, integrity_ok) = verify_manifest(skill_dir, &computed);

            versions.push(SkillVersion {
                version: record.version,
                metadata: current_metadata.clone(),
                skill_md: skill_md.clone(),
                skill_toml_raw: skill_toml_raw.clone(),
                yanked: record.yanked,
                files: files.clone(),
                published: Some(record.published),
                has_content: true,
                content_hash: Some(content_hash),
                integrity_ok,
            });
        } else {
            // Historical version: placeholder metadata, no content
            let placeholder_metadata = SkillMetadata {
                skill: crate::state::SkillInfo {
                    name: current_metadata.skill.name.clone(),
                    owner: current_metadata.skill.owner.clone(),
                    version: record.version.clone(),
                    description: current_metadata.skill.description.clone(),
                    trigger: None,
                    license: None,
                    author: None,
                    classification: None,
                    compatibility: None,
                },
            };
            versions.push(SkillVersion {
                version: record.version,
                metadata: placeholder_metadata,
                skill_md: String::new(),
                skill_toml_raw: String::new(),
                yanked: record.yanked,
                files: std::collections::HashMap::new(),
                published: Some(record.published),
                has_content: false,
                content_hash: None,
                integrity_ok: None,
            });
        }
    }

    Ok(versions)
}

/// Read and verify a `MANIFEST.sha256` file against computed hashes.
///
/// Returns `(composite_hash, integrity_ok)` where `integrity_ok` is:
/// - `None` if no manifest exists (backward compat)
/// - `Some(true)` if hashes match
/// - `Some(false)` if mismatches detected (logged as warnings)
fn verify_manifest(
    skill_dir: &Path,
    computed: &integrity::ContentHashes,
) -> (String, Option<bool>) {
    let manifest_path = skill_dir.join("MANIFEST.sha256");
    let content_hash = computed.composite.clone();

    if !manifest_path.is_file() {
        return (content_hash, None);
    }

    let raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "Failed to read MANIFEST.sha256, skipping verification"
            );
            return (content_hash, None);
        }
    };

    let expected = match integrity::parse_manifest(&raw) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(
                path = %manifest_path.display(),
                error = %e,
                "Failed to parse MANIFEST.sha256, skipping verification"
            );
            return (content_hash, None);
        }
    };

    let mismatches = integrity::verify(computed, &expected);
    if mismatches.is_empty() {
        (content_hash, Some(true))
    } else {
        for m in &mismatches {
            tracing::warn!(
                path = %manifest_path.display(),
                mismatch = %m,
                "Content integrity check failed"
            );
        }
        (content_hash, Some(false))
    }
}

/// Allowed subdirectories in a skillpack (per Agent Skills spec)
const EXTRA_DIRS: &[&str] = &["scripts", "references", "assets"];

/// Load extra files from scripts/, references/, and assets/ subdirectories
fn load_extra_files(
    skill_dir: &Path,
) -> anyhow::Result<std::collections::HashMap<String, SkillFile>> {
    let mut files = std::collections::HashMap::new();

    for subdir_name in EXTRA_DIRS {
        let subdir = skill_dir.join(subdir_name);
        if !subdir.is_dir() {
            continue;
        }

        let entries = std::fs::read_dir(&subdir)
            .with_context(|| format!("Failed to read {}", subdir.display()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = entry.file_name().to_string_lossy().to_string();
            let relative_path = format!("{subdir_name}/{file_name}");

            // Only load text files
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => {
                    tracing::debug!(
                        path = %path.display(),
                        "Skipping non-text file in skillpack"
                    );
                    continue;
                }
            };

            let mime_type = guess_mime_type(&file_name);

            files.insert(relative_path, SkillFile { content, mime_type });
        }
    }

    Ok(files)
}

/// Simple mime type guessing based on file extension
fn guess_mime_type(filename: &str) -> String {
    match filename.rsplit('.').next() {
        Some("md") => "text/markdown",
        Some("sh" | "bash") => "text/x-shellscript",
        Some("py") => "text/x-python",
        Some("js") => "text/javascript",
        Some("ts") => "text/typescript",
        Some("json") => "application/json",
        Some("toml") => "application/toml",
        Some("yaml" | "yml") => "text/yaml",
        _ => "text/plain",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("test-registry")
    }

    #[test]
    fn test_load_index_from_test_registry() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");
        assert!(
            !index.skills.is_empty(),
            "Index should have at least one skill"
        );
    }

    #[test]
    fn test_multi_version_loading() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // rust-dev has versions.toml with 3 versions
        let entry = index
            .skills
            .get(&("joshrotenberg".to_string(), "rust-dev".to_string()))
            .expect("rust-dev should exist");

        assert_eq!(entry.versions.len(), 3);

        // Check ordering: oldest first
        assert_eq!(entry.versions[0].version, "2026.01.01");
        assert_eq!(entry.versions[1].version, "2026.02.01");
        assert_eq!(entry.versions[2].version, "2026.02.24");

        // Historical versions have no content
        assert!(!entry.versions[0].has_content);
        assert!(entry.versions[0].skill_md.is_empty());
        assert!(!entry.versions[1].has_content);
        assert!(entry.versions[1].skill_md.is_empty());

        // Latest has full content
        assert!(entry.versions[2].has_content);
        assert!(!entry.versions[2].skill_md.is_empty());

        // All have published timestamps
        assert!(entry.versions[0].published.is_some());
        assert!(entry.versions[2].published.is_some());

        // latest() returns the last non-yanked version
        let latest = entry.latest().expect("should have latest");
        assert_eq!(latest.version, "2026.02.24");
        assert!(latest.has_content);
    }

    #[test]
    fn test_yanked_version_handling() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // python-dev has 2 versions, first is yanked
        let entry = index
            .skills
            .get(&("acme".to_string(), "python-dev".to_string()))
            .expect("python-dev should exist");

        assert_eq!(entry.versions.len(), 2);
        assert!(entry.versions[0].yanked);
        assert!(!entry.versions[1].yanked);

        // latest() skips yanked versions
        let latest = entry.latest().expect("should have latest");
        assert_eq!(latest.version, "2026.01.15");
    }

    #[test]
    fn test_backward_compat_without_versions_toml() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // skillet/setup has no versions.toml -- should load as single version
        let entry = index
            .skills
            .get(&("skillet".to_string(), "setup".to_string()))
            .expect("skillet/setup should exist");

        assert_eq!(entry.versions.len(), 1);
        assert!(entry.versions[0].has_content);
        assert!(!entry.versions[0].yanked);
        assert!(entry.versions[0].published.is_none());
        assert!(!entry.versions[0].skill_md.is_empty());
    }

    #[test]
    fn test_version_mismatch_validation() {
        // Create a temp dir with mismatched versions
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("testowner").join("testskill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        std::fs::write(
            skill_dir.join("skill.toml"),
            r#"
[skill]
name = "testskill"
owner = "testowner"
version = "2.0.0"
description = "test"
"#,
        )
        .unwrap();

        std::fs::write(skill_dir.join("SKILL.md"), "# Test").unwrap();

        // versions.toml last entry says "1.0.0" but skill.toml says "2.0.0"
        std::fs::write(
            skill_dir.join("versions.toml"),
            r#"
[[versions]]
version = "1.0.0"
published = "2026-01-01T00:00:00Z"
"#,
        )
        .unwrap();

        let result = load_skill("testowner", "testskill", &skill_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Version mismatch"),
            "Expected version mismatch error, got: {err}"
        );
    }

    #[test]
    fn test_load_config_from_test_registry() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let config = load_config(&test_dir).expect("Failed to load config");
        assert_eq!(config.registry.name, "skillet");
        assert_eq!(config.registry.version, 1);
    }

    #[test]
    fn test_load_config_default_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let config = load_config(tmp.path()).expect("Failed to get default config");
        assert_eq!(config.registry.name, "skillet");
        assert_eq!(config.registry.version, 1);
        assert!(config.registry.urls.is_none());
        assert!(config.registry.auth.is_none());
    }

    #[test]
    fn test_load_config_with_full_fields() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("config.toml"),
            r#"
[registry]
name = "my-private-registry"
version = 1

[registry.urls]
download = "https://skills.example.com/packages/{owner}/{name}/{version}.tar.gz"
api = "https://skills.example.com/api/v1"

[registry.auth]
required = true
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).expect("Failed to parse full config");
        assert_eq!(config.registry.name, "my-private-registry");
        assert_eq!(config.registry.version, 1);
        let urls = config.registry.urls.unwrap();
        assert!(urls.download.unwrap().contains("example.com"));
        assert!(urls.api.unwrap().contains("api/v1"));
        assert!(config.registry.auth.unwrap().required);
    }

    #[test]
    fn test_load_config_malformed_fails() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.toml"), "this is not valid toml {{{").unwrap();

        let result = load_config(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_latest_all_yanked_returns_none() {
        let entry = SkillEntry {
            owner: "test".to_string(),
            name: "test".to_string(),
            versions: vec![
                SkillVersion {
                    version: "1.0.0".to_string(),
                    metadata: SkillMetadata {
                        skill: crate::state::SkillInfo {
                            name: "test".to_string(),
                            owner: "test".to_string(),
                            version: "1.0.0".to_string(),
                            description: "test".to_string(),
                            trigger: None,
                            license: None,
                            author: None,
                            classification: None,
                            compatibility: None,
                        },
                    },
                    skill_md: String::new(),
                    skill_toml_raw: String::new(),
                    yanked: true,
                    files: std::collections::HashMap::new(),
                    published: Some("2026-01-01T00:00:00Z".to_string()),
                    has_content: false,
                    content_hash: None,
                    integrity_ok: None,
                },
                SkillVersion {
                    version: "2.0.0".to_string(),
                    metadata: SkillMetadata {
                        skill: crate::state::SkillInfo {
                            name: "test".to_string(),
                            owner: "test".to_string(),
                            version: "2.0.0".to_string(),
                            description: "test".to_string(),
                            trigger: None,
                            license: None,
                            author: None,
                            classification: None,
                            compatibility: None,
                        },
                    },
                    skill_md: "content".to_string(),
                    skill_toml_raw: String::new(),
                    yanked: true,
                    files: std::collections::HashMap::new(),
                    published: Some("2026-02-01T00:00:00Z".to_string()),
                    has_content: true,
                    content_hash: None,
                    integrity_ok: None,
                },
            ],
        };

        assert!(entry.latest().is_none());
    }

    #[test]
    fn test_load_with_manifest_verified() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // code-review has a correct MANIFEST.sha256
        let entry = index
            .skills
            .get(&("joshrotenberg".to_string(), "code-review".to_string()))
            .expect("code-review should exist");

        let latest = entry.latest().expect("should have latest");
        assert!(latest.content_hash.is_some());
        assert_eq!(latest.integrity_ok, Some(true));
    }

    #[test]
    fn test_load_with_manifest_mismatch() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // git-conventions has a deliberately corrupted MANIFEST.sha256
        let entry = index
            .skills
            .get(&("acme".to_string(), "git-conventions".to_string()))
            .expect("git-conventions should exist");

        let latest = entry.latest().expect("should have latest");
        assert!(latest.content_hash.is_some());
        assert_eq!(latest.integrity_ok, Some(false));
    }

    #[test]
    fn test_load_without_manifest() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // skillet/setup has no MANIFEST.sha256
        let entry = index
            .skills
            .get(&("skillet".to_string(), "setup".to_string()))
            .expect("skillet/setup should exist");

        let latest = entry.latest().expect("should have latest");
        assert!(latest.content_hash.is_some());
        assert_eq!(latest.integrity_ok, None);
    }
}
