//! Skillet CLI and MCP Server
//!
//! Binary entry point. CLI parsing (clap), MCP server setup (tower-mcp),
//! and transport management. Core logic lives in the library crate.

mod resources;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use tower_mcp::transport::http::HttpTransport;
use tower_mcp::{McpRouter, StdioTransport};

use skillet_mcp::registry::{cache_dir_for_url, default_cache_dir, parse_duration};
use skillet_mcp::state::AppState;
use skillet_mcp::{git, index, pack, publish, registry, search, state, validate};

#[derive(Parser, Debug)]
#[command(name = "skillet")]
#[command(about = "MCP-native skill registry for AI agents")]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Top-level serve args for backward compat (no subcommand = implicit serve)
    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the MCP server (default when no subcommand is given)
    Serve(ServeArgs),
    /// Validate a skillpack directory
    Validate(ValidateArgs),
    /// Pack a skillpack (validate + generate manifest + update versions)
    Pack(PackArgs),
    /// Publish a skillpack to a registry (pack + open PR)
    Publish(PublishArgs),
    /// Initialize a new skill registry
    InitRegistry(InitRegistryArgs),
}

#[derive(clap::Args, Debug, Clone)]
struct ServeArgs {
    /// Path to a local registry directory (can be specified multiple times)
    #[arg(long)]
    registry: Vec<PathBuf>,

    /// Git URL to clone/pull the registry from (can be specified multiple times)
    #[arg(long)]
    remote: Vec<String>,

    /// How often to pull from remotes (e.g. "5m", "1h", "0" to disable).
    /// Only used with --remote.
    #[arg(long, default_value = "5m")]
    refresh_interval: String,

    /// Directory to clone remote registries into
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Subdirectory within registries that contains the skills
    #[arg(long)]
    subdir: Option<PathBuf>,

    /// Watch local registry directories for changes and auto-reload
    #[arg(long)]
    watch: bool,

    /// Serve over HTTP instead of stdio (e.g. "0.0.0.0:8080")
    #[arg(long)]
    http: Option<String>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(clap::Args, Debug)]
struct ValidateArgs {
    /// Path to the skillpack directory to validate
    path: PathBuf,
}

#[derive(clap::Args, Debug)]
struct PackArgs {
    /// Path to the skillpack directory
    path: PathBuf,
}

#[derive(clap::Args, Debug)]
struct InitRegistryArgs {
    /// Directory to create the registry in
    path: PathBuf,

    /// Registry name (defaults to directory name)
    #[arg(long)]
    name: Option<String>,
}

#[derive(clap::Args, Debug)]
struct PublishArgs {
    /// Path to the skillpack directory
    path: PathBuf,

    /// Target registry repo in owner/repo format (e.g. "joshrotenberg/skillet-registry")
    #[arg(long)]
    repo: String,

    /// Validate and show what would happen without creating a PR
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Validate(args)) => run_validate(args),
        Some(Command::Pack(args)) => run_pack(args),
        Some(Command::Publish(args)) => run_publish(args),
        Some(Command::InitRegistry(args)) => run_init_registry(args),
        Some(Command::Serve(args)) => run_serve(args).await,
        None => run_serve(cli.serve).await,
    }
}

/// Run the `validate` subcommand.
fn run_validate(args: ValidateArgs) -> ExitCode {
    let path = &args.path;
    println!("Validating {} ...\n", path.display());

    let result = match validate::validate_skillpack(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  error: {e}");
            eprintln!("\nValidation failed.");
            return ExitCode::from(1);
        }
    };

    // skill.toml
    println!("  skill.toml ............ ok");

    // SKILL.md
    let line_count = result.skill_md.lines().count();
    println!("  SKILL.md .............. ok ({line_count} lines)");

    // Core fields
    println!("  owner ................. {}", result.owner);
    println!("  name .................. {}", result.name);
    println!("  version ............... {}", result.version);
    println!("  description ........... {}", result.description);

    // Categories and tags
    if let Some(ref classification) = result.metadata.skill.classification {
        if !classification.categories.is_empty() {
            println!(
                "  categories ............ {}",
                classification.categories.join(", ")
            );
        }
        if !classification.tags.is_empty() {
            println!(
                "  tags .................. {}",
                classification.tags.join(", ")
            );
        }
    }

    // Extra files
    if !result.files.is_empty() {
        let mut file_paths: Vec<&String> = result.files.keys().collect();
        file_paths.sort();
        println!(
            "  extra files ........... {}",
            file_paths
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Content hash (show abbreviated)
    let hash_display = if result.hashes.composite.len() > 17 {
        format!("{}...", &result.hashes.composite[..17])
    } else {
        result.hashes.composite.clone()
    };
    println!("  content hash .......... {hash_display}");

    // Manifest status
    match result.manifest_ok {
        Some(true) => println!("  manifest .............. verified"),
        Some(false) => println!("  manifest .............. MISMATCH"),
        None => println!("  manifest .............. not found (will be generated on publish)"),
    }

    // Warnings
    if !result.warnings.is_empty() {
        println!();
        for w in &result.warnings {
            println!("  warning: {w}");
        }
    }

    println!("\nValidation passed.");
    ExitCode::SUCCESS
}

/// Run the `pack` subcommand.
fn run_pack(args: PackArgs) -> ExitCode {
    let path = &args.path;
    println!("Packing {} ...\n", path.display());

    let result = match pack::pack(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  error: {e}");
            eprintln!("\nPack failed.");
            return ExitCode::from(1);
        }
    };

    let v = &result.validation;
    println!("  owner ................. {}", v.owner);
    println!("  name .................. {}", v.name);
    println!("  version ............... {}", v.version);

    if result.manifest_written {
        println!("  MANIFEST.sha256 ....... written");
    }

    if result.versions_updated {
        println!("  versions.toml ......... updated");
    } else {
        println!("  versions.toml ......... up to date");
    }

    println!("\nPack succeeded.");
    ExitCode::SUCCESS
}

/// Run the `publish` subcommand.
fn run_publish(args: PublishArgs) -> ExitCode {
    let path = &args.path;
    println!("Publishing {} to {} ...\n", path.display(), args.repo);

    let result = match publish::publish(path, &args.repo, args.dry_run) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  error: {e}");
            eprintln!("\nPublish failed.");
            return ExitCode::from(1);
        }
    };

    let v = &result.pack.validation;
    println!("  owner ................. {}", v.owner);
    println!("  name .................. {}", v.name);
    println!("  version ............... {}", v.version);

    if args.dry_run {
        println!("\nDry run complete.");
    } else {
        println!("  PR .................... {}", result.pr_url);
        println!("\nPublish succeeded.");
    }

    ExitCode::SUCCESS
}

/// Run the `init-registry` subcommand.
fn run_init_registry(args: InitRegistryArgs) -> ExitCode {
    let path = &args.path;

    if path.exists() {
        eprintln!("Error: {} already exists", path.display());
        return ExitCode::from(1);
    }

    let name = args
        .name
        .or_else(|| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "my-skills".to_string());

    if let Err(e) = registry::init_registry(path, &name) {
        eprintln!("Error: {e}");
        return ExitCode::from(1);
    }

    println!("Initialized skill registry at {}", path.display());
    println!();
    println!("  cd {}", path.display());
    println!("  # add skills: mkdir -p owner/skill-name");
    println!("  # serve locally: skillet --registry .");
    println!("  # push and serve remotely: skillet --remote <git-url>");

    ExitCode::SUCCESS
}

/// Build an MCP router from a loaded AppState.
fn build_router(state: Arc<AppState>) -> McpRouter {
    let search_skills = tools::search_skills::build(state.clone());
    let list_categories = tools::list_categories::build(state.clone());
    let list_skills_by_owner = tools::list_skills_by_owner::build(state.clone());

    let skill_content = resources::skill_content::build(state.clone());
    let skill_content_versioned = resources::skill_content::build_versioned(state.clone());
    let skill_metadata = resources::skill_metadata::build(state.clone());
    let skill_files = resources::skill_files::build(state.clone());

    McpRouter::new()
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
        .resource_template(skill_files)
}

/// Run the MCP server (default behavior / `serve` subcommand).
async fn run_serve(args: ServeArgs) -> ExitCode {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(
                    format!("skillet={}", args.log_level)
                        .parse()
                        .expect("valid log directive"),
                )
                .add_directive(
                    format!("tower_mcp={}", args.log_level)
                        .parse()
                        .expect("valid log directive"),
                ),
        )
        .with_writer(std::io::stderr)
        .init();

    match run_serve_inner(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}

async fn run_serve_inner(args: ServeArgs) -> Result<(), tower_mcp::BoxError> {
    let cache_base = args.cache_dir.unwrap_or_else(default_cache_dir);
    let mut registry_paths = Vec::new();

    // Resolve local registries
    for path in &args.registry {
        let path = match &args.subdir {
            Some(sub) => path.join(sub),
            None => path.clone(),
        };
        tracing::info!(registry = %path.display(), "Adding local registry");
        registry_paths.push(path);
    }

    // Resolve remote registries (clone/pull)
    for url in &args.remote {
        let target = cache_dir_for_url(&cache_base, url);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        git::clone_or_pull(url, &target)?;
        let path = match &args.subdir {
            Some(sub) => target.join(sub),
            None => target,
        };
        tracing::info!(registry = %path.display(), remote = %url, "Adding remote registry");
        registry_paths.push(path);
    }

    // Default to local test-registry if nothing specified
    if registry_paths.is_empty() {
        registry_paths.push(PathBuf::from("test-registry"));
    }

    tracing::info!(count = registry_paths.len(), "Starting skillet server");

    // Load and merge all registries
    let mut merged_index = state::SkillIndex::default();
    let mut config = state::RegistryConfig::default();

    for (i, path) in registry_paths.iter().enumerate() {
        if i == 0 {
            // Use first registry's config for server name
            config = index::load_config(path)?;
        }
        let idx = index::load_index(path)?;
        merged_index.merge(idx);
    }

    let skill_search = search::SkillSearch::build(&merged_index);
    let state = AppState::new(registry_paths, merged_index, skill_search, config);

    // Spawn background refresh tasks for each remote
    let interval = parse_duration(&args.refresh_interval)?;
    if interval > Duration::ZERO {
        for url in args.remote {
            spawn_refresh_task(Arc::clone(&state), url, interval);
        }
    }

    // Spawn filesystem watch task if requested
    if args.watch {
        spawn_watch_task(Arc::clone(&state));
    }

    let router = build_router(state);

    if let Some(addr) = args.http {
        tracing::info!(addr = %addr, "Serving over HTTP");
        HttpTransport::new(router)
            .disable_origin_validation()
            .serve(&addr)
            .await?;
    } else {
        tracing::info!("Serving over stdio");
        StdioTransport::new(router).run().await?;
    }

    Ok(())
}

/// Reload all skill indexes and rebuild search.
async fn reload_index(state: &Arc<AppState>) -> anyhow::Result<()> {
    let paths = state.registry_paths.clone();
    let new_index = tokio::task::spawn_blocking(move || {
        let mut merged = state::SkillIndex::default();
        for path in &paths {
            match index::load_index(path) {
                Ok(idx) => merged.merge(idx),
                Err(e) => {
                    tracing::warn!(
                        registry = %path.display(),
                        error = %e,
                        "Failed to reload registry, skipping"
                    );
                }
            }
        }
        merged
    })
    .await?;
    let new_search = search::SkillSearch::build(&new_index);
    let mut idx = state.index.write().await;
    let mut srch = state.search.write().await;
    *idx = new_index;
    *srch = new_search;
    Ok(())
}

/// Spawn a background task that periodically pulls from a remote and
/// reloads all indexes if the HEAD commit changes.
fn spawn_refresh_task(state: Arc<AppState>, url: String, interval: Duration) {
    let cache_path = {
        let base = default_cache_dir();
        cache_dir_for_url(&base, &url)
    };

    tracing::info!(
        interval_secs = interval.as_secs(),
        remote = %url,
        "Starting background refresh task"
    );

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;

            let pull_path = cache_path.clone();

            let pull_result = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
                let before = git::head(&pull_path)?;
                git::pull(&pull_path)?;
                let after = git::head(&pull_path)?;

                if before == after {
                    return Ok(false);
                }

                tracing::info!(
                    before = %before,
                    after = %after,
                    "HEAD changed, reloading index"
                );
                Ok(true)
            })
            .await;

            match pull_result {
                Ok(Ok(true)) => match reload_index(&state).await {
                    Ok(()) => {
                        tracing::info!(url = %url, "Index refreshed from remote");
                    }
                    Err(e) => {
                        tracing::warn!(
                            url = %url,
                            error = %e,
                            "Failed to reload index after pull, keeping current index"
                        );
                    }
                },
                Ok(Ok(false)) => {
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

/// Spawn a background task that watches all local registry directories for
/// changes and reloads the index when relevant files are modified.
fn spawn_watch_task(state: Arc<AppState>) {
    use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};

    for path in &state.registry_paths {
        tracing::info!(
            registry = %path.display(),
            "Watching local registry for changes"
        );
    }

    let watch_paths: Vec<PathBuf> = state.registry_paths.clone();

    tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);

        // The debouncer must live for the lifetime of the task
        let _debouncer = {
            let debounce_timeout = Duration::from_millis(500);
            let rt = tokio::runtime::Handle::current();

            let mut debouncer =
                new_debouncer(debounce_timeout, move |events: Result<Vec<_>, _>| {
                    if let Ok(events) = events {
                        let _ = rt.block_on(tx.send(events));
                    }
                })
                .expect("failed to create filesystem watcher");

            for path in &watch_paths {
                debouncer
                    .watcher()
                    .watch(path, RecursiveMode::Recursive)
                    .expect("failed to watch registry directory");
            }

            debouncer
        };

        while let Some(events) = rx.recv().await {
            let dominated_by_relevant = events.iter().any(|e| {
                let path = &e.path;

                // Skip .git directory changes
                if path.components().any(|c| c.as_os_str() == ".git") {
                    return false;
                }

                // Only react to files that matter for the index
                match path.extension().and_then(|e| e.to_str()) {
                    Some("toml" | "md") => true,
                    _ => {
                        // Also react to changes in extra-file directories
                        path.components().any(|c| {
                            let s = c.as_os_str().to_string_lossy();
                            s == "scripts" || s == "references" || s == "assets"
                        })
                    }
                }
            });

            if !dominated_by_relevant {
                continue;
            }

            tracing::info!("Filesystem change detected, reloading index");

            match reload_index(&state).await {
                Ok(()) => {
                    tracing::info!("Index reloaded from filesystem change");
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to reload index after filesystem change, keeping current index"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-registry")
    }

    /// Build a router backed by the test-registry for integration tests.
    fn test_router() -> tower_mcp::McpRouter {
        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let skill_index = index::load_index(&registry_path).expect("load index");
        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(vec![registry_path], skill_index, skill_search, config);
        build_router(state)
    }

    #[tokio::test]
    async fn test_mcp_initialize() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        let init = client.initialize().await;

        assert!(init.get("protocolVersion").is_some());
        assert_eq!(
            init.get("serverInfo")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str()),
            Some("skillet")
        );
    }

    #[tokio::test]
    async fn test_mcp_list_tools() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let tools = client.list_tools().await;
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();

        assert!(names.contains(&"search_skills"));
        assert!(names.contains(&"list_categories"));
        assert!(names.contains(&"list_skills_by_owner"));
    }

    #[tokio::test]
    async fn test_mcp_search_skills() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool("search_skills", serde_json::json!({"query": "rust"}))
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(text.contains("rust-dev"), "should find rust-dev skill");
    }

    #[tokio::test]
    async fn test_mcp_search_skills_wildcard() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool("search_skills", serde_json::json!({"query": "*"}))
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(text.contains("Found"));
    }

    #[tokio::test]
    async fn test_mcp_list_categories() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool("list_categories", serde_json::json!({}))
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(text.contains("development"));
    }

    #[tokio::test]
    async fn test_mcp_list_skills_by_owner() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "list_skills_by_owner",
                serde_json::json!({"owner": "joshrotenberg"}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(text.contains("rust-dev"));
    }

    #[tokio::test]
    async fn test_mcp_read_skill_resource() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .read_resource("skillet://skills/joshrotenberg/rust-dev")
            .await;

        let text = result.first_text().expect("should have text content");
        assert!(
            text.contains("Rust"),
            "SKILL.md content should mention Rust"
        );
    }

    #[tokio::test]
    async fn test_mcp_read_metadata_resource() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .read_resource("skillet://metadata/joshrotenberg/rust-dev")
            .await;

        let text = result.first_text().expect("should have text content");
        assert!(text.contains("[skill]"), "should return skill.toml content");
    }
}
