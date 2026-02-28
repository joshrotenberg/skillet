//! Repo catalog: curated external skill repositories.
//!
//! A `repos.toml` file in a registry root maps short names (`anthropics/skills`)
//! to `{url, subdir}` pairs. The CLI and MCP server resolve these short names
//! so users don't need to remember full git URLs.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A single entry in the repo catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    /// Short name in `owner/repo` format (e.g. `anthropics/skills`).
    pub name: String,
    /// Git clone URL.
    pub url: String,
    /// Subdirectory within the repo that contains skills.
    /// `None` means skills are at the repo root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Topic domains covered by this repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domains: Option<Vec<String>>,
}

/// Deserialized `repos.toml` file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoCatalogFile {
    /// The list of repo entries.
    #[serde(default)]
    pub repo: Vec<RepoEntry>,
}

/// In-memory catalog of curated repos.
#[derive(Debug, Clone, Default)]
pub struct RepoCatalog {
    entries: Vec<RepoEntry>,
}

impl RepoCatalog {
    /// Look up a repo by short name (case-insensitive).
    pub fn find(&self, name: &str) -> Option<&RepoEntry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// All entries in the catalog.
    pub fn entries(&self) -> &[RepoEntry] {
        &self.entries
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Load the repo catalog from a registry root directory.
///
/// Looks for `repos.toml` at `registry_path/repos.toml`. Returns an empty
/// catalog if the file is absent. Errors if the file exists but is malformed.
pub fn load_repos_catalog(registry_path: &Path) -> crate::error::Result<RepoCatalog> {
    let path = registry_path.join("repos.toml");
    if !path.is_file() {
        // Also check parent directory -- if registry_path includes a subdir
        // (e.g. `<cache>/joshrotenberg_skillet/registry`), look in the repo root.
        if let Some(parent) = registry_path.parent() {
            let parent_path = parent.join("repos.toml");
            if parent_path.is_file() {
                return load_repos_catalog_from(&parent_path);
            }
        }
        return Ok(RepoCatalog::default());
    }
    load_repos_catalog_from(&path)
}

fn load_repos_catalog_from(path: &Path) -> crate::error::Result<RepoCatalog> {
    let raw = std::fs::read_to_string(path).map_err(|e| crate::error::Error::ConfigRead {
        path: path.to_path_buf(),
        source: e,
    })?;
    let file: RepoCatalogFile =
        toml::from_str(&raw).map_err(|e| crate::error::Error::ConfigParse {
            path: path.to_path_buf(),
            source: e,
        })?;
    Ok(RepoCatalog { entries: file.repo })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_empty_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = load_repos_catalog(tmp.path()).unwrap();
        assert!(catalog.is_empty());
    }

    #[test]
    fn load_catalog_from_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("repos.toml"),
            r#"
[[repo]]
name = "test/repo"
url = "https://github.com/test/repo.git"
subdir = "skills"
description = "Test repo"
domains = ["testing"]
"#,
        )
        .unwrap();

        let catalog = load_repos_catalog(tmp.path()).unwrap();
        assert_eq!(catalog.len(), 1);
        let entry = catalog.find("test/repo").unwrap();
        assert_eq!(entry.url, "https://github.com/test/repo.git");
        assert_eq!(entry.subdir.as_deref(), Some("skills"));
        assert_eq!(entry.description.as_deref(), Some("Test repo"));
        assert_eq!(
            entry.domains.as_deref(),
            Some(vec!["testing".to_string()].as_slice())
        );
    }

    #[test]
    fn find_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("repos.toml"),
            r#"
[[repo]]
name = "Acme/Skills"
url = "https://github.com/acme/skills.git"
"#,
        )
        .unwrap();

        let catalog = load_repos_catalog(tmp.path()).unwrap();
        assert!(catalog.find("acme/skills").is_some());
        assert!(catalog.find("ACME/SKILLS").is_some());
        assert!(catalog.find("nonexistent").is_none());
    }

    #[test]
    fn load_no_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("repos.toml"),
            r#"
[[repo]]
name = "owner/repo"
url = "https://github.com/owner/repo.git"
"#,
        )
        .unwrap();

        let catalog = load_repos_catalog(tmp.path()).unwrap();
        let entry = catalog.find("owner/repo").unwrap();
        assert!(entry.subdir.is_none());
    }

    #[test]
    fn load_malformed_errors() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("repos.toml"), "not valid toml {{{").unwrap();
        assert!(load_repos_catalog(tmp.path()).is_err());
    }

    #[test]
    fn load_from_parent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let subdir = tmp.path().join("registry");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(
            tmp.path().join("repos.toml"),
            r#"
[[repo]]
name = "parent/repo"
url = "https://github.com/parent/repo.git"
"#,
        )
        .unwrap();

        let catalog = load_repos_catalog(&subdir).unwrap();
        assert_eq!(catalog.len(), 1);
        assert!(catalog.find("parent/repo").is_some());
    }
}
