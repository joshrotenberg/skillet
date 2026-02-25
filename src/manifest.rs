//! Installation manifest: tracks which skills are installed and where.
//!
//! The manifest lives at `~/.config/skillet/installed.toml` and records
//! every (skill, target) installation with version, checksum, and path.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::integrity;

/// Errors that can occur when working with the installation manifest.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("failed to read manifest at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse manifest at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to write manifest to {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to serialize manifest: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// The installation manifest file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledManifest {
    #[serde(default)]
    pub skills: Vec<InstalledSkill>,
}

/// A single installed skill entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub owner: String,
    pub name: String,
    pub version: String,
    pub registry: String,
    pub checksum: String,
    pub installed_to: PathBuf,
    pub installed_at: String,
}

/// Result of checking an installed skill's integrity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityStatus {
    /// SKILL.md exists and checksum matches.
    Ok,
    /// SKILL.md exists but checksum differs.
    Modified,
    /// SKILL.md is missing from the installed path.
    Missing,
}

/// Default manifest file path: `~/.config/skillet/installed.toml`.
pub fn manifest_path() -> PathBuf {
    config::config_dir().join("installed.toml")
}

/// Load the installation manifest, returning empty if the file is absent.
pub fn load() -> Result<InstalledManifest, ManifestError> {
    load_from(&manifest_path())
}

/// Load the installation manifest from a specific path.
pub fn load_from(path: &Path) -> Result<InstalledManifest, ManifestError> {
    if !path.is_file() {
        return Ok(InstalledManifest::default());
    }
    let raw = std::fs::read_to_string(path).map_err(|e| ManifestError::Read {
        path: path.to_path_buf(),
        source: e,
    })?;
    let manifest: InstalledManifest = toml::from_str(&raw).map_err(|e| ManifestError::Parse {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(manifest)
}

/// Save the installation manifest to the default path.
pub fn save(manifest: &InstalledManifest) -> Result<(), ManifestError> {
    save_to(manifest, &manifest_path())
}

/// Save the installation manifest to a specific path.
pub fn save_to(manifest: &InstalledManifest, path: &Path) -> Result<(), ManifestError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ManifestError::CreateDir {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let content = toml::to_string_pretty(manifest)?;
    std::fs::write(path, content).map_err(|e| ManifestError::Write {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

impl InstalledManifest {
    /// Add or replace an entry by `installed_to` path.
    pub fn upsert(&mut self, skill: InstalledSkill) {
        if let Some(existing) = self
            .skills
            .iter_mut()
            .find(|s| s.installed_to == skill.installed_to)
        {
            *existing = skill;
        } else {
            self.skills.push(skill);
        }
    }

    /// Remove an entry by owner, name, and installed path. Returns true if found.
    pub fn remove(&mut self, owner: &str, name: &str, path: &Path) -> bool {
        let before = self.skills.len();
        self.skills
            .retain(|s| !(s.owner == owner && s.name == name && s.installed_to == path));
        self.skills.len() < before
    }

    /// Find all installations of a skill by owner and name.
    pub fn find_by_skill(&self, owner: &str, name: &str) -> Vec<&InstalledSkill> {
        self.skills
            .iter()
            .filter(|s| s.owner == owner && s.name == name)
            .collect()
    }

    /// Find an installation by its installed path.
    pub fn find_by_path(&self, path: &Path) -> Option<&InstalledSkill> {
        self.skills.iter().find(|s| s.installed_to == path)
    }

    /// Check the integrity of an installed skill by reading SKILL.md and
    /// comparing its checksum against the stored value.
    pub fn check_integrity(entry: &InstalledSkill) -> IntegrityStatus {
        let skill_md_path = entry.installed_to.join("SKILL.md");
        let content = match std::fs::read_to_string(&skill_md_path) {
            Ok(c) => c,
            Err(_) => return IntegrityStatus::Missing,
        };
        let computed = integrity::sha256_hex(&content);
        if computed == entry.checksum {
            IntegrityStatus::Ok
        } else {
            IntegrityStatus::Modified
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(path: &Path) -> InstalledSkill {
        InstalledSkill {
            owner: "testowner".to_string(),
            name: "testskill".to_string(),
            version: "1.0.0".to_string(),
            registry: "https://github.com/owner/repo.git".to_string(),
            checksum: "sha256:abc123".to_string(),
            installed_to: path.to_path_buf(),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_load_empty_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let manifest = load_from(&path).unwrap();
        assert!(manifest.skills.is_empty());
    }

    #[test]
    fn test_save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("installed.toml");
        let install_path = tmp.path().join("skills/test");

        let mut manifest = InstalledManifest::default();
        manifest.upsert(sample_entry(&install_path));

        save_to(&manifest, &path).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.skills.len(), 1);
        assert_eq!(loaded.skills[0].owner, "testowner");
        assert_eq!(loaded.skills[0].name, "testskill");
    }

    #[test]
    fn test_upsert_new_entry() {
        let mut manifest = InstalledManifest::default();
        let path = PathBuf::from("/tmp/skills/test");
        manifest.upsert(sample_entry(&path));
        assert_eq!(manifest.skills.len(), 1);
    }

    #[test]
    fn test_upsert_replaces_same_path() {
        let mut manifest = InstalledManifest::default();
        let path = PathBuf::from("/tmp/skills/test");

        manifest.upsert(sample_entry(&path));
        assert_eq!(manifest.skills[0].version, "1.0.0");

        let mut updated = sample_entry(&path);
        updated.version = "2.0.0".to_string();
        manifest.upsert(updated);

        assert_eq!(manifest.skills.len(), 1);
        assert_eq!(manifest.skills[0].version, "2.0.0");
    }

    #[test]
    fn test_upsert_keeps_different_paths() {
        let mut manifest = InstalledManifest::default();
        let path1 = PathBuf::from("/tmp/skills/test1");
        let path2 = PathBuf::from("/tmp/skills/test2");

        manifest.upsert(sample_entry(&path1));
        manifest.upsert(sample_entry(&path2));
        assert_eq!(manifest.skills.len(), 2);
    }

    #[test]
    fn test_remove_existing() {
        let mut manifest = InstalledManifest::default();
        let path = PathBuf::from("/tmp/skills/test");
        manifest.upsert(sample_entry(&path));

        let removed = manifest.remove("testowner", "testskill", &path);
        assert!(removed);
        assert!(manifest.skills.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut manifest = InstalledManifest::default();
        let path = PathBuf::from("/tmp/skills/test");
        let removed = manifest.remove("testowner", "testskill", &path);
        assert!(!removed);
    }

    #[test]
    fn test_find_by_skill() {
        let mut manifest = InstalledManifest::default();
        let path1 = PathBuf::from("/tmp/agents/skills/test");
        let path2 = PathBuf::from("/tmp/claude/skills/test");

        manifest.upsert(sample_entry(&path1));
        manifest.upsert(sample_entry(&path2));

        let found = manifest.find_by_skill("testowner", "testskill");
        assert_eq!(found.len(), 2);

        let found = manifest.find_by_skill("nobody", "nothing");
        assert!(found.is_empty());
    }

    #[test]
    fn test_find_by_path() {
        let mut manifest = InstalledManifest::default();
        let path = PathBuf::from("/tmp/skills/test");
        manifest.upsert(sample_entry(&path));

        assert!(manifest.find_by_path(&path).is_some());
        assert!(
            manifest
                .find_by_path(&PathBuf::from("/tmp/other"))
                .is_none()
        );
    }

    #[test]
    fn test_integrity_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = "# My Skill\n\nDo the thing.";
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let entry = InstalledSkill {
            owner: "test".to_string(),
            name: "my-skill".to_string(),
            version: "1.0.0".to_string(),
            registry: "local:/tmp".to_string(),
            checksum: integrity::sha256_hex(content),
            installed_to: skill_dir,
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(
            InstalledManifest::check_integrity(&entry),
            IntegrityStatus::Ok
        );
    }

    #[test]
    fn test_integrity_modified() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        std::fs::write(skill_dir.join("SKILL.md"), "modified content").unwrap();

        let entry = InstalledSkill {
            owner: "test".to_string(),
            name: "my-skill".to_string(),
            version: "1.0.0".to_string(),
            registry: "local:/tmp".to_string(),
            checksum: integrity::sha256_hex("original content"),
            installed_to: skill_dir,
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(
            InstalledManifest::check_integrity(&entry),
            IntegrityStatus::Modified
        );
    }

    #[test]
    fn test_integrity_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        // Don't create the directory or SKILL.md

        let entry = InstalledSkill {
            owner: "test".to_string(),
            name: "my-skill".to_string(),
            version: "1.0.0".to_string(),
            registry: "local:/tmp".to_string(),
            checksum: integrity::sha256_hex("content"),
            installed_to: skill_dir,
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(
            InstalledManifest::check_integrity(&entry),
            IntegrityStatus::Missing
        );
    }

    #[test]
    fn test_malformed_manifest_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("installed.toml");
        std::fs::write(&path, "this is not valid toml {{{").unwrap();
        assert!(load_from(&path).is_err());
    }
}
