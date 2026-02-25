//! Shared application state and data models

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Shared state for the MCP server
pub struct AppState {
    /// In-memory skill index, refreshable
    pub index: RwLock<SkillIndex>,
    /// Path to the registry root (git checkout)
    pub registry_path: PathBuf,
}

impl AppState {
    pub fn new(registry_path: PathBuf, index: SkillIndex) -> Arc<Self> {
        Arc::new(Self {
            index: RwLock::new(index),
            registry_path,
        })
    }
}

/// In-memory index of all skills in the registry
#[derive(Debug, Default)]
pub struct SkillIndex {
    /// All skills keyed by (owner, name)
    pub skills: HashMap<(String, String), SkillEntry>,
    /// All known categories with skill counts
    pub categories: BTreeMap<String, usize>,
}

/// A skill with all its versions
#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub owner: String,
    pub name: String,
    pub versions: Vec<SkillVersion>,
}

impl SkillEntry {
    /// The latest (most recently added) non-yanked version
    pub fn latest(&self) -> Option<&SkillVersion> {
        self.versions.iter().rev().find(|v| !v.yanked)
    }
}

/// A single published version of a skill
#[derive(Debug, Clone)]
pub struct SkillVersion {
    pub version: String,
    pub metadata: SkillMetadata,
    pub skill_md: String,
    pub skill_toml_raw: String,
    pub yanked: bool,
    /// Extra files in the skillpack (scripts/, references/, assets/)
    /// Keyed by relative path from skill root (e.g. "scripts/lint.sh")
    pub files: HashMap<String, SkillFile>,
}

/// An extra file in a skillpack
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compatibility {
    #[serde(default)]
    pub requires_tool_use: Option<bool>,
    #[serde(default)]
    pub requires_vision: Option<bool>,
    #[serde(default)]
    pub min_context_tokens: Option<u64>,
    #[serde(default)]
    pub required_tools: Vec<String>,
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
}

impl SkillSummary {
    pub fn from_entry(entry: &SkillEntry) -> Option<Self> {
        let v = entry.latest()?;
        let info = &v.metadata.skill;
        let classification = info.classification.as_ref();
        let compat = info.compatibility.as_ref();
        let mut files: Vec<String> = v.files.keys().cloned().collect();
        files.sort();
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
        })
    }
}
