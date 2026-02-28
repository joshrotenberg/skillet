//! Index loading from a local registry directory
//!
//! Walks the registry directory tree looking for skill directories containing
//! `skill.toml`. Supports both flat (`owner/skill-name/`) and nested
//! (`owner/group/subgroup/skill-name/`) layouts. Intermediate directories
//! without `skill.toml` are recursed into up to `MAX_NESTING_DEPTH` levels
//! below the owner.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::integrity;
use crate::project;
use crate::state::{
    RegistryConfig, SkillEntry, SkillFile, SkillIndex, SkillMetadata, SkillSource, SkillVersion,
    VersionsManifest,
};
use crate::validate;

/// Maximum directory depth below an owner to search for skill directories.
/// Prevents runaway recursion on malformed registry trees.
const MAX_NESTING_DEPTH: usize = 5;

/// Load registry configuration from the registry root.
///
/// Looks for `skillet.toml` with a `[registry]` section. If not present,
/// returns sensible defaults. If the file exists but is malformed, returns
/// an error.
pub fn load_config(registry_path: &Path) -> crate::error::Result<RegistryConfig> {
    if let Some(manifest) = project::load_skillet_toml(registry_path)?
        && let Some(config) = manifest.into_registry_config()
    {
        tracing::info!(name = %config.registry.name, "Loaded registry config from skillet.toml");
        return Ok(config);
    }

    tracing::debug!("No skillet.toml [registry] found, using defaults");
    Ok(RegistryConfig::default())
}

/// Load a skill index from a registry directory.
///
/// Supports both flat and nested layouts:
/// ```text
/// registry/
///   owner1/
///     skill-a/          # flat: owner1/skill-a/
///       skill.toml
///       SKILL.md
///     lang/
///       java/
///         maven-build/  # nested: owner1/lang/java/maven-build/
///           skill.toml
///           SKILL.md
///   owner2/
///     ...
/// ```
///
/// Intermediate directories (without `skill.toml`) are recursed into up to
/// `MAX_NESTING_DEPTH` levels. If two nested paths under the same owner
/// produce the same skill name, the first one wins and a warning is logged.
pub fn load_index(registry_path: &Path) -> crate::error::Result<SkillIndex> {
    let mut index = SkillIndex::default();

    if !registry_path.is_dir() {
        return Err(Error::SkillLoad {
            path: registry_path.to_path_buf(),
            reason: "registry path does not exist or is not a directory".to_string(),
        });
    }

    // Check for skillet.toml with [skill] or [skills] sections.
    // This bridges npm-style skill repos (flat skills/<name>/ layout)
    // so they work as registries without the owner/skill/ nesting.
    if let Some(manifest) = project::load_skillet_toml(registry_path)?
        && (manifest.skill.is_some() || manifest.skills.is_some())
    {
        tracing::info!(
            path = %registry_path.display(),
            "Loading npm-style skill repo via skillet.toml manifest"
        );
        let embedded = project::load_embedded_skills(registry_path, &manifest);
        // Update category counts from embedded index
        for entry in embedded.skills.values() {
            if let Some(v) = entry.latest()
                && let Some(ref c) = v.metadata.skill.classification
            {
                for cat in &c.categories {
                    *index.categories.entry(cat.clone()).or_insert(0) += 1;
                }
            }
        }
        index.skills = embedded.skills;
        return Ok(index);
    }

    // Iterate over owner directories
    let mut owners: Vec<_> = std::fs::read_dir(registry_path)
        .map_err(|e| Error::FileRead {
            path: registry_path.to_path_buf(),
            source: e,
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

        // Recursively find all skill directories under this owner
        let skill_dirs = find_skill_dirs(&owner_entry.path(), MAX_NESTING_DEPTH);

        for skill_dir in skill_dirs {
            // Compute the relative path from the owner directory
            let rel_from_owner = skill_dir
                .strip_prefix(owner_entry.path())
                .unwrap_or(&skill_dir);
            let skill_name = skill_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // Determine registry_path: None for flat (depth 1), Some for nested
            let depth = rel_from_owner.components().count();
            let registry_path_value = if depth > 1 {
                // Full path from registry root: owner/group/.../skill-name
                let full_rel = registry_path.join(&owner_name).join(rel_from_owner);
                // Use forward slashes for portability
                Some(
                    full_rel
                        .strip_prefix(registry_path)
                        .unwrap_or(&full_rel)
                        .to_string_lossy()
                        .replace('\\', "/"),
                )
            } else {
                None
            };

            match load_skill(&owner_name, &skill_name, &skill_dir) {
                Ok(mut entry) => {
                    // Check for collision: same (owner, name) from a different path
                    let key = (owner_name.clone(), skill_name.clone());
                    if let Some(existing) = index.skills.get(&key) {
                        tracing::warn!(
                            owner = %owner_name,
                            name = %skill_name,
                            existing_path = ?existing.registry_path,
                            new_path = ?registry_path_value,
                            "Duplicate skill name under same owner, keeping first"
                        );
                        continue;
                    }

                    entry.registry_path = registry_path_value;

                    // Update category counts
                    if let Some(v) = entry.latest()
                        && let Some(ref c) = v.metadata.skill.classification
                    {
                        for cat in &c.categories {
                            *index.categories.entry(cat.clone()).or_insert(0) += 1;
                        }
                    }
                    index.skills.insert(key, entry);
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

    // Flat-repo fallback: external repos (Anthropic, Vercel, etc.) use a flat
    // `skill-name/SKILL.md` layout without owner nesting.  When the traditional
    // owner/name walk found nothing, try treating immediate children as skills
    // and infer the owner from the git remote (or fall back to the directory name).
    if index.skills.is_empty() {
        let flat_skills = find_skill_dirs(registry_path, 0);
        if !flat_skills.is_empty() {
            // Walk up from registry_path to find the git root (handles subdir case)
            let git_root = find_git_root(registry_path).unwrap_or(registry_path.to_path_buf());
            let owner = project::owner_from_git_remote(&git_root).unwrap_or_else(|| {
                registry_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });

            tracing::info!(
                owner = %owner,
                count = flat_skills.len(),
                "Flat-repo fallback: loading skills without owner nesting"
            );

            for skill_dir in flat_skills {
                let skill_name = skill_dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                match load_skill(&owner, &skill_name, &skill_dir) {
                    Ok(mut entry) => {
                        let key = (owner.clone(), skill_name.clone());
                        if index.skills.contains_key(&key) {
                            continue;
                        }
                        if let Some(v) = entry.latest()
                            && let Some(ref c) = v.metadata.skill.classification
                        {
                            for cat in &c.categories {
                                *index.categories.entry(cat.clone()).or_insert(0) += 1;
                            }
                        }
                        entry.source = SkillSource::Registry;
                        index.skills.insert(key, entry);
                    }
                    Err(e) => {
                        tracing::warn!(
                            owner = %owner,
                            skill = %skill_name,
                            error = %e,
                            "Flat fallback: skipping skill with invalid metadata"
                        );
                    }
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

/// Walk up from a path to find the nearest directory containing `.git`.
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Recursively find skill directories (those containing `skill.toml` or `SKILL.md`).
///
/// Walks `dir` looking for subdirectories that contain `skill.toml` (preferred)
/// or at minimum `SKILL.md` (lenient / zero-config mode). If a directory has
/// either marker, it is collected and not recursed further.
/// If it doesn't and `remaining_depth > 0`, recurse into it.
/// Hidden directories (starting with `.`) are always skipped.
fn find_skill_dirs(dir: &Path, remaining_depth: usize) -> Vec<PathBuf> {
    let mut result = Vec::new();

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return result,
    };

    let mut subdirs: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    subdirs.sort_by_key(|e| e.file_name());

    for entry in subdirs {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        if path.join("skill.toml").is_file() || path.join("SKILL.md").is_file() {
            // This is a skill directory -- collect it, don't recurse further
            result.push(path);
        } else if remaining_depth > 0 {
            // Intermediate grouping directory -- recurse
            result.extend(find_skill_dirs(&path, remaining_depth - 1));
        } else {
            tracing::debug!(
                path = %path.display(),
                "Skipping directory: max nesting depth reached"
            );
        }
    }

    result
}

/// Load a single skill from its directory.
///
/// Uses `validate::validate_skillpack()` for core parsing and validation
/// when `skill.toml` is present, or `validate::validate_skillpack_lenient()`
/// for SKILL.md-only directories (zero-config mode).
///
/// Then layers on registry-specific checks (owner/name match directory
/// structure, versions.toml handling).
///
/// If `versions.toml` exists, builds a multi-version `SkillEntry` with one
/// `SkillVersion` per record. Only the latest version (last entry) has full
/// content loaded from disk; historical versions are placeholders with
/// `has_content = false`.
///
/// Without `versions.toml`, behaves exactly as before (single version).
fn load_skill(owner: &str, name: &str, dir: &Path) -> crate::error::Result<SkillEntry> {
    let has_skill_toml = dir.join("skill.toml").is_file();

    let validated = if has_skill_toml {
        validate::validate_skillpack(dir)?
    } else {
        // SKILL.md-only: use lenient validation with no manifest context
        validate::validate_skillpack_lenient(dir, None)?
    };

    // Registry-specific: owner/name must match directory structure
    // For lenient mode, the inferred name comes from the directory so it
    // will always match; only check when skill.toml exists (strict mode).
    if has_skill_toml {
        if validated.owner != owner {
            return Err(Error::SkillLoad {
                path: dir.to_path_buf(),
                reason: format!(
                    "owner mismatch: skill.toml says '{}' but directory is '{}'",
                    validated.owner, owner
                ),
            });
        }
        if validated.name != name {
            return Err(Error::SkillLoad {
                path: dir.to_path_buf(),
                reason: format!(
                    "name mismatch: skill.toml says '{}' but directory is '{}'",
                    validated.name, name
                ),
            });
        }
    }

    let versions_path = dir.join("versions.toml");
    let versions = if versions_path.is_file() {
        load_versions_manifest(&versions_path, &validated.metadata)?
    } else {
        // Single-version backward compat
        vec![SkillVersion {
            version: validated.version,
            metadata: validated.metadata,
            skill_md: validated.skill_md,
            skill_toml_raw: validated.skill_toml_raw,
            yanked: false,
            files: validated.files,
            published: None,
            has_content: true,
            content_hash: Some(validated.hashes.composite),
            integrity_ok: validated.manifest_ok,
        }]
    };

    Ok(SkillEntry {
        owner: owner.to_string(),
        name: name.to_string(),
        registry_path: None,
        versions,
        source: SkillSource::default(),
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
) -> crate::error::Result<Vec<SkillVersion>> {
    let raw = std::fs::read_to_string(path).map_err(|e| Error::FileRead {
        path: path.to_path_buf(),
        source: e,
    })?;
    let manifest: VersionsManifest = toml::from_str(&raw).map_err(|e| Error::TomlParse {
        path: path.to_path_buf(),
        source: e,
    })?;

    if manifest.versions.is_empty() {
        return Err(Error::SkillLoad {
            path: path.to_path_buf(),
            reason: "versions.toml has no entries".to_string(),
        });
    }

    // The last entry's version must match skill.toml's version
    let last = manifest.versions.last().unwrap();
    if last.version != current_metadata.skill.version {
        return Err(Error::SkillLoad {
            path: path.to_path_buf(),
            reason: format!(
                "Version mismatch: last entry in versions.toml is '{}' but skill.toml says '{}'",
                last.version, current_metadata.skill.version
            ),
        });
    }

    // We need the on-disk content for the last entry. Re-read from the parent
    // directory (path is `dir/versions.toml`, so parent is the skill dir).
    let skill_dir = path.parent().unwrap();
    let toml_path = skill_dir.join("skill.toml");
    let md_path = skill_dir.join("SKILL.md");

    let skill_toml_raw = std::fs::read_to_string(&toml_path).map_err(|e| Error::FileRead {
        path: toml_path.clone(),
        source: e,
    })?;
    let skill_md = std::fs::read_to_string(&md_path).map_err(|e| Error::FileRead {
        path: md_path.clone(),
        source: e,
    })?;
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

/// Allowed subdirectories in a skillpack (per Agent Skills spec).
///
/// Includes `rules/` and `templates/` for compatibility with npm-style
/// skill repos (redis/agent-skills, anthropics/skills, etc.).
pub const EXTRA_DIRS: &[&str] = &["scripts", "references", "assets", "rules", "templates"];

/// Load extra files from scripts/, references/, and assets/ subdirectories.
pub fn load_extra_files(
    skill_dir: &Path,
) -> crate::error::Result<std::collections::HashMap<String, SkillFile>> {
    let mut files = std::collections::HashMap::new();

    for subdir_name in EXTRA_DIRS {
        let subdir = skill_dir.join(subdir_name);
        if !subdir.is_dir() {
            continue;
        }

        let entries = std::fs::read_dir(&subdir).map_err(|e| Error::FileRead {
            path: subdir.clone(),
            source: e,
        })?;

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

/// Simple mime type guessing based on file extension.
pub fn guess_mime_type(filename: &str) -> String {
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

        // New extended fields
        assert_eq!(
            config.registry.description.as_deref(),
            Some("Test registry for skillet development")
        );

        let maintainer = config.registry.maintainer.as_ref().unwrap();
        assert_eq!(maintainer.name.as_deref(), Some("Josh Rotenberg"));
        assert_eq!(maintainer.github.as_deref(), Some("joshrotenberg"));
        assert!(maintainer.email.is_none());

        let suggests = config.registry.suggests.as_ref().unwrap();
        assert_eq!(suggests.len(), 1);
        assert!(suggests[0].url.contains("skillet"));
        assert!(suggests[0].description.is_some());

        let defaults = config.registry.defaults.as_ref().unwrap();
        assert_eq!(defaults.refresh_interval.as_deref(), Some("10m"));
    }

    #[test]
    fn test_load_config_default_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let config = load_config(tmp.path()).expect("Failed to get default config");
        assert_eq!(config.registry.name, "skillet");
        assert_eq!(config.registry.version, 1);
        assert!(config.registry.urls.is_none());
        assert!(config.registry.auth.is_none());
        assert!(config.registry.description.is_none());
        assert!(config.registry.maintainer.is_none());
        assert!(config.registry.suggests.is_none());
        assert!(config.registry.defaults.is_none());
    }

    #[test]
    fn test_load_config_with_full_fields() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
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
        std::fs::write(
            tmp.path().join("skillet.toml"),
            "this is not valid toml {{{",
        )
        .unwrap();

        let result = load_config(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_with_all_extended_fields() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[registry]
name = "acme-team-skills"
version = 1
description = "Internal Python and DevOps skills"

[registry.maintainer]
name = "Jane Doe"
github = "janedoe"
email = "jane@example.com"

[registry.urls]
download = "https://skills.example.com/packages"
api = "https://skills.example.com/api/v1"

[registry.auth]
required = true

[[registry.suggests]]
url = "https://github.com/joshrotenberg/skillet.git"
description = "Official community skills"

[[registry.suggests]]
url = "https://github.com/acme/devops-skills.git"

[registry.defaults]
refresh_interval = "10m"
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).expect("Failed to parse config with all fields");
        assert_eq!(config.registry.name, "acme-team-skills");
        assert_eq!(
            config.registry.description.as_deref(),
            Some("Internal Python and DevOps skills")
        );

        let m = config.registry.maintainer.as_ref().unwrap();
        assert_eq!(m.name.as_deref(), Some("Jane Doe"));
        assert_eq!(m.github.as_deref(), Some("janedoe"));
        assert_eq!(m.email.as_deref(), Some("jane@example.com"));

        assert!(config.registry.urls.is_some());
        assert!(config.registry.auth.unwrap().required);

        let suggests = config.registry.suggests.as_ref().unwrap();
        assert_eq!(suggests.len(), 2);
        assert!(suggests[0].url.contains("skillet"));
        assert_eq!(
            suggests[0].description.as_deref(),
            Some("Official community skills")
        );
        assert!(suggests[1].url.contains("devops-skills"));
        assert!(suggests[1].description.is_none());

        let defaults = config.registry.defaults.as_ref().unwrap();
        assert_eq!(defaults.refresh_interval.as_deref(), Some("10m"));
    }

    #[test]
    fn test_load_config_with_partial_extended_fields() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[registry]
name = "minimal-plus"
version = 1
description = "Just a description, nothing else new"
"#,
        )
        .unwrap();

        let config = load_config(tmp.path()).expect("Failed to parse partial config");
        assert_eq!(config.registry.name, "minimal-plus");
        assert_eq!(
            config.registry.description.as_deref(),
            Some("Just a description, nothing else new")
        );
        assert!(config.registry.maintainer.is_none());
        assert!(config.registry.suggests.is_none());
        assert!(config.registry.defaults.is_none());
    }

    #[test]
    fn test_load_config_minimal() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            "[registry]\nname = \"minimal\"\nversion = 1\n",
        )
        .unwrap();

        let config = load_config(tmp.path()).expect("Failed to parse minimal config");
        assert_eq!(config.registry.name, "minimal");
        assert_eq!(config.registry.version, 1);
        assert!(config.registry.description.is_none());
        assert!(config.registry.maintainer.is_none());
        assert!(config.registry.suggests.is_none());
        assert!(config.registry.defaults.is_none());
    }

    #[test]
    fn test_init_registry_with_description() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("described-registry");

        crate::registry::init_registry(
            &registry_path,
            "described-registry",
            Some("A test registry with a description"),
        )
        .unwrap();

        let config = load_config(&registry_path).expect("Failed to load init'd config");
        assert_eq!(config.registry.name, "described-registry");
        assert_eq!(
            config.registry.description.as_deref(),
            Some("A test registry with a description")
        );
    }

    #[test]
    fn test_latest_all_yanked_returns_none() {
        let entry = SkillEntry {
            owner: "test".to_string(),
            name: "test".to_string(),
            registry_path: None,
            source: SkillSource::default(),
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

    // ── Nested skill discovery tests ─────────────────────────────────

    #[test]
    fn test_nested_skill_discovery() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // Nested skills should be found
        assert!(
            index
                .skills
                .contains_key(&("acme".to_string(), "maven-build".to_string())),
            "nested maven-build should be discovered"
        );
        assert!(
            index
                .skills
                .contains_key(&("acme".to_string(), "gradle-build".to_string())),
            "nested gradle-build should be discovered"
        );

        // Flat skills should still work
        assert!(
            index
                .skills
                .contains_key(&("acme".to_string(), "python-dev".to_string())),
            "flat python-dev should still be discovered"
        );
    }

    #[test]
    fn test_nested_skill_categories_indexed() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // Categories from nested skills should be in the index
        assert!(
            index.categories.contains_key("java"),
            "java category from nested skills should be indexed: {:?}",
            index.categories
        );
        assert!(
            index.categories.contains_key("build-tools"),
            "build-tools category from nested skills should be indexed: {:?}",
            index.categories
        );
    }

    #[test]
    fn test_nested_registry_path_set() {
        let test_dir = test_registry();
        if !test_dir.exists() {
            return;
        }
        let index = load_index(&test_dir).expect("Failed to load test index");

        // Nested skills should have registry_path set
        let maven = index
            .skills
            .get(&("acme".to_string(), "maven-build".to_string()))
            .expect("maven-build should exist");
        assert_eq!(
            maven.registry_path.as_deref(),
            Some("acme/lang/java/maven-build"),
            "nested skill should have registry_path set"
        );

        // Flat skills should have registry_path = None
        let python = index
            .skills
            .get(&("acme".to_string(), "python-dev".to_string()))
            .expect("python-dev should exist");
        assert_eq!(
            python.registry_path, None,
            "flat skill should have no registry_path"
        );
    }

    #[test]
    fn test_nested_skill_discovery_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = tmp.path();

        // Create a flat skill
        let flat_dir = reg.join("myowner").join("flat-skill");
        std::fs::create_dir_all(&flat_dir).unwrap();
        std::fs::write(
            flat_dir.join("skill.toml"),
            "[skill]\nname = \"flat-skill\"\nowner = \"myowner\"\nversion = \"1.0.0\"\ndescription = \"A flat skill\"\n",
        ).unwrap();
        std::fs::write(flat_dir.join("SKILL.md"), "# Flat\n\nFlat skill.\n").unwrap();

        // Create a nested skill
        let nested_dir = reg.join("myowner").join("group").join("nested-skill");
        std::fs::create_dir_all(&nested_dir).unwrap();
        std::fs::write(
            nested_dir.join("skill.toml"),
            "[skill]\nname = \"nested-skill\"\nowner = \"myowner\"\nversion = \"1.0.0\"\ndescription = \"A nested skill\"\n",
        ).unwrap();
        std::fs::write(nested_dir.join("SKILL.md"), "# Nested\n\nNested skill.\n").unwrap();

        let index = load_index(reg).expect("Failed to load index");

        assert_eq!(index.skills.len(), 2);
        assert!(
            index
                .skills
                .contains_key(&("myowner".to_string(), "flat-skill".to_string()))
        );
        assert!(
            index
                .skills
                .contains_key(&("myowner".to_string(), "nested-skill".to_string()))
        );

        // Flat has no registry_path
        let flat = &index.skills[&("myowner".to_string(), "flat-skill".to_string())];
        assert_eq!(flat.registry_path, None);

        // Nested has registry_path
        let nested = &index.skills[&("myowner".to_string(), "nested-skill".to_string())];
        assert_eq!(
            nested.registry_path.as_deref(),
            Some("myowner/group/nested-skill")
        );
    }

    #[test]
    fn test_max_nesting_depth_respected() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = tmp.path();

        // Create a skill at exactly MAX_NESTING_DEPTH levels below owner
        let mut deep_path = reg.join("owner");
        for i in 0..MAX_NESTING_DEPTH {
            deep_path = deep_path.join(format!("level{i}"));
        }
        deep_path = deep_path.join("deep-skill");
        std::fs::create_dir_all(&deep_path).unwrap();
        std::fs::write(
            deep_path.join("skill.toml"),
            "[skill]\nname = \"deep-skill\"\nowner = \"owner\"\nversion = \"1.0.0\"\ndescription = \"Deep skill\"\n",
        ).unwrap();
        std::fs::write(deep_path.join("SKILL.md"), "# Deep\n").unwrap();

        // Create a skill one level too deep (should NOT be found)
        let mut too_deep = reg.join("owner");
        for i in 0..=MAX_NESTING_DEPTH {
            too_deep = too_deep.join(format!("d{i}"));
        }
        too_deep = too_deep.join("too-deep");
        std::fs::create_dir_all(&too_deep).unwrap();
        std::fs::write(
            too_deep.join("skill.toml"),
            "[skill]\nname = \"too-deep\"\nowner = \"owner\"\nversion = \"1.0.0\"\ndescription = \"Too deep\"\n",
        ).unwrap();
        std::fs::write(too_deep.join("SKILL.md"), "# Too Deep\n").unwrap();

        let index = load_index(reg).expect("Failed to load index");

        assert!(
            index
                .skills
                .contains_key(&("owner".to_string(), "deep-skill".to_string())),
            "skill at max depth should be found"
        );
        assert!(
            !index
                .skills
                .contains_key(&("owner".to_string(), "too-deep".to_string())),
            "skill beyond max depth should not be found"
        );
    }

    #[test]
    fn test_nested_collision_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = tmp.path();

        // Two paths under the same owner, both producing name = "collider"
        let path_a = reg.join("owner").join("a").join("collider");
        std::fs::create_dir_all(&path_a).unwrap();
        std::fs::write(
            path_a.join("skill.toml"),
            "[skill]\nname = \"collider\"\nowner = \"owner\"\nversion = \"1.0.0\"\ndescription = \"First collider\"\n",
        ).unwrap();
        std::fs::write(path_a.join("SKILL.md"), "# First\n\nFirst collider.\n").unwrap();

        let path_b = reg.join("owner").join("b").join("collider");
        std::fs::create_dir_all(&path_b).unwrap();
        std::fs::write(
            path_b.join("skill.toml"),
            "[skill]\nname = \"collider\"\nowner = \"owner\"\nversion = \"2.0.0\"\ndescription = \"Second collider\"\n",
        ).unwrap();
        std::fs::write(path_b.join("SKILL.md"), "# Second\n\nSecond collider.\n").unwrap();

        let index = load_index(reg).expect("Failed to load index");

        // Only one should be present (first-wins based on sorted directory order)
        let entry = index
            .skills
            .get(&("owner".to_string(), "collider".to_string()))
            .expect("collider should exist");

        // "a" sorts before "b", so first collider should win
        let latest = entry.latest().expect("should have latest");
        assert_eq!(
            latest.metadata.skill.description, "First collider",
            "first collision path (a/) should win"
        );
    }

    // ── npm-style repo compatibility tests ───────────────────────────

    fn test_npm_registry() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("test-npm-registry")
    }

    #[test]
    fn test_load_extra_files_rules_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(skill_dir.join("rules")).unwrap();
        std::fs::write(
            skill_dir.join("rules/cache-patterns.md"),
            "# Cache Patterns\n",
        )
        .unwrap();
        std::fs::create_dir_all(skill_dir.join("templates")).unwrap();
        std::fs::write(skill_dir.join("templates/config.toml"), "[default]\n").unwrap();

        let files = load_extra_files(&skill_dir).expect("should load extra files");
        assert!(
            files.contains_key("rules/cache-patterns.md"),
            "rules/ files should be loaded: {files:?}"
        );
        assert!(
            files.contains_key("templates/config.toml"),
            "templates/ files should be loaded: {files:?}"
        );
    }

    #[test]
    fn test_load_index_npm_style() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create npm-style layout: skillet.toml + skills/<name>/SKILL.md
        std::fs::write(
            root.join("skillet.toml"),
            r#"
[project]
name = "test-npm"

[[project.authors]]
github = "testorg"

[skills]
path = "skills"
"#,
        )
        .unwrap();

        let skill_a = root.join("skills/alpha");
        std::fs::create_dir_all(&skill_a).unwrap();
        std::fs::write(skill_a.join("SKILL.md"), "# Alpha\n\nAlpha skill.\n").unwrap();

        let skill_b = root.join("skills/beta");
        std::fs::create_dir_all(&skill_b).unwrap();
        std::fs::write(
            skill_b.join("SKILL.md"),
            "---\nversion: 2.0.0\ndescription: Beta from frontmatter\n---\n\n# Beta\n",
        )
        .unwrap();

        let index = load_index(root).expect("should load npm-style repo");
        assert_eq!(index.skills.len(), 2, "should find both skills");
        assert!(
            index
                .skills
                .contains_key(&("testorg".to_string(), "alpha".to_string())),
            "alpha skill should be keyed by manifest author"
        );
        assert!(
            index
                .skills
                .contains_key(&("testorg".to_string(), "beta".to_string())),
            "beta skill should be keyed by manifest author"
        );

        // Verify frontmatter is used for beta
        let beta = &index.skills[&("testorg".to_string(), "beta".to_string())];
        let latest = beta.latest().expect("should have latest");
        assert_eq!(latest.version, "2.0.0");
        assert_eq!(latest.metadata.skill.description, "Beta from frontmatter");
    }

    #[test]
    fn test_load_index_npm_fixture() {
        let npm_dir = test_npm_registry();
        if !npm_dir.exists() {
            return;
        }
        let index = load_index(&npm_dir).expect("should load test-npm-registry");

        assert_eq!(index.skills.len(), 3, "should find 3 skills");

        // Check redis-caching with frontmatter
        let caching = index
            .skills
            .get(&("redis".to_string(), "redis-caching".to_string()))
            .expect("redis-caching should exist");
        let latest = caching.latest().expect("should have latest");
        assert_eq!(latest.version, "2.1.0");
        assert_eq!(
            latest.metadata.skill.description,
            "Best practices for Redis caching patterns"
        );
        assert!(
            latest.files.contains_key("rules/cache-patterns.md"),
            "rules/ files should be loaded"
        );
        assert!(
            latest.files.contains_key("rules/ttl-guidelines.md"),
            "rules/ files should be loaded"
        );

        // Check vector-search
        let vsearch = index
            .skills
            .get(&("redis".to_string(), "vector-search".to_string()))
            .expect("vector-search should exist");
        let latest = vsearch.latest().expect("should have latest");
        assert_eq!(latest.version, "1.5.0");
        assert!(
            latest.files.contains_key("references/embedding-guide.md"),
            "references/ files should be loaded"
        );

        // Check session-management (no frontmatter)
        let session = index
            .skills
            .get(&("redis".to_string(), "session-management".to_string()))
            .expect("session-management should exist");
        let latest = session.latest().expect("should have latest");
        assert_eq!(
            latest.version, "0.1.0",
            "no frontmatter means default version"
        );

        // Category from manifest should be indexed
        assert!(
            index.categories.contains_key("database"),
            "database category from manifest should be indexed: {:?}",
            index.categories
        );
    }

    #[test]
    fn test_load_index_flat_repo_fallback() {
        // Flat layout without git: owner inferred from directory name
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("anthropics");
        std::fs::create_dir_all(&root).unwrap();

        // skill-a/SKILL.md
        let skill_a = root.join("skill-a");
        std::fs::create_dir_all(&skill_a).unwrap();
        std::fs::write(skill_a.join("SKILL.md"), "# Skill A\n\nA useful skill.\n").unwrap();

        // skill-b/SKILL.md
        let skill_b = root.join("skill-b");
        std::fs::create_dir_all(&skill_b).unwrap();
        std::fs::write(skill_b.join("SKILL.md"), "# Skill B\n\nAnother skill.\n").unwrap();

        let index = load_index(&root).expect("should load flat repo");
        assert_eq!(index.skills.len(), 2, "should find both flat skills");

        // Owner comes from directory name since there's no git remote
        assert!(
            index
                .skills
                .contains_key(&("anthropics".to_string(), "skill-a".to_string())),
            "skill-a should use dir name as owner"
        );
        assert!(
            index
                .skills
                .contains_key(&("anthropics".to_string(), "skill-b".to_string())),
            "skill-b should use dir name as owner"
        );

        // Verify content was loaded
        let entry = &index.skills[&("anthropics".to_string(), "skill-a".to_string())];
        let latest = entry.latest().expect("should have latest");
        assert!(latest.skill_md.contains("Skill A"));
    }

    #[test]
    fn test_load_index_flat_repo_with_git_remote() {
        // Flat layout inside a git repo: owner inferred from remote URL
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Initialize a git repo with a remote
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .expect("git init");
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/vercel-labs/agent-skills.git",
            ])
            .current_dir(root)
            .output()
            .expect("git remote add");

        // Create flat skills
        let skill = root.join("react-patterns");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "# React Patterns\n\nReact best practices.\n",
        )
        .unwrap();

        let index = load_index(root).expect("should load flat repo with git remote");
        assert_eq!(index.skills.len(), 1);
        assert!(
            index
                .skills
                .contains_key(&("vercel-labs".to_string(), "react-patterns".to_string())),
            "owner should come from git remote"
        );
    }

    #[test]
    fn test_load_index_flat_repo_in_subdir() {
        // Skills in a subdirectory (simulates subdir = "skills" in repos.toml)
        // with git remote accessible from the parent
        let tmp = tempfile::tempdir().unwrap();
        let git_root = tmp.path();

        // Initialize git repo at the top level
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(git_root)
            .output()
            .expect("git init");
        std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/firebase/agent-skills.git",
            ])
            .current_dir(git_root)
            .output()
            .expect("git remote add");

        // Skills live in a subdirectory
        let skills_dir = git_root.join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let skill = skills_dir.join("firestore-queries");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "# Firestore Queries\n\nQuery Firestore.\n",
        )
        .unwrap();

        // load_index is called with the subdirectory (as registry.rs does)
        let index = load_index(&skills_dir).expect("should load flat repo from subdir");
        assert_eq!(index.skills.len(), 1);
        assert!(
            index
                .skills
                .contains_key(&("firebase".to_string(), "firestore-queries".to_string())),
            "owner should come from git remote of parent repo"
        );
    }
}
