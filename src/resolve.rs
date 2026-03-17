//! Release model resolution for skill repos.
//!
//! Determines which git ref to checkout for a repo based on:
//! 1. Consumer-side pin (`[[source]]` in config.toml) -- highest priority
//! 2. Author-side preference (`[source]` in skillet.toml)
//! 3. Auto-detection: latest release tag if available, otherwise default branch
//!
//! After clone/pull, call `resolve_and_checkout` to switch to the correct ref.

use std::path::Path;

use crate::config::SourcePin;
use crate::git;
use crate::project;
use crate::suggest::canonicalize_url;

/// Resolve which git ref to use and checkout that ref.
///
/// Priority:
/// 1. Consumer pin from config (exact match on canonical URL)
/// 2. Author preference from `[source].prefer` in skillet.toml
/// 3. Auto-detect: latest release tag, or stay on default branch
///
/// Returns the ref that was checked out, or `None` if staying on default branch.
pub fn resolve_and_checkout(
    repo_path: &Path,
    url: &str,
    consumer_pins: &[SourcePin],
) -> crate::error::Result<Option<String>> {
    // 1. Check consumer-side pin
    if let Some(pin) = find_consumer_pin(url, consumer_pins) {
        tracing::info!(url, version = %pin, "Using consumer-pinned version");
        git::fetch_tags(repo_path)?;
        git::checkout(repo_path, &pin)?;
        return Ok(Some(pin));
    }

    // 2. Check author-side preference
    let author_prefer = load_author_preference(repo_path);

    match author_prefer.as_deref() {
        Some("main") => {
            tracing::debug!(url, "Author prefers main, staying on default branch");
            Ok(None)
        }
        Some(pref) if pref.starts_with("tag:") => {
            let pattern = &pref[4..];
            tracing::debug!(url, pattern, "Author prefers tag pattern");
            git::fetch_tags(repo_path)?;
            let tags = git::list_tags(repo_path)?;
            if let Some(tag) = find_matching_tag(&tags, pattern) {
                tracing::info!(url, tag = %tag, "Checking out author-preferred tag");
                git::checkout(repo_path, &tag)?;
                Ok(Some(tag))
            } else {
                tracing::debug!(
                    url,
                    pattern,
                    "No tags match pattern, staying on default branch"
                );
                Ok(None)
            }
        }
        // "release" or None (default) -- auto-detect
        _ => auto_detect(repo_path, url),
    }
}

/// Auto-detect: if the repo has release-style tags (vX.Y.Z), checkout the latest one.
fn auto_detect(repo_path: &Path, url: &str) -> crate::error::Result<Option<String>> {
    // Fetch tags (shallow clones don't have them by default)
    if let Err(e) = git::fetch_tags(repo_path) {
        tracing::debug!(url, error = %e, "Failed to fetch tags, staying on default branch");
        return Ok(None);
    }

    let tags = git::list_tags(repo_path)?;
    if tags.is_empty() {
        tracing::debug!(url, "No tags found, staying on default branch");
        return Ok(None);
    }

    // Look for the latest semver-style tag (v1.2.3 or 1.2.3)
    let release_tag = tags.iter().rev().find(|t| is_release_tag(t));

    if let Some(tag) = release_tag {
        tracing::info!(url, tag = %tag, "Auto-detected latest release tag");
        git::checkout(repo_path, tag)?;
        Ok(Some(tag.clone()))
    } else {
        tracing::debug!(
            url,
            "No release-style tags found, staying on default branch"
        );
        Ok(None)
    }
}

/// Check if a tag looks like a release version (v1.2.3, 1.2.3, 2026.01.01, etc).
fn is_release_tag(tag: &str) -> bool {
    let s = tag.strip_prefix('v').unwrap_or(tag);
    // Must start with a digit and contain at least one dot
    s.starts_with(|c: char| c.is_ascii_digit()) && s.contains('.')
}

/// Find a consumer-side pin for a URL.
fn find_consumer_pin(url: &str, pins: &[SourcePin]) -> Option<String> {
    let canonical = canonicalize_url(url);
    // Strip scheme for matching (pins use bare "github.com/owner/repo")
    let bare = canonical
        .strip_prefix("https://")
        .or_else(|| canonical.strip_prefix("http://"))
        .unwrap_or(&canonical);

    pins.iter()
        .find(|p| {
            let pin_bare = p
                .repo
                .strip_prefix("https://")
                .or_else(|| p.repo.strip_prefix("http://"))
                .unwrap_or(&p.repo);
            pin_bare == bare
        })
        .and_then(|p| p.version.clone())
}

/// Load author preference from the repo's skillet.toml.
fn load_author_preference(repo_path: &Path) -> Option<String> {
    let manifest = project::load_skillet_toml(repo_path).ok()??;
    manifest.source.map(|s| s.prefer)
}

/// Find the last tag matching a glob pattern (e.g. "v*", "v2.*").
fn find_matching_tag(tags: &[String], pattern: &str) -> Option<String> {
    if let Some(prefix) = pattern.strip_suffix('*') {
        tags.iter().rev().find(|t| t.starts_with(prefix)).cloned()
    } else {
        // Exact match
        tags.iter().find(|t| t.as_str() == pattern).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SourcePin;

    #[test]
    fn is_release_tag_semver() {
        assert!(is_release_tag("v1.0.0"));
        assert!(is_release_tag("v0.2.3"));
        assert!(is_release_tag("1.0.0"));
        assert!(is_release_tag("2026.01.01"));
    }

    #[test]
    fn is_release_tag_non_release() {
        assert!(!is_release_tag("latest"));
        assert!(!is_release_tag("nightly"));
        assert!(!is_release_tag("main"));
        assert!(!is_release_tag("v")); // no digits after v
    }

    #[test]
    fn is_release_tag_no_dot() {
        assert!(!is_release_tag("v1")); // needs a dot
        assert!(!is_release_tag("123")); // no dot
    }

    #[test]
    fn consumer_pin_matches_canonical() {
        let pins = vec![SourcePin {
            repo: "github.com/owner/repo".to_string(),
            version: Some("v1.0.0".to_string()),
        }];

        // HTTPS with .git
        assert_eq!(
            find_consumer_pin("https://github.com/owner/repo.git", &pins),
            Some("v1.0.0".to_string())
        );
        // SSH
        assert_eq!(
            find_consumer_pin("git@github.com:owner/repo.git", &pins),
            Some("v1.0.0".to_string())
        );
        // No match
        assert_eq!(
            find_consumer_pin("https://github.com/other/repo.git", &pins),
            None
        );
    }

    #[test]
    fn consumer_pin_no_version_returns_none() {
        let pins = vec![SourcePin {
            repo: "github.com/owner/repo".to_string(),
            version: None,
        }];
        assert_eq!(
            find_consumer_pin("https://github.com/owner/repo.git", &pins),
            None
        );
    }

    #[test]
    fn find_matching_tag_glob() {
        let tags = vec![
            "v0.1.0".to_string(),
            "v0.2.0".to_string(),
            "v1.0.0".to_string(),
        ];
        assert_eq!(find_matching_tag(&tags, "v*"), Some("v1.0.0".to_string()));
        assert_eq!(find_matching_tag(&tags, "v0.*"), Some("v0.2.0".to_string()));
    }

    #[test]
    fn find_matching_tag_exact() {
        let tags = vec!["v1.0.0".to_string(), "v2.0.0".to_string()];
        assert_eq!(
            find_matching_tag(&tags, "v1.0.0"),
            Some("v1.0.0".to_string())
        );
        assert_eq!(find_matching_tag(&tags, "v3.0.0"), None);
    }

    #[test]
    fn find_matching_tag_no_match() {
        let tags = vec!["v1.0.0".to_string()];
        assert_eq!(find_matching_tag(&tags, "v2.*"), None);
    }

    // Integration tests with actual git repos

    #[test]
    fn resolve_repo_without_tags_stays_on_default() {
        let repo = crate::git::tests::make_repo_with_commit_pub();
        let result = resolve_and_checkout(repo.path(), "file:///test", &[]).unwrap();
        assert_eq!(result, None, "repo without tags should stay on default");
    }

    #[test]
    fn resolve_repo_with_release_tag() {
        let repo = crate::git::tests::make_repo_with_commit_pub();
        // Create a release tag
        std::process::Command::new("git")
            .args(["-C", &repo.path().display().to_string(), "tag", "v1.0.0"])
            .output()
            .unwrap();

        let result = resolve_and_checkout(repo.path(), "file:///test", &[]).unwrap();
        assert_eq!(result, Some("v1.0.0".to_string()));
    }

    #[test]
    fn resolve_consumer_pin_overrides_tag() {
        let repo = crate::git::tests::make_repo_with_commit_pub();
        // Create two tags
        std::process::Command::new("git")
            .args(["-C", &repo.path().display().to_string(), "tag", "v1.0.0"])
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["-C", &repo.path().display().to_string(), "tag", "v2.0.0"])
            .output()
            .unwrap();

        let pins = vec![SourcePin {
            repo: "test".to_string(),
            version: Some("v1.0.0".to_string()),
        }];
        // URL matches the pin's repo when canonicalized
        let result = resolve_and_checkout(repo.path(), "https://test", &pins).unwrap();
        assert_eq!(result, Some("v1.0.0".to_string()));
    }

    #[test]
    fn resolve_author_prefers_main() {
        let repo = crate::git::tests::make_repo_with_commit_pub();
        // Create a tag
        std::process::Command::new("git")
            .args(["-C", &repo.path().display().to_string(), "tag", "v1.0.0"])
            .output()
            .unwrap();
        // Write skillet.toml with [source] prefer = "main"
        std::fs::write(
            repo.path().join("skillet.toml"),
            "[source]\nprefer = \"main\"\n",
        )
        .unwrap();

        let result = resolve_and_checkout(repo.path(), "file:///test", &[]).unwrap();
        assert_eq!(result, None, "author prefers main, should stay on default");
    }
}
