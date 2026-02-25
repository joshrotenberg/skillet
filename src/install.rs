//! Core skill installation logic.
//!
//! Writes skill files to agent target directories and records entries
//! in the installation manifest.

use std::path::{Path, PathBuf};

use crate::config::{self, InstallTarget};
use crate::index::EXTRA_DIRS;
use crate::integrity;
use crate::manifest::{InstalledManifest, InstalledSkill};
use crate::state::SkillVersion;

/// Errors that can occur during skill installation.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to resolve current directory: {0}")]
    CurrentDir(std::io::Error),
}

/// Options controlling how a skill is installed.
pub struct InstallOptions {
    pub targets: Vec<InstallTarget>,
    pub global: bool,
    /// Registry identifier for the manifest entry.
    /// Git URL for remotes, `local:<abs_path>` for local registries.
    pub registry: String,
}

/// Result of installing a skill to a single target.
pub struct InstallResult {
    pub target: InstallTarget,
    pub path: PathBuf,
    pub files_written: Vec<String>,
}

/// Install a skill to all specified targets, updating the manifest.
///
/// Returns one `InstallResult` per target. Does NOT call `manifest::save()` --
/// the caller should save once after all installs complete.
pub fn install_skill(
    owner: &str,
    name: &str,
    version: &SkillVersion,
    options: &InstallOptions,
    manifest: &mut InstalledManifest,
) -> Result<Vec<InstallResult>, InstallError> {
    let cwd = std::env::current_dir().map_err(InstallError::CurrentDir)?;
    let checksum = integrity::sha256_hex(&version.skill_md);
    let now = config::now_iso8601();
    let mut results = Vec::new();

    for &target in &options.targets {
        let relative_dir = if options.global {
            target.global_dir(name)
        } else {
            target.project_dir(name)
        };

        // Resolve to absolute path for the manifest
        let abs_dir = if relative_dir.is_absolute() {
            relative_dir
        } else {
            cwd.join(relative_dir)
        };

        let files_written = write_skill_to_dir(version, &abs_dir)?;

        manifest.upsert(InstalledSkill {
            owner: owner.to_string(),
            name: name.to_string(),
            version: version.version.clone(),
            registry: options.registry.clone(),
            checksum: checksum.clone(),
            installed_to: abs_dir.clone(),
            installed_at: now.clone(),
        });

        results.push(InstallResult {
            target,
            path: abs_dir,
            files_written,
        });
    }

    Ok(results)
}

/// Write skill files to a directory, creating it if needed.
///
/// Writes SKILL.md and any extra files (scripts/, references/, assets/).
/// Does NOT write skill.toml, MANIFEST.sha256, or versions.toml.
/// Returns the list of relative paths written.
fn write_skill_to_dir(version: &SkillVersion, dir: &Path) -> Result<Vec<String>, InstallError> {
    std::fs::create_dir_all(dir).map_err(|e| InstallError::CreateDir {
        path: dir.to_path_buf(),
        source: e,
    })?;

    let mut written = Vec::new();

    // Write SKILL.md
    let skill_md_path = dir.join("SKILL.md");
    std::fs::write(&skill_md_path, &version.skill_md).map_err(|e| InstallError::WriteFile {
        path: skill_md_path,
        source: e,
    })?;
    written.push("SKILL.md".to_string());

    // Write extra files (scripts/, references/, assets/)
    for (rel_path, file) in &version.files {
        let target_path = dir.join(rel_path);

        // Create subdirectory if needed
        if let Some(parent) = target_path.parent() {
            // Only create subdirs that are in the allowed set
            let subdir = rel_path.split('/').next().unwrap_or("");
            if !EXTRA_DIRS.contains(&subdir) {
                continue;
            }
            std::fs::create_dir_all(parent).map_err(|e| InstallError::CreateDir {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        std::fs::write(&target_path, &file.content).map_err(|e| InstallError::WriteFile {
            path: target_path,
            source: e,
        })?;
        written.push(rel_path.clone());
    }

    written.sort();
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::state::{SkillFile, SkillInfo, SkillMetadata};

    fn sample_version() -> SkillVersion {
        SkillVersion {
            version: "1.0.0".to_string(),
            metadata: SkillMetadata {
                skill: SkillInfo {
                    name: "test-skill".to_string(),
                    owner: "testowner".to_string(),
                    version: "1.0.0".to_string(),
                    description: "A test skill".to_string(),
                    trigger: None,
                    license: None,
                    author: None,
                    classification: None,
                    compatibility: None,
                },
            },
            skill_md: "# Test Skill\n\nDo the thing.".to_string(),
            skill_toml_raw: "[skill]\nname = \"test-skill\"".to_string(),
            yanked: false,
            files: HashMap::new(),
            published: Some("2026-01-01T00:00:00Z".to_string()),
            has_content: true,
            content_hash: None,
            integrity_ok: None,
        }
    }

    fn sample_version_with_files() -> SkillVersion {
        let mut version = sample_version();
        version.files.insert(
            "scripts/lint.sh".to_string(),
            SkillFile {
                content: "#!/bin/bash\necho lint".to_string(),
                mime_type: "text/x-shellscript".to_string(),
            },
        );
        version.files.insert(
            "references/guide.md".to_string(),
            SkillFile {
                content: "# Guide\n\nSome reference.".to_string(),
                mime_type: "text/markdown".to_string(),
            },
        );
        version
    }

    #[test]
    fn test_install_single_target() {
        let tmp = tempfile::tempdir().unwrap();
        let version = sample_version();
        let mut manifest = InstalledManifest::default();

        let target_dir = tmp.path().join("project");
        std::fs::create_dir_all(&target_dir).unwrap();

        // Use the tempdir as working directory by making absolute paths
        let skill_dir = target_dir.join(".agents/skills/test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let files = write_skill_to_dir(&version, &skill_dir).unwrap();
        assert!(files.contains(&"SKILL.md".to_string()));

        let content = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert_eq!(content, version.skill_md);

        // Also verify manifest upsert works
        manifest.upsert(InstalledSkill {
            owner: "testowner".to_string(),
            name: "test-skill".to_string(),
            version: "1.0.0".to_string(),
            registry: "local:/tmp".to_string(),
            checksum: integrity::sha256_hex(&version.skill_md),
            installed_to: skill_dir,
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        });
        assert_eq!(manifest.skills.len(), 1);
    }

    #[test]
    fn test_install_with_extra_files() {
        let tmp = tempfile::tempdir().unwrap();
        let version = sample_version_with_files();
        let skill_dir = tmp.path().join("skill");

        let files = write_skill_to_dir(&version, &skill_dir).unwrap();
        assert!(files.contains(&"SKILL.md".to_string()));
        assert!(files.contains(&"scripts/lint.sh".to_string()));
        assert!(files.contains(&"references/guide.md".to_string()));

        // Verify files exist
        assert!(skill_dir.join("SKILL.md").is_file());
        assert!(skill_dir.join("scripts/lint.sh").is_file());
        assert!(skill_dir.join("references/guide.md").is_file());

        // Verify content
        let lint = std::fs::read_to_string(skill_dir.join("scripts/lint.sh")).unwrap();
        assert_eq!(lint, "#!/bin/bash\necho lint");
    }

    #[test]
    fn test_install_to_multiple_targets() {
        let tmp = tempfile::tempdir().unwrap();
        let version = sample_version();
        let mut manifest = InstalledManifest::default();

        // Install to two different paths (simulating two targets)
        let dir1 = tmp.path().join("agents/test-skill");
        let dir2 = tmp.path().join("claude/test-skill");

        write_skill_to_dir(&version, &dir1).unwrap();
        write_skill_to_dir(&version, &dir2).unwrap();

        manifest.upsert(InstalledSkill {
            owner: "testowner".to_string(),
            name: "test-skill".to_string(),
            version: "1.0.0".to_string(),
            registry: "local:/tmp".to_string(),
            checksum: integrity::sha256_hex(&version.skill_md),
            installed_to: dir1.clone(),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        });
        manifest.upsert(InstalledSkill {
            owner: "testowner".to_string(),
            name: "test-skill".to_string(),
            version: "1.0.0".to_string(),
            registry: "local:/tmp".to_string(),
            checksum: integrity::sha256_hex(&version.skill_md),
            installed_to: dir2.clone(),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        });

        assert_eq!(manifest.skills.len(), 2);
        assert!(dir1.join("SKILL.md").is_file());
        assert!(dir2.join("SKILL.md").is_file());
    }

    #[test]
    fn test_reinstall_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let mut version = sample_version();
        let skill_dir = tmp.path().join("skill");

        // First install
        write_skill_to_dir(&version, &skill_dir).unwrap();
        let content1 = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();

        // Update and reinstall
        version.skill_md = "# Updated\n\nNew content.".to_string();
        version.version = "2.0.0".to_string();
        write_skill_to_dir(&version, &skill_dir).unwrap();
        let content2 = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();

        assert_ne!(content1, content2);
        assert_eq!(content2, "# Updated\n\nNew content.");
    }

    #[test]
    fn test_manifest_entries_correct() {
        let tmp = tempfile::tempdir().unwrap();
        let version = sample_version();
        let mut manifest = InstalledManifest::default();
        let skill_dir = tmp.path().join("skill");

        write_skill_to_dir(&version, &skill_dir).unwrap();

        let checksum = integrity::sha256_hex(&version.skill_md);
        manifest.upsert(InstalledSkill {
            owner: "testowner".to_string(),
            name: "test-skill".to_string(),
            version: "1.0.0".to_string(),
            registry: "https://github.com/owner/repo.git".to_string(),
            checksum: checksum.clone(),
            installed_to: skill_dir,
            installed_at: "2026-01-01T00:00:00Z".to_string(),
        });

        let entry = &manifest.skills[0];
        assert_eq!(entry.owner, "testowner");
        assert_eq!(entry.name, "test-skill");
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.checksum, checksum);
    }
}
