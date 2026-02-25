//! Skillet MCP Server
//!
//! An MCP-native skill registry for AI agents. Serves skills from a local
//! registry directory (git checkout) via tools and resource templates.

mod bm25;
mod git;
mod index;
mod integrity;
mod resources;
mod search;
mod state;
mod tools;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use tower_mcp::{McpRouter, StdioTransport};

use crate::state::AppState;

#[derive(Parser, Debug)]
#[command(name = "skillet")]
#[command(about = "MCP-native skill registry for AI agents")]
struct Args {
    /// Path to a local registry directory (contains owner/skill-name/ directories)
    #[arg(long, group = "source")]
    registry: Option<PathBuf>,

    /// Git URL to clone/pull the registry from
    #[arg(long, group = "source")]
    remote: Option<String>,

    /// How often to pull from the remote (e.g. "5m", "1h", "0" to disable).
    /// Only used with --remote.
    #[arg(long, default_value = "5m")]
    refresh_interval: String,

    /// Directory to clone remote registries into
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Subdirectory within the registry (local or remote) that contains the skills
    #[arg(long)]
    subdir: Option<PathBuf>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

/// Parse a human-friendly duration string like "5m", "1h", "30s", or "0".
fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let s = s.trim();
    if s == "0" {
        return Ok(Duration::ZERO);
    }

    let (num, suffix) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let num: u64 = num
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid duration number: {s}"))?;

    let secs = match suffix {
        "s" | "" => num,
        "m" => num * 60,
        "h" => num * 3600,
        _ => anyhow::bail!("Unknown duration suffix: {suffix} (use s, m, or h)"),
    };

    Ok(Duration::from_secs(secs))
}

/// Derive a cache directory from the remote URL.
///
/// Turns `https://github.com/owner/repo.git` into `<base>/owner_repo`.
fn cache_dir_for_url(base: &Path, url: &str) -> PathBuf {
    let slug: String = url
        .trim_end_matches(".git")
        .rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("_");

    let slug = if slug.is_empty() {
        "default".to_string()
    } else {
        slug
    };

    base.join(slug)
}

fn default_cache_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".cache").join("skillet")
    } else {
        PathBuf::from("/tmp").join("skillet")
    }
}

#[tokio::main]
async fn main() -> Result<(), tower_mcp::BoxError> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(format!("skillet={}", args.log_level).parse()?)
                .add_directive(format!("tower_mcp={}", args.log_level).parse()?),
        )
        .with_writer(std::io::stderr)
        .init();

    // Determine the registry path
    let registry_path = match (&args.registry, &args.remote) {
        (Some(path), None) => path.clone(),
        (None, Some(url)) => {
            let base = args.cache_dir.unwrap_or_else(default_cache_dir);
            let target = cache_dir_for_url(&base, url);

            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }

            git::clone_or_pull(url, &target)?;
            target
        }
        (None, None) => {
            // Default to local test-registry for development
            PathBuf::from("test-registry")
        }
        (Some(_), Some(_)) => unreachable!("clap group prevents this"),
    };

    let registry_path = match args.subdir {
        Some(sub) => registry_path.join(sub),
        None => registry_path,
    };

    tracing::info!(registry = %registry_path.display(), "Starting skillet server");

    // Load registry config and skill index
    let config = index::load_config(&registry_path)?;
    let skill_index = index::load_index(&registry_path)?;
    let skill_search = search::SkillSearch::build(&skill_index);
    let state = AppState::new(registry_path, skill_index, skill_search, config);

    // Spawn background refresh task if using a remote
    if let Some(url) = args.remote {
        let interval = parse_duration(&args.refresh_interval)?;
        if interval > Duration::ZERO {
            spawn_refresh_task(Arc::clone(&state), url, interval);
        }
    }

    // Build tools
    let search_skills = tools::search_skills::build(state.clone());
    let list_categories = tools::list_categories::build(state.clone());
    let list_skills_by_owner = tools::list_skills_by_owner::build(state.clone());

    // Build resource templates
    let skill_content = resources::skill_content::build(state.clone());
    let skill_content_versioned = resources::skill_content::build_versioned(state.clone());
    let skill_metadata = resources::skill_metadata::build(state.clone());
    let skill_files = resources::skill_files::build(state.clone());

    // Assemble router
    let router = McpRouter::new()
        .server_info(&state.config.registry.name, env!("CARGO_PKG_VERSION"))
        .instructions(
            "Skillet is a skill registry for AI agents. Use it to discover and \
             fetch skills relevant to your current task.\n\n\
             Tools:\n\
             - search_skills: Search for skills by keyword, category, tag, or model\n\
             - list_categories: Browse all skill categories\n\
             - list_skills_by_owner: List all skills by a publisher\n\n\
             Resources:\n\
             - skillet://skills/{owner}/{name}: Get a skill's SKILL.md content\n\
             - skillet://skills/{owner}/{name}/{version}: Get a specific version\n\
             - skillet://metadata/{owner}/{name}: Get a skill's metadata (skill.toml)\n\
             - skillet://files/{owner}/{name}/{path}: Get a file from the skillpack \
             (scripts, references, or assets)\n\n\
             Workflow: search for skills with tools, then fetch the SKILL.md content \
             via resource templates. You can use the skill inline for this session \
             or install it locally for persistent use. If a skill includes extra \
             files (scripts, references), fetch them via the files resource.\n\n\
             Using skills:\n\
             - **Inline (default)**: Read the resource and follow the skill's \
             instructions for the current session. No restart needed.\n\
             - **Install**: Write the SKILL.md content to .claude/skills/<name>.md \
             (project) or ~/.claude/skills/<name>.md (global) for persistent use \
             across sessions. Requires a restart to take effect.\n\
             - **Install and use**: Write the file for persistence AND follow \
             the instructions inline for immediate use.\n\n\
             Prefer inline use unless the user asks for installation.",
        )
        .tool(search_skills)
        .tool(list_categories)
        .tool(list_skills_by_owner)
        .resource_template(skill_content)
        .resource_template(skill_content_versioned)
        .resource_template(skill_metadata)
        .resource_template(skill_files);

    tracing::info!("Serving over stdio");
    StdioTransport::new(router).run().await?;

    Ok(())
}

/// Spawn a background task that periodically pulls from the remote and
/// reloads the index if the HEAD commit changes.
fn spawn_refresh_task(state: Arc<AppState>, url: String, interval: Duration) {
    tracing::info!(
        interval_secs = interval.as_secs(),
        "Starting background refresh task"
    );

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;

            let registry_path = state.registry_path.clone();

            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<_>> {
                let before = git::head(&registry_path)?;
                git::pull(&registry_path)?;
                let after = git::head(&registry_path)?;

                if before == after {
                    return Ok(None);
                }

                tracing::info!(
                    before = %before,
                    after = %after,
                    "HEAD changed, reloading index"
                );

                let new_index = index::load_index(&registry_path)?;
                Ok(Some(new_index))
            })
            .await;

            match result {
                Ok(Ok(Some(new_index))) => {
                    let new_search = search::SkillSearch::build(&new_index);
                    let mut idx = state.index.write().await;
                    let mut srch = state.search.write().await;
                    *idx = new_index;
                    *srch = new_search;
                    tracing::info!(url = %url, "Index refreshed from remote");
                }
                Ok(Ok(None)) => {
                    tracing::debug!(url = %url, "No changes from remote");
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        url = %url,
                        error = %e,
                        "Failed to refresh from remote, keeping current index"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        url = %url,
                        error = %e,
                        "Refresh task panicked, keeping current index"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_duration_zero() {
        assert_eq!(parse_duration("0").unwrap(), Duration::ZERO);
    }

    #[test]
    fn test_parse_duration_bare_number() {
        assert_eq!(parse_duration("60").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn test_cache_dir_for_url_github() {
        let base = PathBuf::from("/tmp/skillet");
        let dir = cache_dir_for_url(&base, "https://github.com/owner/repo.git");
        assert_eq!(dir, PathBuf::from("/tmp/skillet/owner_repo"));
    }

    #[test]
    fn test_cache_dir_for_url_no_git_suffix() {
        let base = PathBuf::from("/tmp/skillet");
        let dir = cache_dir_for_url(&base, "https://github.com/owner/repo");
        assert_eq!(dir, PathBuf::from("/tmp/skillet/owner_repo"));
    }
}
