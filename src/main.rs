//! Skillet CLI and MCP Server
//!
//! Binary entry point. CLI parsing (clap), MCP server setup (tower-mcp),
//! and transport management. Core logic lives in the library crate.

mod cli;
mod tools;

use std::collections::HashSet;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use tower_mcp::registry::DynamicPromptRegistry;
use tower_mcp::transport::http::HttpTransport;
use tower_mcp::{McpRouter, StdioTransport};

use skillet_mcp::cache::{self, RepoSource};
use skillet_mcp::config;
use skillet_mcp::repo::{cache_dir_for_url, default_cache_dir, parse_duration};
use skillet_mcp::state::AppState;
use skillet_mcp::{git, index, prompts, repo, search, state};

#[derive(Parser, Debug)]
#[command(name = "skillet")]
#[command(version)]
#[command(about = "MCP-native skill discovery for AI agents")]
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
    /// Run the MCP server (default when stdin is not a terminal)
    Serve(ServeArgs),
    /// Initialize a skillet.toml project manifest
    #[command(alias = "init-project")]
    Init(InitArgs),
    /// Search for skills
    Search(SearchArgs),
    /// List all skill categories with counts
    Categories(CategoriesArgs),
    /// Show detailed information about a skill
    Info(InfoArgs),
    /// Manage configured repos
    Repo(RepoCommand),
    /// Discover skill repos on GitHub
    Discover(DiscoverArgs),
}

#[derive(clap::Args, Debug)]
struct DiscoverArgs {
    /// Optional search query to narrow results
    query: Option<String>,
}

#[derive(clap::Args, Debug, Clone)]
struct ServeArgs {
    /// Path to a local repo directory (can be specified multiple times)
    #[arg(long, alias = "registry")]
    repo: Vec<PathBuf>,

    /// Git URL to clone/pull as a remote repo (can be specified multiple times)
    #[arg(long)]
    remote: Vec<String>,

    /// How often to pull from remotes (e.g. "5m", "1h", "0" to disable).
    /// Only used with --remote.
    #[arg(long, default_value = "5m")]
    refresh_interval: String,

    /// Directory to clone remote repos into
    #[arg(long)]
    cache_dir: Option<PathBuf>,

    /// Subdirectory within repos that contains the skills
    #[arg(long)]
    subdir: Option<PathBuf>,

    /// Watch local repo directories for changes and auto-reload
    #[arg(long)]
    watch: bool,

    /// Serve over HTTP instead of stdio (e.g. "0.0.0.0:8080")
    #[arg(long)]
    http: Option<String>,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Read-only mode: expose only read-only tools
    #[arg(long, conflicts_with = "tools")]
    read_only: bool,

    /// Explicit tool allowlist (comma-separated: search,categories,owner,info)
    #[arg(long, value_delimiter = ',')]
    tools: Vec<String>,

    /// Don't follow `[[suggest]]` entries from loaded repos
    #[arg(long)]
    no_suggest: bool,
}

#[derive(clap::Args, Debug)]
struct InitArgs {
    /// Directory for the project (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Project name (defaults to directory name)
    #[arg(long)]
    name: Option<String>,

    /// Project description
    #[arg(long)]
    description: Option<String>,

    /// Include a \[skill\] section for a single inline skill
    #[arg(long)]
    skill: bool,

    /// Include a \[skills\] section for multiple skills
    #[arg(long)]
    multi: bool,
}

/// Shared repo source arguments for CLI subcommands.
#[derive(clap::Args, Debug, Clone)]
struct RepoArgs {
    /// Path to a local repo directory (can be specified multiple times)
    #[arg(long, alias = "registry")]
    repo: Vec<PathBuf>,

    /// Git URL to clone/pull the repo from (can be specified multiple times)
    #[arg(long)]
    remote: Vec<String>,

    /// Subdirectory within repos that contains the skills
    #[arg(long)]
    subdir: Option<PathBuf>,

    /// Bypass the disk cache and rebuild the index from scratch
    #[arg(long)]
    no_cache: bool,

    /// Don't follow `[[suggest]]` entries from loaded repos
    #[arg(long)]
    no_suggest: bool,
}

#[derive(clap::Args, Debug)]
struct SearchArgs {
    /// Search query (or "*" for all skills)
    query: String,

    /// Filter by category
    #[arg(long)]
    category: Option<String>,

    /// Filter by tag
    #[arg(long)]
    tag: Option<String>,

    /// Filter by owner
    #[arg(long)]
    owner: Option<String>,

    #[command(flatten)]
    repos: RepoArgs,
}

#[derive(clap::Args, Debug)]
struct CategoriesArgs {
    #[command(flatten)]
    repos: RepoArgs,
}

#[derive(clap::Args, Debug)]
struct InfoArgs {
    /// Skill to show in owner/name format
    skill: String,

    #[command(flatten)]
    repos: RepoArgs,
}

#[derive(clap::Args, Debug)]
struct RepoCommand {
    #[command(subcommand)]
    action: RepoAction,
}

#[derive(Subcommand, Debug)]
enum RepoAction {
    /// Add a repo (local path or remote URL)
    Add(RepoAddArgs),
    /// Remove a repo (local path or remote URL)
    Remove(RepoRemoveArgs),
    /// List configured repos
    List,
}

#[derive(clap::Args, Debug)]
struct RepoAddArgs {
    /// Repo to add (local path or remote URL)
    repo: String,
}

#[derive(clap::Args, Debug)]
struct RepoRemoveArgs {
    /// Repo to remove (local path or remote URL)
    repo: String,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    // If the user provided serve-specific args (--repo, --remote, --http),
    // they intend to serve even from a TTY. Only show help when truly bare.
    let has_serve_args =
        !cli.serve.repo.is_empty() || !cli.serve.remote.is_empty() || cli.serve.http.is_some();
    let interactive_tty =
        cli.command.is_none() && std::io::stdin().is_terminal() && !has_serve_args;

    match cli.command {
        Some(Command::Init(args)) => cli::author::run_init(args),
        Some(Command::Search(args)) => cli::search::run_search(args),
        Some(Command::Categories(args)) => cli::search::run_categories(args),
        Some(Command::Info(args)) => cli::search::run_info(args),
        Some(Command::Repo(args)) => cli::repo::run_repo(args),
        Some(Command::Discover(args)) => run_discover(args),
        Some(Command::Serve(args)) => run_serve(args).await,
        None if interactive_tty => {
            eprintln!("Skillet - skill discovery for AI agents\n");
            eprintln!("Get started:");
            eprintln!("  skillet search \"*\"          # browse all skills");
            eprintln!("  skillet info owner/name    # show skill details");
            eprintln!("  skillet --help             # see all commands\n");
            eprintln!("To start the MCP server:");
            eprintln!("  skillet serve              # stdio transport");
            eprintln!("  skillet serve --http :8080 # HTTP transport");
            ExitCode::SUCCESS
        }
        None => run_serve(cli.serve).await,
    }
}

/// All known tool short names.
const ALL_TOOL_NAMES: &[&str] = &["search", "categories", "owner", "info", "annotate"];

/// Resolved set of capabilities to expose from the MCP server.
struct ServerCapabilities {
    tools: HashSet<String>,
}

impl ServerCapabilities {
    /// Resolve capabilities from CLI flags and config.
    ///
    /// Priority: CLI flags > config > defaults (all exposed).
    fn resolve(args: &ServeArgs, cli_config: &config::SkilletConfig) -> Self {
        let tools = if !args.tools.is_empty() {
            args.tools.iter().cloned().collect()
        } else if args.read_only {
            ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect()
        } else if !cli_config.server.tools.is_empty() {
            cli_config.server.tools.iter().cloned().collect()
        } else {
            ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect()
        };

        Self { tools }
    }
}

/// Build an MCP router from a loaded AppState and resolved capabilities.
///
/// Returns the router and a `DynamicPromptRegistry` handle for updating
/// prompts on index refresh.
fn build_router(
    state: Arc<AppState>,
    caps: &ServerCapabilities,
) -> (McpRouter, DynamicPromptRegistry) {
    let mut router = McpRouter::new().server_info(&state.config.name, env!("CARGO_PKG_VERSION"));

    // Register tools conditionally
    if caps.tools.contains("search") {
        router = router.tool(tools::search_skills::build(state.clone()));
    }
    if caps.tools.contains("categories") {
        router = router.tool(tools::list_categories::build(state.clone()));
    }
    if caps.tools.contains("owner") {
        router = router.tool(tools::list_skills_by_owner::build(state.clone()));
    }
    if caps.tools.contains("info") {
        router = router.tool(tools::info_skill::build(state.clone()));
    }
    if caps.tools.contains("annotate") {
        router = router.tool(tools::annotate_skill::build());
    }

    // Build dynamic instructions based on exposed capabilities
    router = router.instructions(build_instructions(caps));

    // Enable dynamic prompts -- skills are registered as prompts
    let (router, prompt_registry) = router.with_dynamic_prompts();

    (router, prompt_registry)
}

/// Generate MCP instructions text listing only exposed tools.
fn build_instructions(caps: &ServerCapabilities) -> String {
    let mut text = String::from(
        "Skillet is a skill discovery tool for AI agents. Use it to discover and \
         fetch skills relevant to your current task.\n\n",
    );

    // Tools section
    let mut tool_lines = Vec::new();
    if caps.tools.contains("search") {
        tool_lines.push("- search_skills: Search for skills by keyword, category, tag, or model");
    }
    if caps.tools.contains("categories") {
        tool_lines.push("- list_categories: Browse all skill categories");
    }
    if caps.tools.contains("owner") {
        tool_lines.push("- list_skills_by_owner: List all skills by a publisher");
    }
    if caps.tools.contains("info") {
        tool_lines.push(
            "- info_skill: Get detailed information about a specific skill (version, author, tags, files, etc.)",
        );
    }
    if caps.tools.contains("annotate") {
        tool_lines.push(
            "- annotate_skill: Attach a persistent note to a skill (records gaps, tips, corrections)",
        );
    }
    if !tool_lines.is_empty() {
        text.push_str("Tools:\n");
        for line in &tool_lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push('\n');
    }

    // Workflow guidance
    text.push_str(
        "Workflow: search for skills with the search tool, then use info_skill for details. \
         Skills are served as MCP prompts -- use prompts/list to see available skills \
         and get_prompt to retrieve skill content for your current session.\n",
    );

    text
}

/// Run the `discover` subcommand.
fn run_discover(args: DiscoverArgs) -> ExitCode {
    let repos = match skillet_mcp::discover::search_github(args.query.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("\nMake sure `gh` CLI is installed and authenticated.");
            return ExitCode::from(1);
        }
    };

    if repos.is_empty() {
        println!("No skill repos found on GitHub.");
        return ExitCode::SUCCESS;
    }

    println!(
        "Found {} repo{} with skillet.toml:\n",
        repos.len(),
        if repos.len() == 1 { "" } else { "s" }
    );

    for repo in &repos {
        println!("  {}", repo.full_name);
        if !repo.description.is_empty() {
            println!("    {}", repo.description);
        }
        println!("    skillet repo add {}", repo.clone_url);
        println!();
    }

    ExitCode::SUCCESS
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
    let cache_base = args.cache_dir.clone().unwrap_or_else(default_cache_dir);
    let cli_config = config::load_config().unwrap_or_default();
    let mut repo_paths = Vec::new();

    // Resolve local repos
    for path in &args.repo {
        let path = match &args.subdir {
            Some(sub) => path.join(sub),
            None => path.clone(),
        };
        tracing::info!(repo = %path.display(), "Adding local repo");
        repo_paths.push(path);
    }

    // Resolve remote repos (clone/pull)
    for url in &args.remote {
        let target = cache_dir_for_url(&cache_base, url);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        git::clone_or_pull(url, &target)?;

        // Resolve release model: checkout appropriate tag/ref
        if let Err(e) = skillet_mcp::resolve::resolve_and_checkout(&target, url, &cli_config.source)
        {
            tracing::warn!(url, error = %e, "Failed to resolve release ref, using default branch");
        }

        let path = match &args.subdir {
            Some(sub) => target.join(sub),
            None => target,
        };
        tracing::info!(repo = %path.display(), remote = %url, "Adding remote repo");
        repo_paths.push(path);
    }

    // Fall back to the official repo if nothing is configured
    let mut default_remote_urls = Vec::new();
    if repo_paths.is_empty() && args.remote.is_empty() {
        let url = repo::DEFAULT_REPO_URL;
        let target = cache_dir_for_url(&cache_base, url);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        git::clone_or_pull(url, &target)?;
        let path = match &args.subdir {
            Some(sub) => target.join(sub),
            None => target.join(repo::DEFAULT_REPO_SUBDIR),
        };
        tracing::info!(repo = %path.display(), remote = %url, "Using default repo");
        repo_paths.push(path);
        default_remote_urls.push(url.to_string());
    }

    tracing::info!(count = repo_paths.len(), "Starting skillet server");

    // Load and merge all repos
    let mut merged_index = state::SkillIndex::default();
    let mut config = state::ServerConfig::default();

    for (i, path) in repo_paths.iter().enumerate() {
        if i == 0 {
            // Use first repo's config for server name
            config = index::load_config(path)?;
        }
        let idx = index::load_index(path)?;
        merged_index.merge(idx);
    }

    // Follow [[suggest]] entries from loaded repos
    if !args.no_suggest && cli_config.suggest.enabled {
        let cache_enabled = cli_config.cache.enabled;
        let cache_ttl = if cache_enabled {
            repo::parse_duration(&cli_config.cache.ttl).unwrap_or(Duration::from_secs(300))
        } else {
            Duration::ZERO
        };
        let seed_urls: Vec<String> = args.remote.clone();
        let mut walker = skillet_mcp::suggest::SuggestWalker::new(
            &cli_config.suggest,
            &cache_base,
            cache_enabled,
            cache_ttl,
            &seed_urls,
            cli_config.source.clone(),
        );
        let seed_paths = repo_paths.clone();
        walker.walk(
            &seed_paths,
            &mut merged_index,
            &mut repo_paths,
            cli_config.suggest.max_depth,
            vec![],
        );
    }

    // Auto-detect skillet.toml in current directory for embedded skills
    if let Some(project_root) = skillet_mcp::project::find_skillet_toml(std::path::Path::new(".")) {
        match skillet_mcp::project::load_skillet_toml(&project_root) {
            Ok(Some(manifest)) if manifest.skill.is_some() || manifest.skills.is_some() => {
                let embedded = skillet_mcp::project::load_embedded_skills(&project_root, &manifest);
                if !embedded.skills.is_empty() {
                    tracing::info!(
                        count = embedded.skills.len(),
                        project = %project_root.display(),
                        "Loaded embedded skills from skillet.toml"
                    );
                    merged_index.merge(embedded);
                }
            }
            Ok(_) => {} // No skill sections or no manifest
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to load skillet.toml for embedded skills"
                );
            }
        }
    }

    let skill_search = search::SkillSearch::build(&merged_index);
    let mut remote_urls = args.remote.clone();
    remote_urls.extend(default_remote_urls);
    let state = AppState::new(
        repo_paths,
        remote_urls.clone(),
        merged_index,
        skill_search,
        config,
    );

    // Resolve which tools to expose and build the router
    let caps = ServerCapabilities::resolve(&args, &cli_config);
    tracing::info!(
        tools = ?caps.tools.iter().collect::<Vec<_>>(),
        "Exposing MCP capabilities"
    );

    let (router, prompt_registry) = build_router(Arc::clone(&state), &caps);

    // Register all skills as MCP prompts
    {
        let index = state.index.read().await;
        prompts::register_all(&prompt_registry, &index);
        let count = index.skills.len();
        tracing::info!(count, "Registered skills as MCP prompts");
    }

    // Determine refresh interval: CLI flag wins, then server config, then "5m"
    let effective_interval = if args.refresh_interval == "5m" {
        state
            .config
            .refresh_interval
            .as_deref()
            .unwrap_or("5m")
            .to_string()
    } else {
        args.refresh_interval.clone()
    };
    let interval = parse_duration(&effective_interval)?;
    if interval > Duration::ZERO {
        for url in remote_urls {
            spawn_refresh_task(Arc::clone(&state), prompt_registry.clone(), url, interval);
        }
    }

    // Spawn filesystem watch task if requested
    if args.watch {
        spawn_watch_task(Arc::clone(&state), prompt_registry.clone());
    }

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

/// Reload all skill indexes, rebuild search, and sync prompts.
async fn reload_index(
    state: &Arc<AppState>,
    prompt_registry: &DynamicPromptRegistry,
) -> anyhow::Result<()> {
    let paths = state.repo_paths.clone();
    let remote_urls = state.remote_urls.clone();
    let cache_base = default_cache_dir();

    let new_index = tokio::task::spawn_blocking(move || {
        let mut merged = state::SkillIndex::default();
        for path in &paths {
            match index::load_index(path) {
                Ok(idx) => {
                    // Write cache for this individual repo
                    let source = repo_source_for_path(path, &remote_urls, &cache_base);
                    cache::write(&source, &idx);
                    merged.merge(idx);
                }
                Err(e) => {
                    tracing::warn!(
                        repo = %path.display(),
                        error = %e,
                        "Failed to reload repo, skipping"
                    );
                }
            }
        }
        // Re-load embedded skills from skillet.toml
        if let Some(project_root) =
            skillet_mcp::project::find_skillet_toml(std::path::Path::new("."))
            && let Ok(Some(manifest)) = skillet_mcp::project::load_skillet_toml(&project_root)
            && (manifest.skill.is_some() || manifest.skills.is_some())
        {
            let embedded = skillet_mcp::project::load_embedded_skills(&project_root, &manifest);
            if !embedded.skills.is_empty() {
                tracing::info!(count = embedded.skills.len(), "Re-loaded embedded skills");
                merged.merge(embedded);
            }
        }
        merged
    })
    .await?;

    let new_search = search::SkillSearch::build(&new_index);

    // Sync prompts: unregister removed skills, register new/updated ones
    let old_index = state.index.read().await;
    prompts::sync(prompt_registry, &old_index, &new_index);
    drop(old_index);

    let mut idx = state.index.write().await;
    let mut srch = state.search.write().await;
    *idx = new_index;
    *srch = new_search;
    Ok(())
}

/// Determine the cache `RepoSource` for a given repo path.
fn repo_source_for_path(
    path: &std::path::Path,
    remote_urls: &[String],
    cache_base: &std::path::Path,
) -> RepoSource {
    for url in remote_urls {
        let checkout = cache_dir_for_url(cache_base, url);
        if path.starts_with(&checkout) {
            return RepoSource::Remote {
                url: url.clone(),
                checkout,
            };
        }
    }
    RepoSource::Local(path.to_path_buf())
}

/// Spawn a background task that periodically pulls from a remote and
/// reloads all indexes if the HEAD commit changes.
fn spawn_refresh_task(
    state: Arc<AppState>,
    prompt_registry: DynamicPromptRegistry,
    url: String,
    interval: Duration,
) {
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
                Ok(Ok(true)) => match reload_index(&state, &prompt_registry).await {
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

/// Spawn a background task that watches all local repo directories for
/// changes and reloads the index when relevant files are modified.
fn spawn_watch_task(state: Arc<AppState>, prompt_registry: DynamicPromptRegistry) {
    use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};

    for path in &state.repo_paths {
        tracing::info!(
            repo = %path.display(),
            "Watching local repo for changes"
        );
    }

    let watch_paths: Vec<PathBuf> = state.repo_paths.clone();

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
                    .expect("failed to watch repo directory");
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

            if dominated_by_relevant {
                tracing::info!("File change detected, reloading index");
                if let Err(e) = reload_index(&state, &prompt_registry).await {
                    tracing::warn!(error = %e, "Failed to reload after file change");
                }
            }
        }
    });
}
