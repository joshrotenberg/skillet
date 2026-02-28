//! Project manifest (`skillet.toml`) types and loading.
//!
//! A `skillet.toml` file is a unified manifest that can describe a project,
//! a single inline skill, a multi-skill directory, a registry, or any
//! combination. It enables embedding skills in any repository with zero
//! configuration beyond a SKILL.md file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::Error;

/// Top-level manifest parsed from `skillet.toml`.
///
/// All sections are optional -- the manifest's role is inferred from which
/// sections are present (see [`ManifestRole`]).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkilletToml {
    /// Project metadata (name, description, authors, etc.)
    #[serde(default)]
    pub project: Option<ProjectSection>,

    /// Single inline skill (root-level SKILL.md)
    #[serde(default)]
    pub skill: Option<SkillSection>,

    /// Multiple skills in a subdirectory
    #[serde(default)]
    pub skills: Option<SkillsSection>,

    /// Registry configuration
    #[serde(default)]
    pub registry: Option<RegistrySection>,
}

/// Project metadata section.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectSection {
    /// Project name (defaults to directory name)
    #[serde(default)]
    pub name: Option<String>,

    /// Short project description
    #[serde(default)]
    pub description: Option<String>,

    /// Repository URL (e.g. `https://github.com/owner/repo`)
    #[serde(default)]
    pub repository: Option<String>,

    /// SPDX license identifier
    #[serde(default)]
    pub license: Option<String>,

    /// Default categories for embedded skills
    #[serde(default)]
    pub categories: Vec<String>,

    /// Default tags for embedded skills
    #[serde(default)]
    pub tags: Vec<String>,

    /// Project authors
    #[serde(default)]
    pub authors: Vec<ProjectAuthor>,

    /// Path to an AGENTS.md file to include as context
    #[serde(default)]
    pub agents_md: Option<String>,
}

/// A project author entry.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectAuthor {
    /// Author display name
    #[serde(default)]
    pub name: Option<String>,

    /// Author email
    #[serde(default)]
    pub email: Option<String>,

    /// GitHub username
    #[serde(default)]
    pub github: Option<String>,
}

/// Single inline skill configuration.
///
/// When present, the project root (or the specified path) contains a SKILL.md
/// that is the skill prompt.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillSection {
    /// Skill name (defaults to project name or directory name)
    #[serde(default)]
    pub name: Option<String>,

    /// Skill version (defaults to "0.1.0")
    #[serde(default)]
    pub version: Option<String>,

    /// Skill description (defaults to project description or SKILL.md extraction)
    #[serde(default)]
    pub description: Option<String>,

    /// Categories (inherits from project if unset)
    #[serde(default)]
    pub categories: Option<Vec<String>>,

    /// Tags (inherits from project if unset)
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Path to SKILL.md relative to project root (defaults to ".")
    #[serde(default)]
    pub path: Option<String>,
}

/// Multiple skills directory configuration.
///
/// Points to a directory (default `.skillet/`) containing skill subdirectories,
/// each with at least a SKILL.md file.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillsSection {
    /// Directory containing skill subdirectories (default: ".skillet/")
    #[serde(default)]
    pub path: Option<String>,

    /// Explicit list of skill directory names to include.
    /// If empty, all subdirectories with SKILL.md are included.
    #[serde(default)]
    pub members: Vec<String>,
}

impl SkillsSection {
    /// Resolved path relative to project root, defaulting to `.skillet/`.
    pub fn resolved_path(&self) -> &str {
        self.path.as_deref().unwrap_or(".skillet")
    }
}

/// Registry section, matching the existing `RegistryConfig` structure.
///
/// The field names mirror `state::RegistryInfo` so that existing registry
/// loading works transparently.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistrySection {
    /// Registry name
    pub name: String,

    /// Schema version (always 1 for now)
    #[serde(default = "default_registry_version")]
    pub version: u32,

    /// Registry description
    #[serde(default)]
    pub description: Option<String>,

    /// Maintainer information
    #[serde(default)]
    pub maintainer: Option<crate::state::RegistryMaintainer>,

    /// URL endpoints for non-git-backed registries
    #[serde(default)]
    pub urls: Option<crate::state::RegistryUrls>,

    /// Auth configuration
    #[serde(default)]
    pub auth: Option<crate::state::RegistryAuth>,

    /// Suggested companion registries
    #[serde(default)]
    pub suggests: Option<Vec<crate::state::RegistrySuggestion>>,

    /// Server defaults
    #[serde(default)]
    pub defaults: Option<crate::state::RegistryDefaults>,
}

fn default_registry_version() -> u32 {
    1
}

/// What role this manifest serves, inferred from which sections are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestRole {
    /// Only `[registry]` section -- pure skill registry
    Registry,
    /// `[skill]` section (with or without `[project]`) -- single inline skill
    SingleSkill,
    /// `[skills]` section (with or without `[project]`) -- multi-skill directory
    MultiSkill,
    /// Only `[project]` section -- project metadata without embedded skills
    ProjectOnly,
}

impl SkilletToml {
    /// Determine what role this manifest serves.
    pub fn role(&self) -> ManifestRole {
        // A manifest can have multiple sections; priority is:
        // skill/skills > registry > project-only
        if self.skill.is_some() {
            ManifestRole::SingleSkill
        } else if self.skills.is_some() {
            ManifestRole::MultiSkill
        } else if self.registry.is_some() {
            ManifestRole::Registry
        } else {
            ManifestRole::ProjectOnly
        }
    }

    /// Convert the `[registry]` section into a `RegistryConfig` for backward
    /// compatibility with the existing registry loading pipeline.
    pub fn into_registry_config(&self) -> Option<crate::state::RegistryConfig> {
        let reg = self.registry.as_ref()?;
        Some(crate::state::RegistryConfig {
            registry: crate::state::RegistryInfo {
                name: reg.name.clone(),
                version: reg.version,
                description: reg.description.clone(),
                maintainer: reg.maintainer.clone(),
                urls: reg.urls.clone(),
                auth: reg.auth.clone(),
                suggests: reg.suggests.clone(),
                defaults: reg.defaults.clone(),
            },
        })
    }
}

/// Load and parse `skillet.toml` from a directory.
///
/// Returns `Ok(None)` if the file does not exist, `Err` if it exists but
/// fails to parse.
pub fn load_skillet_toml(dir: &Path) -> crate::error::Result<Option<SkilletToml>> {
    let path = dir.join("skillet.toml");
    if !path.is_file() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(&path).map_err(|e| Error::FileRead {
        path: path.clone(),
        source: e,
    })?;

    let manifest: SkilletToml = toml::from_str(&raw).map_err(|e| Error::TomlParse {
        path: path.clone(),
        source: e,
    })?;

    Ok(Some(manifest))
}

/// Search for `skillet.toml` by walking up from `start` directory.
///
/// Similar to how git finds `.git/` -- walks up until it finds a directory
/// containing `skillet.toml` or reaches the filesystem root.
pub fn find_skillet_toml(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join("skillet.toml");
        if candidate.is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Parsed YAML frontmatter fields from a SKILL.md file.
///
/// npm-style skill repos (redis/agent-skills, anthropics/skills, etc.)
/// store metadata in YAML frontmatter rather than `skill.toml`. This struct
/// captures the standard fields used across those repos.
#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub license: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
}

/// Parse YAML frontmatter from SKILL.md content.
///
/// Handles the simple key-value format used in npm skill repos:
/// ```text
/// ---
/// name: my-skill
/// description: A helpful skill
/// version: 1.0.0
/// tags: [caching, redis]
/// ---
/// ```
///
/// Returns `None` if the content doesn't start with `---` frontmatter.
/// This is a simple line-by-line parser (no YAML dependency) matching
/// the existing pattern in `validate.rs`.
pub fn parse_frontmatter(skill_md: &str) -> Option<Frontmatter> {
    let mut lines = skill_md.lines();

    // First line must be "---"
    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut fm = Frontmatter::default();

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            // End of frontmatter
            return Some(fm);
        }

        // Skip empty lines and comments inside frontmatter
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse "key: value" pairs
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();
        // Strip optional surrounding quotes
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);

        match key {
            "name" => fm.name = Some(value.to_string()),
            "description" => fm.description = Some(value.to_string()),
            "version" => fm.version = Some(value.to_string()),
            "license" => fm.license = Some(value.to_string()),
            "author" => fm.author = Some(value.to_string()),
            "tags" => {
                // Parse inline array: [tag1, tag2] or comma-separated
                let inner = value
                    .strip_prefix('[')
                    .and_then(|v| v.strip_suffix(']'))
                    .unwrap_or(value);
                fm.tags = inner
                    .split(',')
                    .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
            }
            _ => {} // Ignore unknown keys (metadata.*, etc.)
        }
    }

    // Reached end of file without closing "---"
    None
}

/// Infer skill metadata from directory context when `skill.toml` is absent.
///
/// Uses the directory name as the skill name, and attempts to resolve the
/// owner from (in order): `skillet.toml` project authors' github handle,
/// the git remote origin, or the parent directory name.
///
/// Categories and tags cascade from the project section if available.
pub fn infer_metadata(
    skill_dir: &Path,
    skill_md: &str,
    manifest: Option<&SkilletToml>,
) -> crate::state::SkillMetadata {
    let frontmatter = parse_frontmatter(skill_md);

    let name = skill_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let owner = infer_owner(skill_dir, manifest);

    // Precedence: manifest > frontmatter > inference
    let description = frontmatter
        .as_ref()
        .and_then(|fm| fm.description.clone())
        .unwrap_or_else(|| extract_description(skill_md));

    let version = frontmatter
        .as_ref()
        .and_then(|fm| fm.version.clone())
        .unwrap_or_else(|| "0.1.0".to_string());

    let license = manifest
        .and_then(|m| m.project.as_ref())
        .and_then(|p| p.license.clone())
        .or_else(|| frontmatter.as_ref().and_then(|fm| fm.license.clone()));

    let author = frontmatter.as_ref().and_then(|fm| {
        fm.author.as_ref().map(|a| crate::state::AuthorInfo {
            name: Some(a.clone()),
            github: None,
        })
    });

    let (categories, tags) = if let Some(m) = manifest {
        let cats = m
            .project
            .as_ref()
            .map(|p| p.categories.clone())
            .unwrap_or_default();
        let mut tags = m
            .project
            .as_ref()
            .map(|p| p.tags.clone())
            .unwrap_or_default();
        // Merge frontmatter tags if manifest has none
        if tags.is_empty()
            && let Some(ref fm) = frontmatter
        {
            tags = fm.tags.clone();
        }
        (cats, tags)
    } else {
        let tags = frontmatter
            .as_ref()
            .map(|fm| fm.tags.clone())
            .unwrap_or_default();
        (Vec::new(), tags)
    };

    let classification = if !categories.is_empty() || !tags.is_empty() {
        Some(crate::state::Classification { categories, tags })
    } else {
        None
    };

    crate::state::SkillMetadata {
        skill: crate::state::SkillInfo {
            name,
            owner,
            version,
            description,
            trigger: None,
            license,
            author,
            classification,
            compatibility: None,
        },
    }
}

/// Infer the owner for a skill directory.
///
/// Tries (in order):
/// 1. First author's github handle from the manifest
/// 2. Git remote origin (extract owner from GitHub URL)
/// 3. Parent directory name
fn infer_owner(skill_dir: &Path, manifest: Option<&SkilletToml>) -> String {
    // 1. From manifest authors
    if let Some(m) = manifest
        && let Some(ref project) = m.project
        && let Some(author) = project.authors.first()
        && let Some(ref gh) = author.github
    {
        return gh.clone();
    }

    // 2. From git remote origin
    if let Some(owner) = owner_from_git_remote(skill_dir) {
        return owner;
    }

    // 3. From parent directory name
    skill_dir
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Try to extract the repository owner from the git remote origin URL.
fn owner_from_git_remote(dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // Handle "git@github.com:owner/repo.git" and "https://github.com/owner/repo.git"
    let path_part = if let Some(rest) = url.strip_prefix("git@") {
        rest.split_once(':')?.1
    } else {
        url.rsplit("://").next()?.split_once('/')?.1
    };

    let segments: Vec<&str> = path_part
        .trim_end_matches(".git")
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    segments.first().map(|s| s.to_string())
}

/// Extract a description from SKILL.md content.
///
/// Takes the first non-empty line that doesn't start with `#` or `---`.
/// Truncates to 200 characters. Falls back to `"Embedded skill"`.
fn extract_description(skill_md: &str) -> String {
    let mut in_frontmatter = false;
    for line in skill_md.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_frontmatter = !in_frontmatter;
            continue;
        }
        if in_frontmatter {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let max = 200;
        return match trimmed.char_indices().nth(max) {
            Some((idx, _)) => trimmed[..idx].to_string(),
            None => trimmed.to_string(),
        };
    }
    "Embedded skill".to_string()
}

/// Load embedded skills from a project with a `skillet.toml` manifest.
///
/// For `[skill]`: builds an entry from the inline skill section + SKILL.md
/// at the project root (or specified path).
/// For `[skills]`: walks the skills directory and loads each member.
///
/// Each entry uses `SkillSource::Embedded { project, path }`.
/// Metadata inheritance: owner, license, categories, tags cascade from
/// `[project]` to skills.
pub fn load_embedded_skills(
    project_root: &Path,
    manifest: &SkilletToml,
) -> crate::state::SkillIndex {
    let mut index = crate::state::SkillIndex::default();

    let project_name = manifest
        .project
        .as_ref()
        .and_then(|p| p.name.clone())
        .or_else(|| {
            project_root
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Handle [skill] section: single inline skill
    if let Some(ref skill_section) = manifest.skill {
        let skill_path = match &skill_section.path {
            Some(p) => project_root.join(p),
            None => project_root.to_path_buf(),
        };

        match build_embedded_entry(&skill_path, skill_section, manifest, &project_name) {
            Ok(entry) => {
                let key = (entry.owner.clone(), entry.name.clone());
                tracing::debug!(
                    skill = %entry.name,
                    project = %project_name,
                    "Loaded embedded skill (inline)"
                );
                index.skills.insert(key, entry);
            }
            Err(e) => {
                tracing::warn!(
                    path = %skill_path.display(),
                    error = %e,
                    "Failed to load embedded inline skill"
                );
            }
        }
    }

    // Handle [skills] section: multi-skill directory
    if let Some(ref skills_section) = manifest.skills {
        let skills_dir = project_root.join(skills_section.resolved_path());
        if skills_dir.is_dir() {
            load_skills_dir(
                &skills_dir,
                skills_section,
                manifest,
                &project_name,
                &mut index,
            );
        } else {
            tracing::debug!(
                path = %skills_dir.display(),
                "Skills directory not found, skipping"
            );
        }
    }

    index
}

/// Build a `SkillEntry` from an inline `[skill]` section.
fn build_embedded_entry(
    skill_path: &Path,
    skill_section: &SkillSection,
    manifest: &SkilletToml,
    project_name: &str,
) -> anyhow::Result<crate::state::SkillEntry> {
    let md_path = skill_path.join("SKILL.md");
    let skill_md = std::fs::read_to_string(&md_path)?;

    if skill_md.trim().is_empty() {
        anyhow::bail!("SKILL.md is empty at {}", skill_path.display());
    }

    let name = skill_section
        .name
        .clone()
        .or_else(|| manifest.project.as_ref().and_then(|p| p.name.clone()))
        .unwrap_or_else(|| {
            skill_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    let owner = infer_owner(skill_path, Some(manifest));
    let description = skill_section
        .description
        .clone()
        .or_else(|| {
            manifest
                .project
                .as_ref()
                .and_then(|p| p.description.clone())
        })
        .unwrap_or_else(|| extract_description(&skill_md));

    let version = skill_section
        .version
        .clone()
        .unwrap_or_else(|| "0.1.0".to_string());

    let categories = skill_section
        .categories
        .clone()
        .or_else(|| manifest.project.as_ref().map(|p| p.categories.clone()))
        .unwrap_or_default();

    let tags = skill_section
        .tags
        .clone()
        .or_else(|| manifest.project.as_ref().map(|p| p.tags.clone()))
        .unwrap_or_default();

    let classification = if !categories.is_empty() || !tags.is_empty() {
        Some(crate::state::Classification { categories, tags })
    } else {
        None
    };

    let files = crate::index::load_extra_files(skill_path).unwrap_or_default();

    let skill_toml_raw = String::new();
    let metadata = crate::state::SkillMetadata {
        skill: crate::state::SkillInfo {
            name: name.clone(),
            owner: owner.clone(),
            version: version.clone(),
            description,
            trigger: None,
            license: manifest.project.as_ref().and_then(|p| p.license.clone()),
            author: None,
            classification,
            compatibility: None,
        },
    };

    Ok(crate::state::SkillEntry {
        owner,
        name,
        registry_path: None,
        source: crate::state::SkillSource::Embedded {
            project: project_name.to_string(),
            path: skill_path.to_path_buf(),
        },
        versions: vec![crate::state::SkillVersion {
            version,
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

/// Build a `SkillEntry` from a skill subdirectory (used by `[skills]`).
fn build_embedded_entry_from_dir(
    skill_dir: &Path,
    manifest: &SkilletToml,
    project_name: &str,
) -> anyhow::Result<crate::state::SkillEntry> {
    let md_path = skill_dir.join("SKILL.md");
    let skill_md = std::fs::read_to_string(&md_path)?;

    if skill_md.trim().is_empty() {
        anyhow::bail!("SKILL.md is empty at {}", skill_dir.display());
    }

    let frontmatter = parse_frontmatter(&skill_md);

    let name = skill_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let owner = infer_owner(skill_dir, Some(manifest));

    // Precedence: frontmatter > extract_description fallback
    let description = frontmatter
        .as_ref()
        .and_then(|fm| fm.description.clone())
        .unwrap_or_else(|| extract_description(&skill_md));

    let version = frontmatter
        .as_ref()
        .and_then(|fm| fm.version.clone())
        .unwrap_or_else(|| "0.1.0".to_string());

    let license = manifest
        .project
        .as_ref()
        .and_then(|p| p.license.clone())
        .or_else(|| frontmatter.as_ref().and_then(|fm| fm.license.clone()));

    let author = frontmatter.as_ref().and_then(|fm| {
        fm.author.as_ref().map(|a| crate::state::AuthorInfo {
            name: Some(a.clone()),
            github: None,
        })
    });

    let categories = manifest
        .project
        .as_ref()
        .map(|p| p.categories.clone())
        .unwrap_or_default();

    let mut tags = manifest
        .project
        .as_ref()
        .map(|p| p.tags.clone())
        .unwrap_or_default();
    if tags.is_empty()
        && let Some(ref fm) = frontmatter
    {
        tags = fm.tags.clone();
    }

    let classification = if !categories.is_empty() || !tags.is_empty() {
        Some(crate::state::Classification { categories, tags })
    } else {
        None
    };

    // Read skill.toml if present for richer metadata
    let skill_toml_raw = std::fs::read_to_string(skill_dir.join("skill.toml")).unwrap_or_default();

    let files = crate::index::load_extra_files(skill_dir).unwrap_or_default();

    let metadata = crate::state::SkillMetadata {
        skill: crate::state::SkillInfo {
            name: name.clone(),
            owner: owner.clone(),
            version: version.clone(),
            description,
            trigger: None,
            license,
            author,
            classification,
            compatibility: None,
        },
    };

    Ok(crate::state::SkillEntry {
        owner,
        name,
        registry_path: None,
        source: crate::state::SkillSource::Embedded {
            project: project_name.to_string(),
            path: skill_dir.to_path_buf(),
        },
        versions: vec![crate::state::SkillVersion {
            version,
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

/// Scan a skills directory and load each member.
fn load_skills_dir(
    skills_dir: &Path,
    skills_section: &SkillsSection,
    manifest: &SkilletToml,
    project_name: &str,
    index: &mut crate::state::SkillIndex,
) {
    let entries = match std::fs::read_dir(skills_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(
                path = %skills_dir.display(),
                error = %e,
                "Cannot read skills directory"
            );
            return;
        }
    };

    let mut subdirs: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    subdirs.sort_by_key(|e| e.file_name());

    for entry in subdirs {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();
        if dir_name.starts_with('.') {
            continue;
        }

        // If members list is non-empty, only include listed members
        if !skills_section.members.is_empty() && !skills_section.members.contains(&dir_name) {
            continue;
        }

        // Must have SKILL.md
        if !path.join("SKILL.md").is_file() {
            continue;
        }

        match build_embedded_entry_from_dir(&path, manifest, project_name) {
            Ok(entry) => {
                let key = (entry.owner.clone(), entry.name.clone());
                tracing::debug!(
                    skill = %entry.name,
                    project = %project_name,
                    "Loaded embedded skill (multi)"
                );
                index.skills.insert(key, entry);
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to load embedded skill"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_missing_skillet_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let result = load_skillet_toml(tmp.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_empty_skillet_toml() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("skillet.toml"), "").unwrap();
        let manifest = load_skillet_toml(tmp.path()).unwrap().unwrap();
        assert!(manifest.project.is_none());
        assert!(manifest.skill.is_none());
        assert!(manifest.skills.is_none());
        assert!(manifest.registry.is_none());
        assert_eq!(manifest.role(), ManifestRole::ProjectOnly);
    }

    #[test]
    fn test_load_registry_only() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[registry]
name = "my-registry"
description = "Test registry"
"#,
        )
        .unwrap();

        let manifest = load_skillet_toml(tmp.path()).unwrap().unwrap();
        assert_eq!(manifest.role(), ManifestRole::Registry);

        let reg = manifest.registry.as_ref().unwrap();
        assert_eq!(reg.name, "my-registry");
        assert_eq!(reg.description.as_deref(), Some("Test registry"));
    }

    #[test]
    fn test_load_single_skill() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[project]
name = "my-tool"
description = "A CLI tool"

[skill]
name = "my-tool-usage"
description = "How to use my-tool"
"#,
        )
        .unwrap();

        let manifest = load_skillet_toml(tmp.path()).unwrap().unwrap();
        assert_eq!(manifest.role(), ManifestRole::SingleSkill);
        assert_eq!(
            manifest.project.as_ref().unwrap().name.as_deref(),
            Some("my-tool")
        );
        assert_eq!(
            manifest.skill.as_ref().unwrap().name.as_deref(),
            Some("my-tool-usage")
        );
    }

    #[test]
    fn test_load_multi_skill() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[project]
name = "my-project"

[skills]
path = "skills/"
members = ["api-usage", "debugging"]
"#,
        )
        .unwrap();

        let manifest = load_skillet_toml(tmp.path()).unwrap().unwrap();
        assert_eq!(manifest.role(), ManifestRole::MultiSkill);
        let skills = manifest.skills.as_ref().unwrap();
        assert_eq!(skills.resolved_path(), "skills/");
        assert_eq!(skills.members, vec!["api-usage", "debugging"]);
    }

    #[test]
    fn test_load_hybrid_skill_and_registry() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[project]
name = "my-registry"

[registry]
name = "community-skills"

[skill]
name = "meta-skill"
"#,
        )
        .unwrap();

        let manifest = load_skillet_toml(tmp.path()).unwrap().unwrap();
        // skill takes priority over registry for role determination
        assert_eq!(manifest.role(), ManifestRole::SingleSkill);
        assert!(manifest.registry.is_some());
    }

    #[test]
    fn test_into_registry_config() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[registry]
name = "test-reg"
version = 1
description = "A test"
"#,
        )
        .unwrap();

        let manifest = load_skillet_toml(tmp.path()).unwrap().unwrap();
        let config = manifest.into_registry_config().unwrap();
        assert_eq!(config.registry.name, "test-reg");
        assert_eq!(config.registry.version, 1);
        assert_eq!(config.registry.description.as_deref(), Some("A test"));
    }

    #[test]
    fn test_find_skillet_toml_in_current() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("skillet.toml"), "[project]\n").unwrap();

        let found = find_skillet_toml(tmp.path());
        assert_eq!(found, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn test_find_skillet_toml_in_parent() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("skillet.toml"), "[project]\n").unwrap();
        let child = tmp.path().join("src");
        std::fs::create_dir_all(&child).unwrap();

        let found = find_skillet_toml(&child);
        assert_eq!(found, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn test_find_skillet_toml_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let found = find_skillet_toml(tmp.path());
        assert!(found.is_none());
    }

    #[test]
    fn test_skills_section_default_path() {
        let section = SkillsSection::default();
        assert_eq!(section.resolved_path(), ".skillet");
    }

    #[test]
    fn test_project_authors() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            r#"
[project]
name = "test"

[[project.authors]]
name = "Alice"
github = "alice"

[[project.authors]]
name = "Bob"
email = "bob@example.com"
"#,
        )
        .unwrap();

        let manifest = load_skillet_toml(tmp.path()).unwrap().unwrap();
        let authors = &manifest.project.as_ref().unwrap().authors;
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0].github.as_deref(), Some("alice"));
        assert_eq!(authors[1].email.as_deref(), Some("bob@example.com"));
    }

    #[test]
    fn test_malformed_skillet_toml_errors() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skillet.toml"),
            "this is not valid toml [[[",
        )
        .unwrap();

        let result = load_skillet_toml(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_infer_metadata_from_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let skill_md = "# My Skill\n\nA helpful skill for testing.\n";
        let metadata = infer_metadata(&skill_dir, skill_md, None);

        assert_eq!(metadata.skill.name, "my-skill");
        assert_eq!(metadata.skill.version, "0.1.0");
        assert_eq!(metadata.skill.description, "A helpful skill for testing.");
    }

    #[test]
    fn test_infer_metadata_with_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let manifest = SkilletToml {
            project: Some(ProjectSection {
                name: Some("my-project".to_string()),
                categories: vec!["development".to_string()],
                tags: vec!["rust".to_string()],
                authors: vec![ProjectAuthor {
                    github: Some("alice".to_string()),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let skill_md = "# My Skill\n\nA skill.\n";
        let metadata = infer_metadata(&skill_dir, skill_md, Some(&manifest));

        assert_eq!(metadata.skill.owner, "alice");
        assert_eq!(
            metadata.skill.classification.as_ref().unwrap().categories,
            vec!["development"]
        );
        assert_eq!(
            metadata.skill.classification.as_ref().unwrap().tags,
            vec!["rust"]
        );
    }

    #[test]
    fn test_extract_description_skips_frontmatter() {
        let md = "---\nname: test\n---\n\n# Heading\n\nActual description.";
        assert_eq!(extract_description(md), "Actual description.");
    }

    #[test]
    fn test_extract_description_fallback() {
        assert_eq!(extract_description("# Only heading"), "Embedded skill");
        assert_eq!(extract_description(""), "Embedded skill");
    }

    #[test]
    fn test_load_embedded_single_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create skillet.toml with [skill] section
        std::fs::write(
            root.join("skillet.toml"),
            r#"
[project]
name = "my-cli"
description = "A CLI tool"

[[project.authors]]
name = "Test"
github = "testuser"

[skill]
name = "my-cli-usage"
description = "How to use my-cli"
"#,
        )
        .unwrap();

        // Create SKILL.md at root
        std::fs::write(
            root.join("SKILL.md"),
            "# My CLI Usage\n\nUse my-cli to do things.\n",
        )
        .unwrap();

        let manifest = load_skillet_toml(root).unwrap().unwrap();
        let index = load_embedded_skills(root, &manifest);

        assert_eq!(index.skills.len(), 1);
        let entry = index
            .skills
            .get(&("testuser".to_string(), "my-cli-usage".to_string()))
            .expect("should find embedded skill");
        assert_eq!(entry.name, "my-cli-usage");
        assert_eq!(entry.owner, "testuser");
        match &entry.source {
            crate::state::SkillSource::Embedded { project, .. } => {
                assert_eq!(project, "my-cli");
            }
            other => panic!("Expected Embedded source, got {other:?}"),
        }
    }

    #[test]
    fn test_load_embedded_multi_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(
            root.join("skillet.toml"),
            r#"
[project]
name = "my-project"
categories = ["development"]

[[project.authors]]
github = "dev"

[skills]
path = ".skillet"
"#,
        )
        .unwrap();

        // Create .skillet/ with two skills
        let skill1 = root.join(".skillet/api-usage");
        std::fs::create_dir_all(&skill1).unwrap();
        std::fs::write(
            skill1.join("SKILL.md"),
            "# API Usage\n\nHow to use the API.\n",
        )
        .unwrap();

        let skill2 = root.join(".skillet/debugging");
        std::fs::create_dir_all(&skill2).unwrap();
        std::fs::write(skill2.join("SKILL.md"), "# Debugging\n\nHow to debug.\n").unwrap();

        let manifest = load_skillet_toml(root).unwrap().unwrap();
        let index = load_embedded_skills(root, &manifest);

        assert_eq!(index.skills.len(), 2);
        assert!(
            index
                .skills
                .contains_key(&("dev".to_string(), "api-usage".to_string()))
        );
        assert!(
            index
                .skills
                .contains_key(&("dev".to_string(), "debugging".to_string()))
        );
    }

    #[test]
    fn test_load_embedded_multi_skill_with_members_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(
            root.join("skillet.toml"),
            r#"
[project]
name = "filtered"

[[project.authors]]
github = "dev"

[skills]
path = ".skillet"
members = ["included"]
"#,
        )
        .unwrap();

        let included = root.join(".skillet/included");
        std::fs::create_dir_all(&included).unwrap();
        std::fs::write(included.join("SKILL.md"), "# Included\n\nYes.\n").unwrap();

        let excluded = root.join(".skillet/excluded");
        std::fs::create_dir_all(&excluded).unwrap();
        std::fs::write(excluded.join("SKILL.md"), "# Excluded\n\nNo.\n").unwrap();

        let manifest = load_skillet_toml(root).unwrap().unwrap();
        let index = load_embedded_skills(root, &manifest);

        assert_eq!(index.skills.len(), 1);
        assert!(
            index
                .skills
                .contains_key(&("dev".to_string(), "included".to_string()))
        );
    }

    #[test]
    fn test_load_embedded_hybrid_skill_and_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(
            root.join("skillet.toml"),
            r#"
[project]
name = "hybrid"

[[project.authors]]
github = "dev"

[skill]
name = "primary"
description = "Primary skill"

[skills]
path = ".skillet"
"#,
        )
        .unwrap();

        // Inline skill at root
        std::fs::write(root.join("SKILL.md"), "# Primary\n\nMain skill.\n").unwrap();

        // Multi skills
        let extra = root.join(".skillet/extra");
        std::fs::create_dir_all(&extra).unwrap();
        std::fs::write(extra.join("SKILL.md"), "# Extra\n\nExtra skill.\n").unwrap();

        let manifest = load_skillet_toml(root).unwrap().unwrap();
        let index = load_embedded_skills(root, &manifest);

        assert_eq!(index.skills.len(), 2);
        assert!(
            index
                .skills
                .contains_key(&("dev".to_string(), "primary".to_string()))
        );
        assert!(
            index
                .skills
                .contains_key(&("dev".to_string(), "extra".to_string()))
        );
    }

    // ── Frontmatter parsing tests ────────────────────────────────────

    #[test]
    fn test_parse_frontmatter_basic() {
        let md = "---\nname: my-skill\ndescription: A helpful skill\nversion: 1.2.0\nlicense: MIT\nauthor: Alice\n---\n\n# Content\n";
        let fm = parse_frontmatter(md).expect("should parse frontmatter");
        assert_eq!(fm.name.as_deref(), Some("my-skill"));
        assert_eq!(fm.description.as_deref(), Some("A helpful skill"));
        assert_eq!(fm.version.as_deref(), Some("1.2.0"));
        assert_eq!(fm.license.as_deref(), Some("MIT"));
        assert_eq!(fm.author.as_deref(), Some("Alice"));
        assert!(fm.tags.is_empty());
    }

    #[test]
    fn test_parse_frontmatter_with_tags() {
        let md = "---\nname: redis-caching\ntags: [caching, redis, performance]\n---\n\n# Redis\n";
        let fm = parse_frontmatter(md).expect("should parse frontmatter");
        assert_eq!(fm.name.as_deref(), Some("redis-caching"));
        assert_eq!(fm.tags, vec!["caching", "redis", "performance"]);
    }

    #[test]
    fn test_parse_frontmatter_quoted_values() {
        let md = "---\nname: \"quoted-skill\"\ndescription: 'single quoted'\n---\n";
        let fm = parse_frontmatter(md).expect("should parse frontmatter");
        assert_eq!(fm.name.as_deref(), Some("quoted-skill"));
        assert_eq!(fm.description.as_deref(), Some("single quoted"));
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let md = "# Just a heading\n\nNo frontmatter here.\n";
        assert!(parse_frontmatter(md).is_none());
    }

    #[test]
    fn test_parse_frontmatter_empty_content() {
        assert!(parse_frontmatter("").is_none());
    }

    #[test]
    fn test_parse_frontmatter_unclosed() {
        let md = "---\nname: broken\n";
        assert!(
            parse_frontmatter(md).is_none(),
            "unclosed frontmatter should return None"
        );
    }

    #[test]
    fn test_infer_metadata_uses_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let skill_md = "---\ndescription: From frontmatter\nversion: 3.0.0\ntags: [fast, cool]\n---\n\n# Heading\n\nBody text.\n";
        let metadata = infer_metadata(&skill_dir, skill_md, None);

        assert_eq!(metadata.skill.name, "my-skill");
        assert_eq!(metadata.skill.version, "3.0.0");
        assert_eq!(metadata.skill.description, "From frontmatter");
        assert_eq!(
            metadata.skill.classification.as_ref().unwrap().tags,
            vec!["fast", "cool"]
        );
    }

    #[test]
    fn test_infer_metadata_manifest_overrides_frontmatter_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let manifest = SkilletToml {
            project: Some(ProjectSection {
                tags: vec!["manifest-tag".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };

        let skill_md = "---\ntags: [fm-tag]\n---\n\n# Heading\n";
        let metadata = infer_metadata(&skill_dir, skill_md, Some(&manifest));

        // Manifest tags take precedence over frontmatter tags
        assert_eq!(
            metadata.skill.classification.as_ref().unwrap().tags,
            vec!["manifest-tag"]
        );
    }
}
