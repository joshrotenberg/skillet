//! Discover skill repos via GitHub code search.
//!
//! Searches GitHub for repos containing `skillet.toml` files,
//! indicating they've opted into the skillet ecosystem.

use std::process::Command;

use serde::Deserialize;

/// A discovered skill repo from GitHub search.
#[derive(Debug, Clone)]
pub struct DiscoveredRepo {
    /// Full repo name (e.g. "owner/repo")
    pub full_name: String,
    /// Repo description
    pub description: String,
    /// Clone URL
    pub clone_url: String,
}

/// Search result from GitHub code search API.
#[derive(Debug, Deserialize)]
struct SearchResult {
    items: Vec<SearchItem>,
    total_count: u64,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    repository: RepoInfo,
}

#[derive(Debug, Deserialize)]
struct RepoInfo {
    full_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    private: bool,
}

/// Search GitHub for repos containing `skillet.toml`.
///
/// Uses the `gh` CLI for authentication. Returns an error if `gh` is not
/// installed or not authenticated.
pub fn search_github(query: Option<&str>) -> crate::error::Result<Vec<DiscoveredRepo>> {
    let search_query = if let Some(q) = query {
        format!("filename:skillet.toml {q}")
    } else {
        "filename:skillet.toml".to_string()
    };

    let output = Command::new("gh")
        .args([
            "api",
            "search/code",
            "-X",
            "GET",
            "-f",
            &format!("q={search_query}"),
            "-f",
            "per_page=30",
        ])
        .output()
        .map_err(|e| crate::error::Error::Io {
            context: "failed to run gh CLI (is it installed?)".to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(crate::error::Error::Other(format!(
            "GitHub search failed: {stderr}"
        )));
    }

    let result: SearchResult = serde_json::from_slice(&output.stdout)
        .map_err(|e| crate::error::Error::Other(format!("Failed to parse search results: {e}")))?;

    // Deduplicate by repo and skip private repos
    let mut seen = std::collections::HashSet::new();
    let repos: Vec<DiscoveredRepo> = result
        .items
        .into_iter()
        .filter_map(|item| {
            if item.repository.private {
                return None;
            }
            let name = item.repository.full_name.clone();
            if seen.insert(name) {
                let clone_url = format!("https://github.com/{}.git", item.repository.full_name);
                Some(DiscoveredRepo {
                    full_name: item.repository.full_name,
                    description: item.repository.description.unwrap_or_default(),
                    clone_url,
                })
            } else {
                None
            }
        })
        .collect();

    tracing::info!(
        total = result.total_count,
        returned = repos.len(),
        "GitHub search for skillet.toml"
    );

    Ok(repos)
}
