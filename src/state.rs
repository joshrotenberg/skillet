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
    /// Registry configuration (from config.toml or defaults)
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

/// Top-level registry configuration, parsed from `config.toml`.
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
}

impl SkillSource {
    /// Returns the platform label for local skills, or `None` for registry skills.
    pub fn label(&self) -> Option<String> {
        match self {
            Self::Registry => None,
            Self::Local { platform, .. } => Some(format!("local ({platform})")),
        }
    }

    /// Returns the on-disk path for local skills.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Registry => None,
            Self::Local { path, .. } => Some(path),
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
