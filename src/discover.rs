//! Auto-discover skills installed in local agent skill directories.
//!
//! Scans well-known agent skill directories (global and project-local) for
//! `SKILL.md` files and builds synthetic `SkillEntry` records. These are
//! merged into the main index after registry skills, so registry skills
//! always win on name collision.

use std::path::Path;

use crate::config::ALL_TARGETS;
use crate::index::load_extra_files;
use crate::state::{SkillEntry, SkillIndex, SkillInfo, SkillMetadata, SkillSource, SkillVersion};

/// Scan all well-known agent skill directories and return a synthetic index.
///
/// Scans both global directories (e.g. `~/.claude/skills/`) and project-local
/// directories (relative to the current working directory). Skills are keyed
/// as `("local", skill_name)` -- first platform wins on dedup.
pub fn discover_local_skills() -> SkillIndex {
    let mut index = SkillIndex::default();

    for &target in ALL_TARGETS {
        let platform = target.as_str().to_string();

        // Global directory (e.g. ~/.claude/skills/)
        let global_parent = target.global_dir("");
        // global_dir returns e.g. ~/.claude/skills/name/ -- we want the parent
        // so strip the trailing empty component
        let global_dir = global_parent.parent().unwrap_or(&global_parent);
        scan_skills_dir(global_dir, &platform, &mut index);

        // Project-local directory (e.g. .claude/skills/)
        let project_parent = target.project_dir("");
        let project_dir = project_parent.parent().unwrap_or(&project_parent);
        scan_skills_dir(project_dir, &platform, &mut index);
    }

    index
}

/// Scan a single skills parent directory for skill subdirectories.
fn scan_skills_dir(skills_dir: &Path, platform: &str, index: &mut SkillIndex) {
    let entries = match std::fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(_) => return, // Directory doesn't exist or isn't readable
    };

    let mut subdirs: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    subdirs.sort_by_key(|e| e.file_name());

    for entry in subdirs {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden directories
        if dir_name.starts_with('.') {
            continue;
        }

        // Must contain SKILL.md
        let skill_md_path = path.join("SKILL.md");
        if !skill_md_path.is_file() {
            continue;
        }

        // Already indexed under this name? First platform wins.
        let key = ("local".to_string(), dir_name.clone());
        if index.skills.contains_key(&key) {
            continue;
        }

        match build_local_entry(&dir_name, &path, platform) {
            Ok(entry) => {
                tracing::debug!(
                    skill = %dir_name,
                    platform = %platform,
                    path = %path.display(),
                    "Discovered local skill"
                );
                index.skills.insert(key, entry);
            }
            Err(e) => {
                tracing::debug!(
                    skill = %dir_name,
                    path = %path.display(),
                    error = %e,
                    "Skipping unreadable local skill"
                );
            }
        }
    }
}

/// Build a synthetic `SkillEntry` from a local skill directory.
fn build_local_entry(name: &str, path: &Path, platform: &str) -> anyhow::Result<SkillEntry> {
    let skill_md = std::fs::read_to_string(path.join("SKILL.md"))?;
    let description = extract_description(&skill_md);

    // Read skill.toml if present (for richer metadata), but don't require it
    let skill_toml_raw = std::fs::read_to_string(path.join("skill.toml")).unwrap_or_default();

    // Load extra files (scripts/, references/, assets/)
    let files = load_extra_files(path).unwrap_or_default();

    let metadata = SkillMetadata {
        skill: SkillInfo {
            name: name.to_string(),
            owner: "local".to_string(),
            version: "0.0.0".to_string(),
            description,
            trigger: None,
            license: None,
            author: None,
            classification: None,
            compatibility: None,
        },
    };

    Ok(SkillEntry {
        owner: "local".to_string(),
        name: name.to_string(),
        registry_path: None,
        source: SkillSource::Local {
            platform: platform.to_string(),
            path: path.to_path_buf(),
        },
        versions: vec![SkillVersion {
            version: "0.0.0".to_string(),
            metadata,
            skill_md,
            skill_toml_raw,
            yanked: false,
            files,
            published: None,
            has_content: true,
            content_hash: None,
            integrity_ok: None,
        }],
    })
}

/// Extract a description from SKILL.md content.
///
/// Takes the first non-empty line that doesn't start with `#`.
/// Truncates to 200 characters. Falls back to `"Local skill"`.
fn extract_description(skill_md: &str) -> String {
    for line in skill_md.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return truncate(trimmed, 200).to_string();
    }
    "Local skill".to_string()
}

/// Truncate a string to at most `max_chars` characters.
fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_description_skips_headings() {
        let md = "# Heading\n\nActual description here.";
        assert_eq!(extract_description(md), "Actual description here.");
    }

    #[test]
    fn test_extract_description_truncation() {
        let long_line = format!("# Title\n\n{}", "a".repeat(300));
        let desc = extract_description(&long_line);
        assert_eq!(desc.len(), 200);
        assert!(desc.chars().all(|c| c == 'a'));
    }

    #[test]
    fn test_extract_description_fallback() {
        assert_eq!(
            extract_description("# Only Headings\n## Another"),
            "Local skill"
        );
        assert_eq!(extract_description(""), "Local skill");
        assert_eq!(extract_description("  \n  \n"), "Local skill");
    }

    #[test]
    fn test_discover_from_temp_dir() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a skill directory with SKILL.md
        let skill_dir = tmp.path().join("my-test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "# My Test Skill\n\nA skill for testing discovery.\n",
        )
        .unwrap();

        // Create another skill
        let skill2_dir = tmp.path().join("another-skill");
        std::fs::create_dir_all(&skill2_dir).unwrap();
        std::fs::write(skill2_dir.join("SKILL.md"), "# Another\n\nSecond skill.").unwrap();

        let mut index = SkillIndex::default();
        scan_skills_dir(tmp.path(), "test-platform", &mut index);

        assert_eq!(index.skills.len(), 2);
        assert!(
            index
                .skills
                .contains_key(&("local".to_string(), "my-test-skill".to_string()))
        );
        assert!(
            index
                .skills
                .contains_key(&("local".to_string(), "another-skill".to_string()))
        );

        let entry = &index.skills[&("local".to_string(), "my-test-skill".to_string())];
        assert_eq!(entry.owner, "local");
        assert_eq!(entry.name, "my-test-skill");
        let v = entry.latest().unwrap();
        assert_eq!(
            v.metadata.skill.description,
            "A skill for testing discovery."
        );
    }

    #[test]
    fn test_discover_skips_hidden_dirs() {
        let tmp = tempfile::tempdir().unwrap();

        // Hidden dir should be skipped
        let hidden = tmp.path().join(".hidden-skill");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("SKILL.md"), "# Hidden").unwrap();

        // Visible dir should be included
        let visible = tmp.path().join("visible-skill");
        std::fs::create_dir_all(&visible).unwrap();
        std::fs::write(visible.join("SKILL.md"), "# Visible\n\nA visible skill.").unwrap();

        let mut index = SkillIndex::default();
        scan_skills_dir(tmp.path(), "test", &mut index);

        assert_eq!(index.skills.len(), 1);
        assert!(
            index
                .skills
                .contains_key(&("local".to_string(), "visible-skill".to_string()))
        );
    }

    #[test]
    fn test_discover_skips_dirs_without_skill_md() {
        let tmp = tempfile::tempdir().unwrap();

        // Dir without SKILL.md
        let no_skill = tmp.path().join("no-skill-md");
        std::fs::create_dir_all(&no_skill).unwrap();
        std::fs::write(no_skill.join("README.md"), "Not a skill").unwrap();

        let mut index = SkillIndex::default();
        scan_skills_dir(tmp.path(), "test", &mut index);

        assert!(index.skills.is_empty());
    }

    #[test]
    fn test_discover_dedup_across_platforms() {
        let tmp1 = tempfile::tempdir().unwrap();
        let tmp2 = tempfile::tempdir().unwrap();

        // Same skill name in two different platform dirs
        let skill1 = tmp1.path().join("shared-skill");
        std::fs::create_dir_all(&skill1).unwrap();
        std::fs::write(skill1.join("SKILL.md"), "# From platform1\n\nFirst.").unwrap();

        let skill2 = tmp2.path().join("shared-skill");
        std::fs::create_dir_all(&skill2).unwrap();
        std::fs::write(skill2.join("SKILL.md"), "# From platform2\n\nSecond.").unwrap();

        let mut index = SkillIndex::default();
        scan_skills_dir(tmp1.path(), "platform1", &mut index);
        scan_skills_dir(tmp2.path(), "platform2", &mut index);

        // First platform wins
        assert_eq!(index.skills.len(), 1);
        let entry = &index.skills[&("local".to_string(), "shared-skill".to_string())];
        assert_eq!(
            entry.source,
            SkillSource::Local {
                platform: "platform1".to_string(),
                path: skill1,
            }
        );
    }

    #[test]
    fn test_local_source_set() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# Test\n\nDescription.").unwrap();

        let mut index = SkillIndex::default();
        scan_skills_dir(tmp.path(), "claude", &mut index);

        let entry = &index.skills[&("local".to_string(), "test-skill".to_string())];
        match &entry.source {
            SkillSource::Local { platform, path } => {
                assert_eq!(platform, "claude");
                assert_eq!(path, &skill_dir);
            }
            other => panic!("Expected Local source, got {other:?}"),
        }
    }

    #[test]
    fn test_discover_loads_extra_files() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("files-skill");
        std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# Files\n\nHas scripts.").unwrap();
        std::fs::write(skill_dir.join("scripts/run.sh"), "#!/bin/bash\necho ok").unwrap();

        let mut index = SkillIndex::default();
        scan_skills_dir(tmp.path(), "test", &mut index);

        let entry = &index.skills[&("local".to_string(), "files-skill".to_string())];
        let v = entry.latest().unwrap();
        assert!(v.files.contains_key("scripts/run.sh"));
    }
}
