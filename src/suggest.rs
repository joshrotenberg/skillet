//! Suggest graph walker with safety limits, URL canonicalization, and trust tiers.
//!
//! The `[[suggest]]` mechanism in `skillet.toml` lets repos point to other repos,
//! forming a decentralized discovery graph. This module walks that graph with
//! configurable safety limits (fan-out, total repos, clone timeout, negative caching)
//! and stamps each discovered skill with a hop-based trust tier.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::cache::{self, RepoSource};
use crate::config::SuggestConfig;
use crate::state::{SkillIndex, TrustTier};
use crate::{git, index, project};

/// Normalize a git URL for deduplication.
///
/// - Converts `git@host:owner/repo.git` to `https://host/owner/repo`
/// - Strips trailing `.git`
/// - Lowercases the host
/// - Strips trailing slashes
pub fn canonicalize_url(url: &str) -> String {
    let url = url.trim();

    // Convert SSH-style URLs: git@github.com:owner/repo -> https://github.com/owner/repo
    let url = if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            format!("https://{host}/{path}")
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    };

    // Strip trailing .git
    let url = url.strip_suffix(".git").unwrap_or(&url).to_string();

    // Strip trailing slashes
    let url = url.trim_end_matches('/').to_string();

    // Lowercase the host portion
    if let Some(rest) = url.strip_prefix("https://") {
        if let Some(slash) = rest.find('/') {
            let (host, path) = rest.split_at(slash);
            format!("https://{}{path}", host.to_lowercase())
        } else {
            format!("https://{}", rest.to_lowercase())
        }
    } else if let Some(rest) = url.strip_prefix("http://") {
        if let Some(slash) = rest.find('/') {
            let (host, path) = rest.split_at(slash);
            format!("http://{}{path}", host.to_lowercase())
        } else {
            format!("http://{}", rest.to_lowercase())
        }
    } else {
        url
    }
}

/// Check if a URL matches any pattern in a list.
///
/// Patterns support simple prefix matching with `*` as a wildcard suffix.
/// Examples: `github.com/tokio-rs/*`, `github.com/*`, `*.example.com`.
fn url_matches_patterns(url: &str, patterns: &[String]) -> bool {
    let canonical = canonicalize_url(url);
    // Strip scheme for matching
    let bare = canonical
        .strip_prefix("https://")
        .or_else(|| canonical.strip_prefix("http://"))
        .unwrap_or(&canonical);

    patterns.iter().any(|pattern| {
        let pattern = pattern
            .strip_prefix("https://")
            .or_else(|| pattern.strip_prefix("http://"))
            .unwrap_or(pattern);

        if let Some(prefix) = pattern.strip_suffix('*') {
            bare.starts_with(prefix)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            bare.ends_with(suffix)
        } else {
            bare == pattern
        }
    })
}

/// In-memory cache of URLs that failed to clone.
struct NegativeCache {
    failures: HashMap<String, Instant>,
    ttl: Duration,
}

impl NegativeCache {
    fn new(ttl: Duration) -> Self {
        Self {
            failures: HashMap::new(),
            ttl,
        }
    }

    /// Returns true if this URL failed recently and should be skipped.
    fn is_blocked(&self, canonical_url: &str) -> bool {
        if let Some(failed_at) = self.failures.get(canonical_url) {
            failed_at.elapsed() < self.ttl
        } else {
            false
        }
    }

    /// Record a clone failure for this URL.
    fn record_failure(&mut self, canonical_url: &str) {
        self.failures
            .insert(canonical_url.to_string(), Instant::now());
    }
}

/// BFS walker for the `[[suggest]]` graph with safety limits.
pub struct SuggestWalker {
    config: SuggestConfig,
    cache_base: PathBuf,
    cache_enabled: bool,
    cache_ttl: Duration,
    visited: HashSet<String>,
    negative_cache: NegativeCache,
    total_cloned: usize,
}

impl SuggestWalker {
    /// Create a new walker, seeding the visited set with explicitly configured URLs.
    pub fn new(
        config: &SuggestConfig,
        cache_base: &Path,
        cache_enabled: bool,
        cache_ttl: Duration,
        seed_urls: &[String],
    ) -> Self {
        let neg_ttl = crate::repo::parse_duration(&config.negative_cache_ttl)
            .unwrap_or(Duration::from_secs(3600));

        let visited: HashSet<String> = seed_urls.iter().map(|u| canonicalize_url(u)).collect();

        Self {
            config: config.clone(),
            cache_base: cache_base.to_path_buf(),
            cache_enabled,
            cache_ttl,
            visited,
            negative_cache: NegativeCache::new(neg_ttl),
            total_cloned: 0,
        }
    }

    /// Walk the suggest graph starting from the given repo paths.
    ///
    /// Discovers `[[suggest]]` entries in each repo's `skillet.toml`, clones them,
    /// indexes their skills, and merges into `merged`. Recurses up to `depth` levels.
    ///
    /// Each discovered skill is stamped with the appropriate `TrustTier` and
    /// `discovered_via` provenance chain.
    pub fn walk(
        &mut self,
        repo_paths: &[PathBuf],
        merged: &mut SkillIndex,
        all_paths: &mut Vec<PathBuf>,
        depth: u32,
        provenance: Vec<String>,
    ) {
        if depth == 0 || !self.config.enabled {
            return;
        }

        if self.total_cloned >= self.config.max_repos {
            tracing::info!(
                max = self.config.max_repos,
                "Suggest graph: total repo cap reached"
            );
            return;
        }

        let trust_tier = match depth {
            d if d >= self.config.max_depth => TrustTier::Transitive,
            d if d == self.config.max_depth - 1 => TrustTier::Transitive,
            _ => TrustTier::Suggested,
        };

        let clone_timeout = crate::repo::parse_duration(&self.config.clone_timeout)
            .unwrap_or(Duration::from_secs(30));

        let mut new_suggestions = Vec::new();

        for path in repo_paths {
            let root = find_repo_root(path).or_else(|| path.parent().and_then(find_repo_root));

            let root = match root {
                Some(r) => r,
                None => continue,
            };

            let manifest = match project::load_skillet_toml(&root) {
                Ok(Some(m)) => m,
                _ => continue,
            };

            let mut followed_from_this_repo = 0;

            for entry in &manifest.suggest {
                // Per-repo fan-out limit
                if followed_from_this_repo >= self.config.max_per_repo {
                    tracing::debug!(
                        repo = %root.display(),
                        max = self.config.max_per_repo,
                        "Suggest graph: per-repo fan-out limit reached"
                    );
                    break;
                }

                // Total repo cap
                if self.total_cloned >= self.config.max_repos {
                    tracing::info!(
                        max = self.config.max_repos,
                        "Suggest graph: total repo cap reached"
                    );
                    return;
                }

                if entry.url.is_empty() {
                    continue;
                }

                let canonical = canonicalize_url(&entry.url);

                // Already visited?
                if self.visited.contains(&canonical) {
                    continue;
                }
                self.visited.insert(canonical.clone());

                // Negative cache: skip recently failed URLs
                if self.negative_cache.is_blocked(&canonical) {
                    tracing::debug!(url = %entry.url, "Skipping negatively cached URL");
                    continue;
                }

                // Blocklist check
                if !self.config.block.is_empty()
                    && url_matches_patterns(&entry.url, &self.config.block)
                {
                    tracing::debug!(url = %entry.url, "Blocked by suggest blocklist");
                    continue;
                }

                // Allowlist check (if configured, URL must match)
                if !self.config.allow.is_empty()
                    && !url_matches_patterns(&entry.url, &self.config.allow)
                {
                    tracing::debug!(url = %entry.url, "Not in suggest allowlist, skipping");
                    continue;
                }

                tracing::debug!(
                    url = %entry.url,
                    description = entry.description.as_deref().unwrap_or(""),
                    depth,
                    "Following suggestion"
                );

                let target = crate::repo::cache_dir_for_url(&self.cache_base, &entry.url);
                if let Some(parent) = target.parent()
                    && let Err(e) = std::fs::create_dir_all(parent)
                {
                    tracing::warn!(url = %entry.url, error = %e, "Failed to create cache dir");
                    continue;
                }

                if let Err(e) = git::clone_or_pull_with_timeout(&entry.url, &target, clone_timeout)
                {
                    tracing::warn!(url = %entry.url, error = %e, "Failed to clone suggested repo");
                    self.negative_cache.record_failure(&canonical);
                    continue;
                }

                self.total_cloned += 1;
                followed_from_this_repo += 1;

                let skill_path = match &entry.subdir {
                    Some(sub) => target.join(sub),
                    None => target.clone(),
                };

                let source = RepoSource::Remote {
                    url: entry.url.clone(),
                    checkout: target,
                };

                let mut entry_provenance = provenance.clone();
                entry_provenance.push(entry.url.clone());

                if self.cache_enabled
                    && let Some(mut idx) = cache::load(&source, self.cache_ttl)
                {
                    stamp_trust(&mut idx, &trust_tier, &entry_provenance);
                    merged.merge(idx);
                    all_paths.push(skill_path.clone());
                    new_suggestions.push(skill_path);
                    continue;
                }

                match index::load_index(&skill_path) {
                    Ok(mut idx) => {
                        if self.cache_enabled {
                            cache::write(&source, &idx);
                        }
                        stamp_trust(&mut idx, &trust_tier, &entry_provenance);
                        merged.merge(idx);
                        all_paths.push(skill_path.clone());
                        new_suggestions.push(skill_path);
                    }
                    Err(e) => {
                        tracing::warn!(url = %entry.url, error = %e, "Failed to index suggested repo");
                        self.negative_cache.record_failure(&canonical);
                    }
                }
            }
        }

        // Recurse for the newly added repos
        if !new_suggestions.is_empty() && depth > 1 {
            self.walk(&new_suggestions, merged, all_paths, depth - 1, provenance);
        }
    }
}

/// Stamp trust tier and provenance on all entries in an index.
fn stamp_trust(index: &mut SkillIndex, tier: &TrustTier, provenance: &[String]) {
    for entry in index.skills.values_mut() {
        entry.trust_tier = tier.clone();
        entry.discovered_via = provenance.to_vec();
    }
}

/// Walk up from a skills subdirectory to find the repo root containing `skillet.toml`.
fn find_repo_root(skill_path: &Path) -> Option<PathBuf> {
    let mut current = skill_path.to_path_buf();
    for _ in 0..5 {
        if current.join("skillet.toml").is_file() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- canonicalize_url --

    #[test]
    fn canonical_https_with_git_suffix() {
        assert_eq!(
            canonicalize_url("https://github.com/owner/repo.git"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn canonical_https_without_git_suffix() {
        assert_eq!(
            canonicalize_url("https://github.com/owner/repo"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn canonical_ssh_to_https() {
        assert_eq!(
            canonicalize_url("git@github.com:owner/repo.git"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn canonical_ssh_no_git_suffix() {
        assert_eq!(
            canonicalize_url("git@github.com:owner/repo"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn canonical_lowercases_host() {
        assert_eq!(
            canonicalize_url("https://GitHub.COM/Owner/Repo.git"),
            "https://github.com/Owner/Repo"
        );
    }

    #[test]
    fn canonical_strips_trailing_slashes() {
        assert_eq!(
            canonicalize_url("https://github.com/owner/repo/"),
            "https://github.com/owner/repo"
        );
    }

    #[test]
    fn canonical_trims_whitespace() {
        assert_eq!(
            canonicalize_url("  https://github.com/owner/repo.git  "),
            "https://github.com/owner/repo"
        );
    }

    // -- url_matches_patterns --

    #[test]
    fn pattern_prefix_wildcard() {
        assert!(url_matches_patterns(
            "https://github.com/tokio-rs/skills.git",
            &["github.com/tokio-rs/*".to_string()]
        ));
    }

    #[test]
    fn pattern_no_match() {
        assert!(!url_matches_patterns(
            "https://github.com/other/repo.git",
            &["github.com/tokio-rs/*".to_string()]
        ));
    }

    #[test]
    fn pattern_exact_match() {
        assert!(url_matches_patterns(
            "https://github.com/owner/repo.git",
            &["github.com/owner/repo".to_string()]
        ));
    }

    #[test]
    fn pattern_suffix_wildcard() {
        assert!(url_matches_patterns(
            "https://skills.example.com/repo",
            &["*.example.com/repo".to_string()]
        ));
    }

    // -- NegativeCache --

    #[test]
    fn negative_cache_blocks_recent_failure() {
        let mut cache = NegativeCache::new(Duration::from_secs(3600));
        let url = "https://github.com/broken/repo";
        cache.record_failure(url);
        assert!(cache.is_blocked(url));
    }

    #[test]
    fn negative_cache_allows_unknown_url() {
        let cache = NegativeCache::new(Duration::from_secs(3600));
        assert!(!cache.is_blocked("https://github.com/unknown/repo"));
    }

    #[test]
    fn negative_cache_expires() {
        let mut cache = NegativeCache::new(Duration::from_millis(1));
        cache.record_failure("https://github.com/broken/repo");
        std::thread::sleep(Duration::from_millis(10));
        assert!(!cache.is_blocked("https://github.com/broken/repo"));
    }

    // -- find_repo_root --

    #[test]
    fn find_repo_root_at_root() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("skillet.toml"), "[project]\n").unwrap();
        let found = find_repo_root(tmp.path());
        assert_eq!(found, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn find_repo_root_from_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("skillet.toml"), "[project]\n").unwrap();
        let skills = tmp.path().join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        let found = find_repo_root(&skills);
        assert_eq!(found, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn find_repo_root_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let found = find_repo_root(tmp.path());
        assert!(found.is_none());
    }

    // -- SuggestWalker construction --

    #[test]
    fn walker_seeds_visited_with_canonical_urls() {
        let config = SuggestConfig::default();
        let walker = SuggestWalker::new(
            &config,
            Path::new("/tmp"),
            false,
            Duration::ZERO,
            &[
                "https://github.com/a/b.git".to_string(),
                "git@github.com:c/d.git".to_string(),
            ],
        );

        assert!(walker.visited.contains("https://github.com/a/b"));
        assert!(walker.visited.contains("https://github.com/c/d"));
    }

    // -- Aliased URLs dedup --

    #[test]
    fn canonical_deduplicates_aliases() {
        let urls = [
            "https://github.com/org/repo",
            "https://github.com/org/repo.git",
            "git@github.com:org/repo.git",
            "https://GitHub.COM/org/repo.git",
            "https://github.com/org/repo/",
        ];
        let canonical: HashSet<String> = urls.iter().map(|u| canonicalize_url(u)).collect();
        assert_eq!(
            canonical.len(),
            1,
            "all aliases should canonicalize to the same URL"
        );
        assert!(canonical.contains("https://github.com/org/repo"));
    }
}
