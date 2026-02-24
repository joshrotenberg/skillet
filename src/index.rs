//! Index loading from a local registry directory
//!
//! Walks the registry directory tree looking for `owner/skill-name/skill.toml`
//! files. Each skill directory is expected to contain:
//! - `skill.toml` (required)
//! - `SKILL.md` (required)

use std::path::Path;

use anyhow::{Context, bail};

use crate::state::{SkillEntry, SkillIndex, SkillMetadata, SkillVersion};

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

/// Load a single skill from its directory
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

    let version = SkillVersion {
        version: metadata.skill.version.clone(),
        metadata,
        skill_md,
        skill_toml_raw,
        yanked: false,
    };

    Ok(SkillEntry {
        owner: owner.to_string(),
        name: name.to_string(),
        versions: vec![version],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_load_index_from_test_registry() {
        let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test-registry");
        if !test_dir.exists() {
            return; // Skip if test registry not created yet
        }
        let index = load_index(&test_dir).expect("Failed to load test index");
        assert!(
            !index.skills.is_empty(),
            "Index should have at least one skill"
        );
    }
}
