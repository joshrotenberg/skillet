//! Shared application state and data models

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::search::SkillSearch;

/// Shared state for the MCP server
pub struct AppState {
    /// In-memory skill index, refreshable
    pub index: RwLock<SkillIndex>,
    /// BM25 search index over skills, rebuilt on refresh
    pub search: RwLock<SkillSearch>,
    /// Paths to all registry roots (git checkouts)
    pub registry_paths: Vec<PathBuf>,
    /// Remote URLs (for cache key generation)
    pub remote_urls: Vec<String>,
    /// Registry configuration (from skillet.toml or defaults)
    pub config: RegistryConfig,
}

impl AppState {
    pub fn new(
        registry_paths: Vec<PathBuf>,
        remote_urls: Vec<String>,
        index: SkillIndex,
        search: SkillSearch,
        config: RegistryConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            index: RwLock::new(index),
            search: RwLock::new(search),
            registry_paths,
            remote_urls,
            config,
        })
    }
}

/// Top-level registry configuration, parsed from `skillet.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryConfig {
    pub registry: RegistryInfo,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registry: RegistryInfo {
                name: default_registry_name(),
                version: default_registry_version(),
                description: None,
                maintainer: None,
                urls: None,
                auth: None,
                suggests: None,
                defaults: None,
            },
        }
    }
}

/// Core registry metadata.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RegistryInfo {
    #[serde(default = "default_registry_name")]
    pub name: String,
    #[serde(default = "default_registry_version")]
    pub version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub maintainer: Option<RegistryMaintainer>,
    #[serde(default)]
    pub urls: Option<RegistryUrls>,
    #[serde(default)]
    pub auth: Option<RegistryAuth>,
    #[serde(default)]
    pub suggests: Option<Vec<RegistrySuggestion>>,
    #[serde(default)]
    pub defaults: Option<RegistryDefaults>,
}

/// Registry maintainer information.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryMaintainer {
    pub name: Option<String>,
    pub github: Option<String>,
    pub email: Option<String>,
}

/// A suggested registry for discovery (lightweight federation).
#[derive(Debug, Clone, Deserialize)]
pub struct RegistrySuggestion {
    pub url: String,
    pub description: Option<String>,
}

/// Server defaults that a registry can specify.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryDefaults {
    pub refresh_interval: Option<String>,
}

/// Optional URL endpoints for non-git-backed registries.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RegistryUrls {
    pub download: Option<String>,
    pub api: Option<String>,
}

/// Optional auth configuration.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RegistryAuth {
    #[serde(default)]
    pub required: bool,
}

fn default_registry_name() -> String {
    "skillet".to_string()
}

fn default_registry_version() -> u32 {
    1
}

/// In-memory index of all skills across all registries
#[derive(Debug, Default)]
pub struct SkillIndex {
    /// All skills keyed by (owner, name)
    pub skills: HashMap<(String, String), SkillEntry>,
    /// All known categories with skill counts
    pub categories: BTreeMap<String, usize>,
}

impl SkillIndex {
    /// Merge another index into this one. Skills already present are skipped
    /// (first registry wins).
    pub fn merge(&mut self, other: SkillIndex) {
        for (key, entry) in other.skills {
            if self.skills.contains_key(&key) {
                tracing::debug!(
                    owner = %key.0,
                    name = %key.1,
                    "Skipping duplicate skill from secondary registry"
                );
                continue;
            }
            // Update category counts for the new entry
            if let Some(v) = entry.latest()
                && let Some(ref c) = v.metadata.skill.classification
            {
                for cat in &c.categories {
                    *self.categories.entry(cat.clone()).or_insert(0) += 1;
                }
            }
            self.skills.insert(key, entry);
        }
    }
}

/// Where a skill was discovered from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SkillSource {
    /// From a git-backed registry with skill.toml
    #[default]
    Registry,
    /// Auto-discovered from a local agent skills directory
    Local {
        /// Agent platform (e.g. "claude", "agents")
        platform: String,
        /// Absolute path to the skill directory on disk
        path: PathBuf,
    },
    /// Embedded in a project via `skillet.toml`
    Embedded {
        /// Project name from the manifest
        project: String,
        /// Absolute path to the skill directory on disk
        path: PathBuf,
    },
}

impl SkillSource {
    /// Returns a human-readable label for the source, or `None` for registry skills.
    pub fn label(&self) -> Option<String> {
        match self {
            Self::Registry => None,
            Self::Local { platform, .. } => Some(format!("local ({platform})")),
            Self::Embedded { project, .. } => Some(format!("embedded ({project})")),
        }
    }

    /// Returns the on-disk path for local or embedded skills.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Registry => None,
            Self::Local { path, .. } | Self::Embedded { path, .. } => Some(path),
        }
    }
}

/// A skill with all its versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub owner: String,
    pub name: String,
    /// Relative path from registry root (e.g., "acme/lang/java/maven-build").
    /// None for flat skills at the standard `owner/name/` depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_path: Option<String>,
    pub versions: Vec<SkillVersion>,
    #[serde(default)]
    pub source: SkillSource,
}

impl SkillEntry {
    /// The latest (most recently added) non-yanked version
    pub fn latest(&self) -> Option<&SkillVersion> {
        self.versions.iter().rev().find(|v| !v.yanked)
    }
}

/// A single published version of a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    pub version: String,
    pub metadata: SkillMetadata,
    pub skill_md: String,
    pub skill_toml_raw: String,
    pub yanked: bool,
    /// Extra files in the skillpack (scripts/, references/, assets/)
    /// Keyed by relative path from skill root (e.g. "scripts/lint.sh")
    pub files: HashMap<String, SkillFile>,
    /// ISO 8601 publish timestamp from versions.toml
    pub published: Option<String>,
    /// Whether this version's content is loaded from disk.
    /// Historical versions listed in versions.toml have `has_content = false`.
    pub has_content: bool,
    /// Computed composite content hash (SHA256 of all files)
    pub content_hash: Option<String>,
    /// Integrity verification result: None if no manifest, Some(true) if
    /// verified, Some(false) if mismatch detected
    pub integrity_ok: Option<bool>,
}

/// Deserialized versions.toml manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionsManifest {
    pub versions: Vec<VersionRecord>,
}

/// A single version record from versions.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRecord {
    pub version: String,
    pub published: String,
    #[serde(default)]
    pub yanked: bool,
}

/// An extra file in a skillpack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFile {
    pub content: String,
    pub mime_type: String,
}

/// Parsed skill.toml metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub skill: SkillInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub owner: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub author: Option<AuthorInfo>,
    #[serde(default)]
    pub classification: Option<Classification>,
    #[serde(default)]
    pub compatibility: Option<Compatibility>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub github: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Known abstract capability names for `required_capabilities`.
///
/// Values outside this list trigger a validation warning (not error) to
/// allow forward-compatible extension while catching typos.
pub const KNOWN_CAPABILITIES: &[&str] = &[
    "shell_exec",
    "file_read",
    "file_write",
    "file_edit",
    "web_fetch",
    "web_search",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compatibility {
    #[serde(default)]
    pub requires_tool_use: Option<bool>,
    #[serde(default)]
    pub requires_vision: Option<bool>,
    #[serde(default)]
    pub min_context_tokens: Option<u64>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub required_mcp_servers: Vec<String>,
    #[serde(default)]
    pub verified_with: Vec<String>,
}

/// Summary of a skill for search results
#[derive(Debug, Clone, Serialize)]
pub struct SkillSummary {
    pub owner: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub verified_with: Vec<String>,
    /// Extra files included in the skillpack
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// When the latest version was published (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// Total number of versions (including yanked)
    pub version_count: usize,
    /// All available (non-yanked) version strings, oldest first
    pub available_versions: Vec<String>,
    /// Composite content hash of the latest version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Integrity verification status: "verified", "failed", or absent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<String>,
    /// Source label for display (e.g. "local (claude)")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
}

impl SkillSummary {
    pub fn from_entry(entry: &SkillEntry) -> Option<Self> {
        let v = entry.latest()?;
        let info = &v.metadata.skill;
        let classification = info.classification.as_ref();
        let compat = info.compatibility.as_ref();
        let mut files: Vec<String> = v.files.keys().cloned().collect();
        files.sort();
        let available_versions: Vec<String> = entry
            .versions
            .iter()
            .filter(|v| !v.yanked)
            .map(|v| v.version.clone())
            .collect();
        let integrity = match v.integrity_ok {
            Some(true) => Some("verified".to_string()),
            Some(false) => Some("failed".to_string()),
            None => None,
        };

        Some(Self {
            owner: entry.owner.clone(),
            name: entry.name.clone(),
            version: info.version.clone(),
            description: info.description.clone(),
            trigger: info.trigger.clone(),
            categories: classification
                .map(|c| c.categories.clone())
                .unwrap_or_default(),
            tags: classification.map(|c| c.tags.clone()).unwrap_or_default(),
            verified_with: compat.map(|c| c.verified_with.clone()).unwrap_or_default(),
            files,
            published: v.published.clone(),
            version_count: entry.versions.len(),
            available_versions,
            content_hash: v.content_hash.clone(),
            integrity,
            source_label: entry.source.label(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal SkillVersion for testing
    fn make_version(version: &str, description: &str, yanked: bool) -> SkillVersion {
        SkillVersion {
            version: version.to_string(),
            metadata: SkillMetadata {
                skill: SkillInfo {
                    name: "test".to_string(),
                    owner: "owner".to_string(),
                    version: version.to_string(),
                    description: description.to_string(),
                    trigger: None,
                    license: None,
                    author: None,
                    classification: None,
                    compatibility: None,
                },
            },
            skill_md: "# Test".to_string(),
            skill_toml_raw: String::new(),
            yanked,
            files: HashMap::new(),
            published: None,
            has_content: true,
            content_hash: None,
            integrity_ok: None,
        }
    }

    /// Helper: build a minimal SkillEntry
    fn make_entry(owner: &str, name: &str, versions: Vec<SkillVersion>) -> SkillEntry {
        SkillEntry {
            owner: owner.to_string(),
            name: name.to_string(),
            registry_path: None,
            versions,
            source: SkillSource::Registry,
        }
    }

    // -- SkillEntry::latest() --

    #[test]
    fn latest_returns_last_non_yanked() {
        let entry = make_entry(
            "acme",
            "tool",
            vec![
                make_version("0.1.0", "first", false),
                make_version("0.2.0", "second", false),
                make_version("0.3.0", "third", false),
            ],
        );
        let latest = entry.latest().unwrap();
        assert_eq!(latest.version, "0.3.0");
    }

    #[test]
    fn latest_skips_yanked() {
        let entry = make_entry(
            "acme",
            "tool",
            vec![
                make_version("0.1.0", "first", false),
                make_version("0.2.0", "second", true),
            ],
        );
        let latest = entry.latest().unwrap();
        assert_eq!(latest.version, "0.1.0");
    }

    #[test]
    fn latest_returns_none_when_all_yanked() {
        let entry = make_entry(
            "acme",
            "tool",
            vec![
                make_version("0.1.0", "first", true),
                make_version("0.2.0", "second", true),
            ],
        );
        assert!(entry.latest().is_none());
    }

    #[test]
    fn latest_returns_none_for_empty_versions() {
        let entry = make_entry("acme", "tool", vec![]);
        assert!(entry.latest().is_none());
    }

    // -- SkillIndex::merge() --

    #[test]
    fn merge_first_registry_wins() {
        let mut primary = SkillIndex::default();
        primary.skills.insert(
            ("acme".into(), "tool".into()),
            make_entry(
                "acme",
                "tool",
                vec![make_version("1.0.0", "primary", false)],
            ),
        );

        let mut secondary = SkillIndex::default();
        secondary.skills.insert(
            ("acme".into(), "tool".into()),
            make_entry(
                "acme",
                "tool",
                vec![make_version("2.0.0", "secondary", false)],
            ),
        );

        primary.merge(secondary);

        assert_eq!(primary.skills.len(), 1);
        let entry = primary.skills.get(&("acme".into(), "tool".into())).unwrap();
        assert_eq!(entry.latest().unwrap().version, "1.0.0");
    }

    #[test]
    fn merge_adds_new_skills() {
        let mut primary = SkillIndex::default();
        primary.skills.insert(
            ("acme".into(), "tool-a".into()),
            make_entry(
                "acme",
                "tool-a",
                vec![make_version("1.0.0", "first", false)],
            ),
        );

        let mut secondary = SkillIndex::default();
        secondary.skills.insert(
            ("acme".into(), "tool-b".into()),
            make_entry(
                "acme",
                "tool-b",
                vec![make_version("1.0.0", "second", false)],
            ),
        );

        primary.merge(secondary);

        assert_eq!(primary.skills.len(), 2);
        assert!(
            primary
                .skills
                .contains_key(&("acme".into(), "tool-b".into()))
        );
    }

    #[test]
    fn merge_updates_category_counts() {
        let mut primary = SkillIndex::default();

        let mut version = make_version("1.0.0", "categorized", false);
        version.metadata.skill.classification = Some(Classification {
            categories: vec!["database".into(), "caching".into()],
            tags: vec![],
        });

        let mut secondary = SkillIndex::default();
        secondary.skills.insert(
            ("acme".into(), "redis".into()),
            make_entry("acme", "redis", vec![version]),
        );

        primary.merge(secondary);

        assert_eq!(primary.categories.get("database"), Some(&1));
        assert_eq!(primary.categories.get("caching"), Some(&1));
    }

    #[test]
    fn merge_accumulates_categories_across_skills() {
        let mut primary = SkillIndex::default();

        let mut v1 = make_version("1.0.0", "first db", false);
        v1.metadata.skill.classification = Some(Classification {
            categories: vec!["database".into()],
            tags: vec![],
        });
        primary.skills.insert(
            ("acme".into(), "pg".into()),
            make_entry("acme", "pg", vec![v1]),
        );
        *primary.categories.entry("database".into()).or_insert(0) += 1;

        let mut v2 = make_version("1.0.0", "second db", false);
        v2.metadata.skill.classification = Some(Classification {
            categories: vec!["database".into()],
            tags: vec![],
        });
        let mut secondary = SkillIndex::default();
        secondary.skills.insert(
            ("acme".into(), "redis".into()),
            make_entry("acme", "redis", vec![v2]),
        );

        primary.merge(secondary);

        assert_eq!(primary.categories.get("database"), Some(&2));
    }

    // -- SkillSource --

    #[test]
    fn source_registry_label_is_none() {
        assert!(SkillSource::Registry.label().is_none());
    }

    #[test]
    fn source_local_label() {
        let source = SkillSource::Local {
            platform: "claude".into(),
            path: PathBuf::from("/tmp/skills/test"),
        };
        assert_eq!(source.label(), Some("local (claude)".into()));
    }

    #[test]
    fn source_embedded_label() {
        let source = SkillSource::Embedded {
            project: "my-tool".into(),
            path: PathBuf::from("/tmp/project/.skillet/test"),
        };
        assert_eq!(source.label(), Some("embedded (my-tool)".into()));
    }

    #[test]
    fn source_registry_path_is_none() {
        assert!(SkillSource::Registry.path().is_none());
    }

    #[test]
    fn source_local_path() {
        let p = PathBuf::from("/tmp/skills/test");
        let source = SkillSource::Local {
            platform: "claude".into(),
            path: p.clone(),
        };
        assert_eq!(source.path(), Some(p.as_path()));
    }

    #[test]
    fn source_embedded_path() {
        let p = PathBuf::from("/tmp/project/.skillet/test");
        let source = SkillSource::Embedded {
            project: "my-tool".into(),
            path: p.clone(),
        };
        assert_eq!(source.path(), Some(p.as_path()));
    }

    // -- SkillSummary::from_entry() --

    #[test]
    fn summary_from_entry_basic() {
        let entry = make_entry(
            "acme",
            "tool",
            vec![make_version("1.0.0", "A great tool", false)],
        );
        let summary = SkillSummary::from_entry(&entry).unwrap();
        assert_eq!(summary.owner, "acme");
        assert_eq!(summary.name, "tool");
        assert_eq!(summary.version, "1.0.0");
        assert_eq!(summary.description, "A great tool");
        assert_eq!(summary.version_count, 1);
        assert_eq!(summary.available_versions, vec!["1.0.0"]);
        assert!(summary.categories.is_empty());
        assert!(summary.tags.is_empty());
        assert!(summary.source_label.is_none());
    }

    #[test]
    fn summary_from_entry_with_classification() {
        let mut version = make_version("2.0.0", "classified", false);
        version.metadata.skill.classification = Some(Classification {
            categories: vec!["database".into()],
            tags: vec!["redis".into(), "caching".into()],
        });
        let entry = make_entry("acme", "redis", vec![version]);
        let summary = SkillSummary::from_entry(&entry).unwrap();
        assert_eq!(summary.categories, vec!["database"]);
        assert_eq!(summary.tags, vec!["redis", "caching"]);
    }

    #[test]
    fn summary_from_entry_files_sorted() {
        let mut version = make_version("1.0.0", "with files", false);
        version.files.insert(
            "scripts/lint.sh".into(),
            SkillFile {
                content: "#!/bin/bash".into(),
                mime_type: "text/x-shellscript".into(),
            },
        );
        version.files.insert(
            "references/guide.md".into(),
            SkillFile {
                content: "# Guide".into(),
                mime_type: "text/markdown".into(),
            },
        );
        let entry = make_entry("acme", "tool", vec![version]);
        let summary = SkillSummary::from_entry(&entry).unwrap();
        assert_eq!(
            summary.files,
            vec!["references/guide.md", "scripts/lint.sh"]
        );
    }

    #[test]
    fn summary_from_entry_yanked_excluded_from_available() {
        let entry = make_entry(
            "acme",
            "tool",
            vec![
                make_version("0.1.0", "old", false),
                make_version("0.2.0", "yanked", true),
                make_version("0.3.0", "latest", false),
            ],
        );
        let summary = SkillSummary::from_entry(&entry).unwrap();
        assert_eq!(summary.version, "0.3.0");
        assert_eq!(summary.version_count, 3);
        assert_eq!(summary.available_versions, vec!["0.1.0", "0.3.0"]);
    }

    #[test]
    fn summary_from_entry_none_when_all_yanked() {
        let entry = make_entry("acme", "tool", vec![make_version("0.1.0", "yanked", true)]);
        assert!(SkillSummary::from_entry(&entry).is_none());
    }

    #[test]
    fn summary_integrity_verified() {
        let mut version = make_version("1.0.0", "verified", false);
        version.integrity_ok = Some(true);
        let entry = make_entry("acme", "tool", vec![version]);
        let summary = SkillSummary::from_entry(&entry).unwrap();
        assert_eq!(summary.integrity, Some("verified".into()));
    }

    #[test]
    fn summary_integrity_failed() {
        let mut version = make_version("1.0.0", "bad", false);
        version.integrity_ok = Some(false);
        let entry = make_entry("acme", "tool", vec![version]);
        let summary = SkillSummary::from_entry(&entry).unwrap();
        assert_eq!(summary.integrity, Some("failed".into()));
    }

    #[test]
    fn summary_source_label_for_local() {
        let mut entry = make_entry(
            "acme",
            "tool",
            vec![make_version("1.0.0", "local skill", false)],
        );
        entry.source = SkillSource::Local {
            platform: "claude".into(),
            path: PathBuf::from("/tmp/skills/tool"),
        };
        let summary = SkillSummary::from_entry(&entry).unwrap();
        assert_eq!(summary.source_label, Some("local (claude)".into()));
    }

    // -- RegistryConfig default --

    #[test]
    fn registry_config_default() {
        let config = RegistryConfig::default();
        assert_eq!(config.registry.name, "skillet");
        assert_eq!(config.registry.version, 1);
        assert!(config.registry.description.is_none());
        assert!(config.registry.maintainer.is_none());
    }
}
