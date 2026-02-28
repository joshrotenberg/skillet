//! Skillet CLI and MCP Server
//!
//! Binary entry point. CLI parsing (clap), MCP server setup (tower-mcp),
//! and transport management. Core logic lives in the library crate.

mod cli;
mod resources;
mod tools;

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use tower_mcp::transport::http::HttpTransport;
use tower_mcp::{McpRouter, StdioTransport};

use skillet_mcp::cache::{self, RegistrySource};
use skillet_mcp::config;
use skillet_mcp::registry::{cache_dir_for_url, default_cache_dir, parse_duration};
use skillet_mcp::state::AppState;
use skillet_mcp::{discover, git, index, registry, search, state};

#[derive(Parser, Debug)]
#[command(name = "skillet")]
#[command(version)]
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
    /// Scaffold a new skillpack directory
    InitSkill(InitSkillArgs),
    /// Initialize a skillet.toml project manifest
    InitProject(InitProjectArgs),
    /// Install a skill from a registry
    Install(InstallArgs),
    /// Search for skills in registries
    Search(SearchArgs),
    /// List all skill categories with counts
    Categories(CategoriesArgs),
    /// Show detailed information about a skill
    Info(InfoArgs),
    /// List installed skills
    List(ListArgs),
    /// Manage trusted registries and pinned skills
    Trust(TrustArgs),
    /// Audit installed skills against pinned content hashes
    Audit(AuditArgs),
    /// Generate initial configuration
    Setup(SetupArgs),
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

    /// Read-only mode: don't expose the install_skill tool
    #[arg(long, conflicts_with = "tools")]
    read_only: bool,

    /// Don't expose the install_skill tool
    #[arg(long, conflicts_with = "tools")]
    no_install: bool,

    /// Explicit tool allowlist (comma-separated: search,categories,owner,install)
    #[arg(long, value_delimiter = ',')]
    tools: Vec<String>,

    /// Explicit resource allowlist (comma-separated: skills,metadata,files)
    #[arg(long, value_delimiter = ',')]
    resources: Vec<String>,
}

#[derive(clap::Args, Debug)]
struct ValidateArgs {
    /// Path to the skillpack directory to validate
    path: PathBuf,

    /// Skip safety scanning
    #[arg(long)]
    skip_safety: bool,
}

#[derive(clap::Args, Debug)]
struct PackArgs {
    /// Path to the skillpack directory
    path: PathBuf,

    /// Skip safety scanning
    #[arg(long)]
    skip_safety: bool,
}

#[derive(clap::Args, Debug)]
struct InitRegistryArgs {
    /// Directory to create the registry in
    path: PathBuf,

    /// Registry name (defaults to directory name)
    #[arg(long)]
    name: Option<String>,

    /// Registry description
    #[arg(long)]
    description: Option<String>,

    /// Generate legacy config.toml instead of skillet.toml
    #[arg(long)]
    legacy: bool,
}

#[derive(clap::Args, Debug)]
struct InitSkillArgs {
    /// Path for the new skillpack (e.g. owner/skill-name)
    path: PathBuf,

    /// Skill description
    #[arg(long)]
    description: Option<String>,

    /// Skill categories (can be specified multiple times)
    #[arg(long)]
    category: Vec<String>,

    /// Skill tags (comma-separated)
    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,
}

#[derive(clap::Args, Debug)]
struct InitProjectArgs {
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

    /// Include a \[registry\] section
    #[arg(long)]
    registry: bool,
}

#[derive(clap::Args, Debug)]
struct PublishArgs {
    /// Path to the skillpack directory
    path: PathBuf,

    /// Target registry repo in owner/repo format (e.g. "joshrotenberg/skillet")
    #[arg(long)]
    repo: String,

    /// Override the destination path in the registry (e.g. "acme/lang/java/maven-build").
    /// If not set, defaults to `owner/name/`.
    #[arg(long)]
    registry_path: Option<String>,

    /// Validate and show what would happen without creating a PR
    #[arg(long)]
    dry_run: bool,

    /// Skip safety scanning
    #[arg(long)]
    skip_safety: bool,
}

/// Shared registry source arguments for CLI subcommands.
#[derive(clap::Args, Debug, Clone)]
struct RegistryArgs {
    /// Path to a local registry directory (can be specified multiple times)
    #[arg(long)]
    registry: Vec<PathBuf>,

    /// Git URL to clone/pull the registry from (can be specified multiple times)
    #[arg(long)]
    remote: Vec<String>,

    /// Subdirectory within registries that contains the skills
    #[arg(long)]
    subdir: Option<PathBuf>,

    /// Bypass the disk cache and rebuild the index from scratch
    #[arg(long)]
    no_cache: bool,
}

#[derive(clap::Args, Debug)]
struct InstallArgs {
    /// Skill to install in owner/name format
    skill: String,

    /// Install target (agents, claude, cursor, copilot, windsurf, gemini, all)
    #[arg(long)]
    target: Vec<String>,

    /// Install globally instead of into the current project
    #[arg(long)]
    global: bool,

    /// Install a specific version (default: latest)
    #[arg(long)]
    version: Option<String>,

    /// Require trusted registry or pinned hash (blocks unknown sources)
    #[arg(long)]
    require_trusted: bool,

    #[command(flatten)]
    registries: RegistryArgs,
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
    registries: RegistryArgs,
}

#[derive(clap::Args, Debug)]
struct CategoriesArgs {
    #[command(flatten)]
    registries: RegistryArgs,
}

#[derive(clap::Args, Debug)]
struct InfoArgs {
    /// Skill to show in owner/name format
    skill: String,

    #[command(flatten)]
    registries: RegistryArgs,
}

#[derive(clap::Args, Debug)]
struct ListArgs {
    /// List installed skills (default behavior)
    #[arg(long, default_value_t = true)]
    installed: bool,
}

#[derive(clap::Args, Debug)]
struct TrustArgs {
    #[command(subcommand)]
    action: TrustAction,
}

#[derive(Subcommand, Debug)]
enum TrustAction {
    /// Add a registry to the trusted list
    AddRegistry(TrustAddRegistryArgs),
    /// Remove a registry from the trusted list
    RemoveRegistry(TrustRemoveRegistryArgs),
    /// List trusted registries and pinned skills
    List(TrustListArgs),
    /// Pin a skill's content hash
    Pin(TrustPinArgs),
    /// Remove a skill's content hash pin
    Unpin(TrustUnpinArgs),
}

#[derive(clap::Args, Debug)]
struct TrustAddRegistryArgs {
    /// Registry URL to trust
    url: String,
    /// Optional note describing why this registry is trusted
    #[arg(long)]
    note: Option<String>,
}

#[derive(clap::Args, Debug)]
struct TrustRemoveRegistryArgs {
    /// Registry URL to remove
    url: String,
}

#[derive(clap::Args, Debug)]
struct TrustListArgs {
    /// Only show trusted registries (omit pinned skills)
    #[arg(long)]
    registries_only: bool,
}

#[derive(clap::Args, Debug)]
struct TrustPinArgs {
    /// Skill to pin in owner/name format
    skill: String,

    #[command(flatten)]
    registries: RegistryArgs,
}

#[derive(clap::Args, Debug)]
struct TrustUnpinArgs {
    /// Skill to unpin in owner/name format
    skill: String,
}

#[derive(clap::Args, Debug)]
struct AuditArgs {
    /// Audit only a specific skill (owner/name format)
    #[arg(long)]
    skill: Option<String>,
}

#[derive(clap::Args, Debug)]
struct SetupArgs {
    /// Git URL of a remote registry to add (can be specified multiple times)
    #[arg(long)]
    remote: Vec<String>,

    /// Path to a local registry directory (can be specified multiple times)
    #[arg(long)]
    registry: Vec<PathBuf>,

    /// Default install target (agents, claude, cursor, copilot, windsurf, gemini, all)
    #[arg(long, default_value = "agents")]
    target: String,

    /// Skip adding the official registry as a remote
    #[arg(long)]
    no_official_registry: bool,

    /// Overwrite existing config file
    #[arg(long)]
    force: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let is_serve = matches!(cli.command, Some(Command::Serve(_)) | None);

    let exit_code = match cli.command {
        Some(Command::Validate(args)) => cli::author::run_validate(args),
        Some(Command::Pack(args)) => cli::author::run_pack(args),
        Some(Command::Publish(args)) => cli::author::run_publish(args),
        Some(Command::InitRegistry(args)) => cli::author::run_init_registry(args),
        Some(Command::InitSkill(args)) => cli::author::run_init_skill(args),
        Some(Command::InitProject(args)) => cli::author::run_init_project(args),
        Some(Command::Install(args)) => cli::install::run_install(args),
        Some(Command::Search(args)) => cli::search::run_search(args),
        Some(Command::Categories(args)) => cli::search::run_categories(args),
        Some(Command::Info(args)) => cli::search::run_info(args),
        Some(Command::List(args)) => cli::search::run_list(args),
        Some(Command::Trust(args)) => cli::trust::run_trust(args),
        Some(Command::Audit(args)) => cli::trust::run_audit(args),
        Some(Command::Setup(args)) => cli::setup::run_setup(args),
        Some(Command::Serve(args)) => run_serve(args).await,
        None => run_serve(cli.serve).await,
    };

    // Check for new version after CLI commands (not during serve)
    if !is_serve {
        skillet_mcp::version::check_and_notify();
    }

    exit_code
}

/// All known tool short names.
const ALL_TOOL_NAMES: &[&str] = &[
    "search",
    "categories",
    "owner",
    "install",
    "info",
    "compare",
    "status",
    "list_installed",
    "audit",
    "setup",
    "validate",
];

/// All known resource short names.
const ALL_RESOURCE_NAMES: &[&str] = &["skills", "metadata", "files"];

/// Resolved set of capabilities to expose from the MCP server.
struct ServerCapabilities {
    tools: HashSet<String>,
    resources: HashSet<String>,
}

impl ServerCapabilities {
    /// Resolve capabilities from CLI flags and config.
    ///
    /// Priority: CLI flags > config > defaults (all exposed).
    fn resolve(args: &ServeArgs, cli_config: &config::SkilletConfig) -> Self {
        let tools = if !args.tools.is_empty() {
            args.tools.iter().cloned().collect()
        } else if args.read_only || args.no_install {
            ALL_TOOL_NAMES
                .iter()
                .filter(|&&t| t != "install" && t != "setup")
                .map(|&s| s.to_string())
                .collect()
        } else if !cli_config.server.tools.is_empty() {
            cli_config.server.tools.iter().cloned().collect()
        } else {
            ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect()
        };

        let resources = if !args.resources.is_empty() {
            args.resources.iter().cloned().collect()
        } else if !cli_config.server.resources.is_empty() {
            cli_config.server.resources.iter().cloned().collect()
        } else {
            ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect()
        };

        Self { tools, resources }
    }
}

/// Build an MCP router from a loaded AppState and resolved capabilities.
fn build_router(state: Arc<AppState>, caps: &ServerCapabilities) -> McpRouter {
    let mut router =
        McpRouter::new().server_info(&state.config.registry.name, env!("CARGO_PKG_VERSION"));

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
    if caps.tools.contains("install") {
        router = router.tool(tools::install_skill::build(state.clone()));
    }
    if caps.tools.contains("info") {
        router = router.tool(tools::info_skill::build(state.clone()));
    }
    if caps.tools.contains("compare") {
        router = router.tool(tools::compare_skills::build(state.clone()));
    }
    if caps.tools.contains("status") {
        router = router.tool(tools::skill_status::build(state.clone()));
    }
    if caps.tools.contains("list_installed") {
        router = router.tool(tools::list_installed::build(state.clone()));
    }
    if caps.tools.contains("audit") {
        router = router.tool(tools::audit_skills::build(state.clone()));
    }
    if caps.tools.contains("setup") {
        router = router.tool(tools::setup_config::build(state.clone()));
    }
    if caps.tools.contains("validate") {
        router = router.tool(tools::validate_skill::build(state.clone()));
    }

    // Register resources conditionally
    if caps.resources.contains("skills") {
        router = router.resource_template(resources::skill_content::build(state.clone()));
        router = router.resource_template(resources::skill_content::build_versioned(state.clone()));
    }
    if caps.resources.contains("metadata") {
        router = router.resource_template(resources::skill_metadata::build(state.clone()));
    }
    if caps.resources.contains("files") {
        router = router.resource_template(resources::skill_files::build(state.clone()));
    }

    // Build dynamic instructions based on exposed capabilities
    router = router.instructions(build_instructions(caps));

    router
}

/// Generate MCP instructions text listing only exposed tools and resources.
fn build_instructions(caps: &ServerCapabilities) -> String {
    let mut text = String::from(
        "Skillet is a skill registry for AI agents. Use it to discover and \
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
    if caps.tools.contains("install") {
        tool_lines
            .push("- install_skill: Install a skill to the local filesystem for persistent use");
    }
    if caps.tools.contains("info") {
        tool_lines.push(
            "- info_skill: Get detailed information about a specific skill (version, author, tags, files, etc.)",
        );
    }
    if caps.tools.contains("compare") {
        tool_lines.push(
            "- compare_skills: Compare two skills side-by-side (overlap, differences, content)",
        );
    }
    if caps.tools.contains("status") {
        tool_lines.push(
            "- skill_status: Show installed skill status (version, integrity, trust, updates available)",
        );
    }
    if caps.tools.contains("list_installed") {
        tool_lines
            .push("- list_installed: List all skills currently installed on the local filesystem");
    }
    if caps.tools.contains("audit") {
        tool_lines.push(
            "- audit_skills: Audit installed skills against pinned content hashes for integrity",
        );
    }
    if caps.tools.contains("setup") {
        tool_lines.push(
            "- setup_config: Generate initial skillet configuration at ~/.config/skillet/config.toml",
        );
    }
    if caps.tools.contains("validate") {
        tool_lines
            .push("- validate_skill: Validate a skillpack directory for correctness and safety");
    }
    if !tool_lines.is_empty() {
        text.push_str("Tools:\n");
        for line in &tool_lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push('\n');
    }

    // Resources section
    let mut resource_lines = Vec::new();
    if caps.resources.contains("skills") {
        resource_lines.push("- skillet://skills/{owner}/{name}: Get a skill's SKILL.md content");
        resource_lines.push("- skillet://skills/{owner}/{name}/{version}: Get a specific version");
    }
    if caps.resources.contains("metadata") {
        resource_lines
            .push("- skillet://metadata/{owner}/{name}: Get a skill's metadata (skill.toml)");
    }
    if caps.resources.contains("files") {
        resource_lines.push(
            "- skillet://files/{owner}/{name}/{path}: Get a file from the skillpack \
             (scripts, references, or assets)",
        );
    }
    if !resource_lines.is_empty() {
        text.push_str("Resources:\n");
        for line in &resource_lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push('\n');
    }

    // Workflow guidance
    text.push_str(
        "Workflow: search for skills with tools, then fetch the SKILL.md content \
         via resource templates. You can use the skill inline for this session \
         or install it locally for persistent use. If a skill includes extra \
         files (scripts, references), fetch them via the files resource.\n\n\
         Using skills:\n\
         - **Inline (default)**: Read the resource and follow the skill's \
         instructions for the current session. No restart needed.\n",
    );
    if caps.tools.contains("install") {
        text.push_str(
            "- **Install**: Use the install_skill tool to write SKILL.md to the \
             appropriate agent skills directory. Supports multiple targets \
             (agents, claude, cursor, copilot, windsurf, gemini) and project \
             or global scope. A restart may be required.\n\
             - **Install and use**: Install for persistence AND follow \
             the instructions inline for immediate use.\n\n\
             Prefer inline use unless the user asks for installation.",
        );
    }

    text
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

    // Fall back to the official registry if nothing is configured
    let mut default_remote_urls = Vec::new();
    if registry_paths.is_empty() && args.remote.is_empty() {
        let url = registry::DEFAULT_REGISTRY_URL;
        let target = cache_dir_for_url(&cache_base, url);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        git::clone_or_pull(url, &target)?;
        let path = match &args.subdir {
            Some(sub) => target.join(sub),
            None => target.join(registry::DEFAULT_REGISTRY_SUBDIR),
        };
        tracing::info!(registry = %path.display(), remote = %url, "Using default registry");
        registry_paths.push(path);
        default_remote_urls.push(url.to_string());
    }

    tracing::info!(count = registry_paths.len(), "Starting skillet server");

    // Load CLI config early (used for discovery and capabilities)
    let cli_config = config::load_config().unwrap_or_default();

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

    // Auto-discover locally installed skills
    if cli_config.server.discover_local {
        let local_index = discover::discover_local_skills();
        if !local_index.skills.is_empty() {
            tracing::info!(count = local_index.skills.len(), "Discovered local skills");
            merged_index.merge(local_index);
        }
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
        registry_paths,
        remote_urls.clone(),
        merged_index,
        skill_search,
        config,
    );

    // Determine refresh interval: CLI flag wins, then registry defaults, then "5m"
    let effective_interval = if args.refresh_interval == "5m" {
        // CLI is at default -- check registry config for an override
        state
            .config
            .registry
            .defaults
            .as_ref()
            .and_then(|d| d.refresh_interval.as_deref())
            .unwrap_or("5m")
            .to_string()
    } else {
        args.refresh_interval.clone()
    };
    let interval = parse_duration(&effective_interval)?;
    if interval > Duration::ZERO {
        for url in remote_urls {
            spawn_refresh_task(Arc::clone(&state), url, interval);
        }
    }

    // Spawn filesystem watch task if requested
    if args.watch {
        spawn_watch_task(Arc::clone(&state));
    }

    // Resolve which tools/resources to expose (cli_config loaded earlier)
    let caps = ServerCapabilities::resolve(&args, &cli_config);
    tracing::info!(
        tools = ?caps.tools.iter().collect::<Vec<_>>(),
        resources = ?caps.resources.iter().collect::<Vec<_>>(),
        "Exposing MCP capabilities"
    );

    let router = build_router(state, &caps);

    if let Some(addr) = args.http {
        tracing::info!(addr = %addr, "Serving over HTTP");
        // SECURITY: Origin validation is disabled to allow connections from any
        // origin. The HTTP transport is intended for local development and trusted
        // networks. In production, place behind a reverse proxy with proper
        // authentication and CORS configuration.
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

/// Reload all skill indexes and rebuild search, writing cache for each registry.
async fn reload_index(state: &Arc<AppState>) -> anyhow::Result<()> {
    let paths = state.registry_paths.clone();
    let remote_urls = state.remote_urls.clone();
    let cache_base = default_cache_dir();

    let new_index = tokio::task::spawn_blocking(move || {
        let mut merged = state::SkillIndex::default();
        for path in &paths {
            match index::load_index(path) {
                Ok(idx) => {
                    // Write cache for this individual registry
                    let source = registry_source_for_path(path, &remote_urls, &cache_base);
                    cache::write(&source, &idx);
                    merged.merge(idx);
                }
                Err(e) => {
                    tracing::warn!(
                        registry = %path.display(),
                        error = %e,
                        "Failed to reload registry, skipping"
                    );
                }
            }
        }
        // Re-discover local skills (never cached, always live from disk)
        let cli_config = config::load_config().unwrap_or_default();
        if cli_config.server.discover_local {
            let local_index = discover::discover_local_skills();
            if !local_index.skills.is_empty() {
                tracing::info!(
                    count = local_index.skills.len(),
                    "Re-discovered local skills"
                );
                merged.merge(local_index);
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
    let mut idx = state.index.write().await;
    let mut srch = state.search.write().await;
    *idx = new_index;
    *srch = new_search;
    Ok(())
}

/// Determine the cache `RegistrySource` for a given registry path.
///
/// Checks if the path is a cached clone of a known remote URL; if so,
/// returns `RegistrySource::Remote`, otherwise `RegistrySource::Local`.
fn registry_source_for_path(
    path: &std::path::Path,
    remote_urls: &[String],
    cache_base: &std::path::Path,
) -> RegistrySource {
    for url in remote_urls {
        let checkout = cache_dir_for_url(cache_base, url);
        if path.starts_with(&checkout) {
            return RegistrySource::Remote {
                url: url.clone(),
                checkout,
            };
        }
    }
    RegistrySource::Local(path.to_path_buf())
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
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        build_router(state, &caps)
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
        assert!(names.contains(&"install_skill"));
        assert!(names.contains(&"info_skill"));
        assert!(names.contains(&"compare_skills"));
        assert!(names.contains(&"skill_status"));
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

    #[tokio::test]
    async fn test_mcp_search_skills_with_category_filter() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "search_skills",
                serde_json::json!({"query": "*", "category": "security"}),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(
            text.contains("security-audit"),
            "should find security-audit: {text}"
        );
        // Should not include skills from other categories
        assert!(
            !text.contains("python-dev"),
            "should not include python-dev: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_search_skills_with_tag_filter() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "search_skills",
                serde_json::json!({"query": "*", "tag": "pytest"}),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(
            text.contains("python-dev"),
            "should find python-dev (has pytest tag): {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_search_skills_with_verified_with_filter() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "search_skills",
                serde_json::json!({"query": "*", "verified_with": "claude-sonnet-4-6"}),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(
            text.contains("python-dev"),
            "python-dev is verified with claude-sonnet-4-6: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_search_skills_no_results() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "search_skills",
                serde_json::json!({"query": "nonexistent_xyzzy_skill"}),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(
            text.contains("No skills found"),
            "should report no results: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_read_files_resource() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .read_resource("skillet://files/acme/python-dev/scripts/lint.sh")
            .await;

        let text = result.first_text().expect("should have text content");
        assert!(text.contains("ruff"), "lint.sh should mention ruff: {text}");
    }

    #[tokio::test]
    async fn test_mcp_read_files_resource_reference() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .read_resource("skillet://files/acme/python-dev/references/RUFF_CONFIG.md")
            .await;

        let text = result.first_text().expect("should have text content");
        assert!(
            text.contains("pyproject.toml"),
            "RUFF_CONFIG.md should mention pyproject.toml: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_list_skills_by_owner_no_results() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "list_skills_by_owner",
                serde_json::json!({"owner": "nonexistent_owner"}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("No skills found") || text.contains("0"),
            "should handle nonexistent owner: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_call_nonexistent_tool() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let error = client
            .call_tool_expect_error("nonexistent_tool", serde_json::json!({}))
            .await;

        assert!(error.get("code").is_some());
    }

    /// Build a default ServeArgs for testing capability resolution.
    fn default_serve_args() -> ServeArgs {
        ServeArgs {
            registry: Vec::new(),
            remote: Vec::new(),
            refresh_interval: "5m".to_string(),
            cache_dir: None,
            subdir: None,
            watch: false,
            http: None,
            log_level: "info".to_string(),
            read_only: false,
            no_install: false,
            tools: Vec::new(),
            resources: Vec::new(),
        }
    }

    #[test]
    fn test_caps_default_all() {
        let args = default_serve_args();
        let config = config::SkilletConfig::default();
        let caps = ServerCapabilities::resolve(&args, &config);
        for &name in ALL_TOOL_NAMES {
            assert!(caps.tools.contains(name), "should contain tool {name}");
        }
        for &name in ALL_RESOURCE_NAMES {
            assert!(
                caps.resources.contains(name),
                "should contain resource {name}"
            );
        }
    }

    #[test]
    fn test_caps_no_install() {
        let args = ServeArgs {
            no_install: true,
            ..default_serve_args()
        };
        let config = config::SkilletConfig::default();
        let caps = ServerCapabilities::resolve(&args, &config);
        assert!(caps.tools.contains("search"));
        assert!(caps.tools.contains("categories"));
        assert!(caps.tools.contains("owner"));
        assert!(!caps.tools.contains("install"));
    }

    #[test]
    fn test_caps_read_only_same_as_no_install() {
        let args = ServeArgs {
            read_only: true,
            ..default_serve_args()
        };
        let config = config::SkilletConfig::default();
        let caps = ServerCapabilities::resolve(&args, &config);
        assert!(!caps.tools.contains("install"));
        assert!(caps.tools.contains("search"));
    }

    #[test]
    fn test_caps_explicit_tools() {
        let args = ServeArgs {
            tools: vec!["search".to_string(), "categories".to_string()],
            ..default_serve_args()
        };
        let config = config::SkilletConfig::default();
        let caps = ServerCapabilities::resolve(&args, &config);
        assert_eq!(caps.tools.len(), 2);
        assert!(caps.tools.contains("search"));
        assert!(caps.tools.contains("categories"));
        assert!(!caps.tools.contains("install"));
        assert!(!caps.tools.contains("owner"));
    }

    #[test]
    fn test_caps_explicit_resources() {
        let args = ServeArgs {
            resources: vec!["skills".to_string()],
            ..default_serve_args()
        };
        let config = config::SkilletConfig::default();
        let caps = ServerCapabilities::resolve(&args, &config);
        assert_eq!(caps.resources.len(), 1);
        assert!(caps.resources.contains("skills"));
        assert!(!caps.resources.contains("metadata"));
        assert!(!caps.resources.contains("files"));
    }

    #[test]
    fn test_caps_config_tools() {
        let args = default_serve_args();
        let config = config::SkilletConfig {
            server: config::ServerConfig {
                tools: vec!["search".to_string(), "owner".to_string()],
                resources: Vec::new(),
                ..Default::default()
            },
            ..Default::default()
        };
        let caps = ServerCapabilities::resolve(&args, &config);
        assert_eq!(caps.tools.len(), 2);
        assert!(caps.tools.contains("search"));
        assert!(caps.tools.contains("owner"));
        // Resources should default to all
        for &name in ALL_RESOURCE_NAMES {
            assert!(caps.resources.contains(name));
        }
    }

    #[test]
    fn test_caps_config_resources() {
        let args = default_serve_args();
        let config = config::SkilletConfig {
            server: config::ServerConfig {
                tools: Vec::new(),
                resources: vec!["skills".to_string(), "metadata".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let caps = ServerCapabilities::resolve(&args, &config);
        assert_eq!(caps.resources.len(), 2);
        assert!(caps.resources.contains("skills"));
        assert!(caps.resources.contains("metadata"));
        assert!(!caps.resources.contains("files"));
    }

    #[test]
    fn test_caps_cli_overrides_config() {
        let args = ServeArgs {
            tools: vec!["search".to_string()],
            resources: vec!["files".to_string()],
            ..default_serve_args()
        };
        let config = config::SkilletConfig {
            server: config::ServerConfig {
                tools: vec![
                    "search".to_string(),
                    "categories".to_string(),
                    "owner".to_string(),
                ],
                resources: vec!["skills".to_string(), "metadata".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let caps = ServerCapabilities::resolve(&args, &config);
        assert_eq!(caps.tools.len(), 1);
        assert!(caps.tools.contains("search"));
        assert_eq!(caps.resources.len(), 1);
        assert!(caps.resources.contains("files"));
    }

    // ── info_skill tool tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_info_skill() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "joshrotenberg", "name": "rust-dev"}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("joshrotenberg/rust-dev"),
            "should show owner/name header: {text}"
        );
        assert!(text.contains("**Version:**"), "should show version: {text}");
        assert!(
            text.contains("**Description:**"),
            "should show description: {text}"
        );
        assert!(
            text.contains("Rust"),
            "should mention Rust in description: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_info_skill_metadata_fields() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "joshrotenberg", "name": "rust-dev"}),
            )
            .await;
        let text = result.all_text();

        // rust-dev has author, license, categories, tags, verified_with
        assert!(text.contains("**Author:**"), "should show author: {text}");
        assert!(text.contains("**License:**"), "should show license: {text}");
        assert!(
            text.contains("**Categories:**"),
            "should show categories: {text}"
        );
        assert!(text.contains("**Tags:**"), "should show tags: {text}");
        assert!(
            text.contains("**Verified with:**"),
            "should show verified_with: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_info_skill_not_found() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "nonexistent", "name": "no-such-skill"}),
            )
            .await;

        assert!(result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("not found"),
            "should report skill not found: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_info_skill_with_files() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // python-dev has extra files (scripts/, references/)
        let result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "acme", "name": "python-dev"}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("**Files:**"),
            "should list extra files: {text}"
        );
    }

    // ── install_skill tool tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_install_skill_not_found() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "install_skill",
                serde_json::json!({"owner": "nonexistent", "name": "no-such-skill"}),
            )
            .await;

        assert!(result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("not found"),
            "should report skill not found: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_install_skill_local_early_return() {
        use skillet_mcp::state::{SkillEntry, SkillInfo, SkillMetadata, SkillSource, SkillVersion};
        use std::collections::HashMap;

        // Build a router with a synthetic local skill injected
        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let mut skill_index = index::load_index(&registry_path).expect("load index");

        // Insert a synthetic local skill
        skill_index.skills.insert(
            ("local".to_string(), "my-local-skill".to_string()),
            SkillEntry {
                owner: "local".to_string(),
                name: "my-local-skill".to_string(),
                registry_path: None,
                versions: vec![SkillVersion {
                    version: "0.0.0".to_string(),
                    metadata: SkillMetadata {
                        skill: SkillInfo {
                            name: "my-local-skill".to_string(),
                            owner: "local".to_string(),
                            version: "0.0.0".to_string(),
                            description: "A local skill".to_string(),
                            trigger: None,
                            license: None,
                            author: None,
                            classification: None,
                            compatibility: None,
                        },
                    },
                    skill_md: "# My Local Skill".to_string(),
                    skill_toml_raw: String::new(),
                    yanked: false,
                    files: HashMap::new(),
                    published: None,
                    has_content: true,
                    content_hash: None,
                    integrity_ok: None,
                }],
                source: SkillSource::Local {
                    platform: "claude".to_string(),
                    path: PathBuf::from("/tmp/fake-skill-dir"),
                },
            },
        );

        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "install_skill",
                serde_json::json!({"owner": "local", "name": "my-local-skill"}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("already installed locally"),
            "should report already installed: {text}"
        );
        assert!(
            text.contains("claude"),
            "should mention the platform: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_install_skill_invalid_target() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "install_skill",
                serde_json::json!({
                    "owner": "joshrotenberg",
                    "name": "rust-dev",
                    "target": "invalid_platform"
                }),
            )
            .await;

        assert!(result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("invalid_platform") || text.contains("Unknown"),
            "should report invalid target: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_info_skill_all_yanked() {
        use skillet_mcp::state::{SkillEntry, SkillInfo, SkillMetadata, SkillVersion};
        use std::collections::HashMap;

        // Build a router with a skill that has all versions yanked
        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let mut skill_index = index::load_index(&registry_path).expect("load index");

        skill_index.skills.insert(
            ("testowner".to_string(), "yanked-skill".to_string()),
            SkillEntry {
                owner: "testowner".to_string(),
                name: "yanked-skill".to_string(),
                registry_path: None,
                versions: vec![SkillVersion {
                    version: "1.0.0".to_string(),
                    metadata: SkillMetadata {
                        skill: SkillInfo {
                            name: "yanked-skill".to_string(),
                            owner: "testowner".to_string(),
                            version: "1.0.0".to_string(),
                            description: "A yanked skill".to_string(),
                            trigger: None,
                            license: None,
                            author: None,
                            classification: None,
                            compatibility: None,
                        },
                    },
                    skill_md: "# Yanked".to_string(),
                    skill_toml_raw: String::new(),
                    yanked: true,
                    files: HashMap::new(),
                    published: None,
                    has_content: true,
                    content_hash: None,
                    integrity_ok: None,
                }],
                source: Default::default(),
            },
        );

        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "testowner", "name": "yanked-skill"}),
            )
            .await;

        assert!(result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("yanked") || text.contains("No available versions"),
            "should report all versions yanked: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_install_skill_all_yanked() {
        use skillet_mcp::state::{SkillEntry, SkillInfo, SkillMetadata, SkillVersion};
        use std::collections::HashMap;

        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let mut skill_index = index::load_index(&registry_path).expect("load index");

        skill_index.skills.insert(
            ("testowner".to_string(), "yanked-skill".to_string()),
            SkillEntry {
                owner: "testowner".to_string(),
                name: "yanked-skill".to_string(),
                registry_path: None,
                versions: vec![SkillVersion {
                    version: "1.0.0".to_string(),
                    metadata: SkillMetadata {
                        skill: SkillInfo {
                            name: "yanked-skill".to_string(),
                            owner: "testowner".to_string(),
                            version: "1.0.0".to_string(),
                            description: "A yanked skill".to_string(),
                            trigger: None,
                            license: None,
                            author: None,
                            classification: None,
                            compatibility: None,
                        },
                    },
                    skill_md: "# Yanked".to_string(),
                    skill_toml_raw: String::new(),
                    yanked: true,
                    files: HashMap::new(),
                    published: None,
                    has_content: true,
                    content_hash: None,
                    integrity_ok: None,
                }],
                source: Default::default(),
            },
        );

        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "install_skill",
                serde_json::json!({"owner": "testowner", "name": "yanked-skill"}),
            )
            .await;

        assert!(result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("yanked") || text.contains("No available versions"),
            "should report all versions yanked: {text}"
        );
    }

    // ── Capability gating tests for info_skill ───────────────────────────

    #[tokio::test]
    async fn test_mcp_info_skill_excluded_when_not_in_caps() {
        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let skill_index = index::load_index(&registry_path).expect("load index");
        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        // Only include search, not info
        let caps = ServerCapabilities {
            tools: ["search"].iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let tools = client.list_tools().await;
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(
            !names.contains(&"info_skill"),
            "info_skill should be excluded"
        );
        assert!(
            names.contains(&"search_skills"),
            "search_skills should be present"
        );
    }

    // ── MCP multi-step scenario tests ───────────────────────────────────

    /// Discovery-to-read workflow: search -> info -> read resource -> verify consistency
    #[tokio::test]
    async fn test_scenario_discovery_to_read() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Step 1: Search for rust skills
        let search_result = client
            .call_tool("search_skills", serde_json::json!({"query": "rust"}))
            .await;
        assert!(!search_result.is_error);
        let search_text = search_result.all_text();
        assert!(
            search_text.contains("joshrotenberg/rust-dev"),
            "search should find rust-dev: {search_text}"
        );

        // Step 2: Get info on the found skill
        let info_result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "joshrotenberg", "name": "rust-dev"}),
            )
            .await;
        assert!(!info_result.is_error);
        let info_text = info_result.all_text();
        assert!(info_text.contains("**Version:**"));
        assert!(info_text.contains("**Description:**"));

        // Step 3: Read the SKILL.md content via resource
        let resource_result = client
            .read_resource("skillet://skills/joshrotenberg/rust-dev")
            .await;
        let skill_md = resource_result
            .first_text()
            .expect("should have SKILL.md content");
        assert!(
            skill_md.contains("Rust"),
            "SKILL.md should mention Rust: {skill_md}"
        );

        // Step 4: Read metadata via resource and verify consistency
        let metadata_result = client
            .read_resource("skillet://metadata/joshrotenberg/rust-dev")
            .await;
        let metadata = metadata_result
            .first_text()
            .expect("should have metadata content");
        assert!(
            metadata.contains("rust-dev"),
            "metadata should reference rust-dev: {metadata}"
        );
        assert!(
            metadata.contains("joshrotenberg"),
            "metadata should reference owner: {metadata}"
        );
    }

    /// Filtered browsing: list_categories -> search with category -> info -> verify match
    #[tokio::test]
    async fn test_scenario_filtered_browsing() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Step 1: List categories
        let cats_result = client
            .call_tool("list_categories", serde_json::json!({}))
            .await;
        assert!(!cats_result.is_error);
        let cats_text = cats_result.all_text();
        assert!(
            cats_text.contains("security"),
            "should list security category: {cats_text}"
        );

        // Step 2: Search filtered by security category
        let search_result = client
            .call_tool(
                "search_skills",
                serde_json::json!({"query": "*", "category": "security"}),
            )
            .await;
        assert!(!search_result.is_error);
        let search_text = search_result.all_text();
        assert!(
            search_text.contains("security-audit"),
            "should find security-audit: {search_text}"
        );
        assert!(
            !search_text.contains("python-dev"),
            "should not include non-security skills: {search_text}"
        );

        // Step 3: Info on the security skill
        let info_result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "joshrotenberg", "name": "security-audit"}),
            )
            .await;
        assert!(!info_result.is_error);
        let info_text = info_result.all_text();
        assert!(
            info_text.contains("security"),
            "info should show security category: {info_text}"
        );

        // Step 4: Metadata should be consistent
        let metadata_result = client
            .read_resource("skillet://metadata/joshrotenberg/security-audit")
            .await;
        let metadata = metadata_result.first_text().expect("should have metadata");
        assert!(
            metadata.contains("security"),
            "metadata should include security category: {metadata}"
        );
    }

    /// Files workflow: search for skill with files -> read file -> verify content
    #[tokio::test]
    async fn test_scenario_files_workflow() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Step 1: Info on python-dev (has extra files)
        let info_result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "acme", "name": "python-dev"}),
            )
            .await;
        assert!(!info_result.is_error);
        let info_text = info_result.all_text();
        assert!(
            info_text.contains("**Files:**"),
            "should list files: {info_text}"
        );
        assert!(
            info_text.contains("scripts/lint.sh"),
            "should mention lint.sh: {info_text}"
        );

        // Step 2: Read the script file via resource
        let file_result = client
            .read_resource("skillet://files/acme/python-dev/scripts/lint.sh")
            .await;
        let file_content = file_result.first_text().expect("should have file content");
        assert!(
            file_content.contains("ruff"),
            "lint.sh should mention ruff: {file_content}"
        );

        // Step 3: Read the reference file
        let ref_result = client
            .read_resource("skillet://files/acme/python-dev/references/RUFF_CONFIG.md")
            .await;
        let ref_content = ref_result
            .first_text()
            .expect("should have reference content");
        assert!(
            ref_content.contains("pyproject.toml"),
            "RUFF_CONFIG.md should mention pyproject.toml: {ref_content}"
        );

        // Step 4: Also read the SKILL.md to verify it's distinct from the files
        let skill_result = client
            .read_resource("skillet://skills/acme/python-dev")
            .await;
        let skill_md = skill_result.first_text().expect("should have SKILL.md");
        assert!(
            skill_md.contains("Python"),
            "SKILL.md should mention Python: {skill_md}"
        );
    }

    /// Owner browsing: list_skills_by_owner -> info on each -> verify all belong to owner
    #[tokio::test]
    async fn test_scenario_owner_browsing() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Step 1: List skills by joshrotenberg
        let list_result = client
            .call_tool(
                "list_skills_by_owner",
                serde_json::json!({"owner": "joshrotenberg"}),
            )
            .await;
        assert!(!list_result.is_error);
        let list_text = list_result.all_text();
        assert!(list_text.contains("rust-dev"));
        assert!(list_text.contains("security-audit"));

        // Step 2: Info on each skill found -- all should belong to joshrotenberg
        for skill_name in ["rust-dev", "security-audit", "code-review"] {
            let info_result = client
                .call_tool(
                    "info_skill",
                    serde_json::json!({"owner": "joshrotenberg", "name": skill_name}),
                )
                .await;
            assert!(
                !info_result.is_error,
                "info should succeed for {skill_name}"
            );
            let info_text = info_result.all_text();
            assert!(
                info_text.contains(&format!("joshrotenberg/{skill_name}")),
                "info should show correct owner/name for {skill_name}: {info_text}"
            );
        }
    }

    /// Error handling: verify graceful responses across tools and resources
    #[tokio::test]
    async fn test_scenario_error_handling() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Search with no results is not an error
        let search_result = client
            .call_tool(
                "search_skills",
                serde_json::json!({"query": "zzz_nonexistent_xyzzy"}),
            )
            .await;
        assert!(!search_result.is_error);
        assert!(search_result.all_text().contains("No skills found"));

        // Info on nonexistent skill is an error
        let info_result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "bad", "name": "nonexistent"}),
            )
            .await;
        assert!(info_result.is_error);
        assert!(info_result.all_text().contains("not found"));

        // Install nonexistent skill is an error
        let install_result = client
            .call_tool(
                "install_skill",
                serde_json::json!({"owner": "bad", "name": "nonexistent"}),
            )
            .await;
        assert!(install_result.is_error);
        assert!(install_result.all_text().contains("not found"));

        // List skills by nonexistent owner is not an error (just empty)
        let owner_result = client
            .call_tool(
                "list_skills_by_owner",
                serde_json::json!({"owner": "zzz_nobody"}),
            )
            .await;
        assert!(!owner_result.is_error);
    }

    /// Cross-tool consistency: search and info return consistent data
    #[tokio::test]
    async fn test_scenario_cross_tool_consistency() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Search returns version and description
        let search_result = client
            .call_tool(
                "search_skills",
                serde_json::json!({"query": "*", "tag": "rust"}),
            )
            .await;
        let search_text = search_result.all_text();
        assert!(search_text.contains("rust-dev"));

        // Info should show the same version
        let info_result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "joshrotenberg", "name": "rust-dev"}),
            )
            .await;
        let info_text = info_result.all_text();

        // Both should reference the same version string
        assert!(
            info_text.contains("2026.02.24"),
            "info should show version: {info_text}"
        );

        // Metadata resource should also be consistent
        let metadata_result = client
            .read_resource("skillet://metadata/joshrotenberg/rust-dev")
            .await;
        let metadata = metadata_result.first_text().expect("metadata");
        assert!(
            metadata.contains("2026.02.24"),
            "metadata should have same version: {metadata}"
        );
    }

    /// Wildcard search then drill down into multiple skills from different owners
    #[tokio::test]
    async fn test_scenario_browse_all_then_drill_down() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Step 1: Wildcard search to see everything
        let all_result = client
            .call_tool("search_skills", serde_json::json!({"query": "*"}))
            .await;
        assert!(!all_result.is_error);
        let all_text = all_result.all_text();
        assert!(all_text.contains("Found"));

        // Step 2: Pick skills from different owners and read their content
        let skills = [
            ("joshrotenberg", "rust-dev"),
            ("acme", "python-dev"),
            ("devtools", "api-design"),
        ];

        for (owner, name) in skills {
            let resource = client
                .read_resource(&format!("skillet://skills/{owner}/{name}"))
                .await;
            let content = resource
                .first_text()
                .unwrap_or_else(|| panic!("should have content for {owner}/{name}"));
            assert!(
                !content.is_empty(),
                "SKILL.md for {owner}/{name} should not be empty"
            );
        }
    }

    /// Build a router with limited capabilities and verify only those tools are listed.
    #[tokio::test]
    async fn test_mcp_limited_tools() {
        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let skill_index = index::load_index(&registry_path).expect("load index");
        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ["search", "categories"]
                .iter()
                .map(|&s| s.to_string())
                .collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let tools = client.list_tools().await;
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"search_skills"));
        assert!(names.contains(&"list_categories"));
        assert!(!names.contains(&"install_skill"));
        assert!(!names.contains(&"list_skills_by_owner"));
        assert!(!names.contains(&"compare_skills"));
        assert!(!names.contains(&"skill_status"));
    }

    // ── compare_skills tests ─────────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_compare_skills() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "compare_skills",
                serde_json::json!({
                    "owner_a": "joshrotenberg",
                    "name_a": "rust-dev",
                    "owner_b": "acme",
                    "name_b": "python-dev"
                }),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(text.contains("Comparison"));
        assert!(text.contains("joshrotenberg/rust-dev"));
        assert!(text.contains("acme/python-dev"));
        assert!(text.contains("Version"));
        assert!(text.contains("Description"));
    }

    #[tokio::test]
    async fn test_mcp_compare_skills_shared_category() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Compare two skills that share the "development" category
        let result = client
            .call_tool(
                "compare_skills",
                serde_json::json!({
                    "owner_a": "joshrotenberg",
                    "name_a": "rust-dev",
                    "owner_b": "acme",
                    "name_b": "python-dev"
                }),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        // Both skills have "development" category
        assert!(
            text.contains("development"),
            "should show shared development category"
        );
    }

    #[tokio::test]
    async fn test_mcp_compare_skills_not_found() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "compare_skills",
                serde_json::json!({
                    "owner_a": "nonexistent",
                    "name_a": "skill",
                    "owner_b": "joshrotenberg",
                    "name_b": "rust-dev"
                }),
            )
            .await;

        assert!(result.is_error);
        assert!(result.all_text().contains("not found"));
    }

    #[tokio::test]
    async fn test_mcp_compare_skills_second_not_found() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "compare_skills",
                serde_json::json!({
                    "owner_a": "joshrotenberg",
                    "name_a": "rust-dev",
                    "owner_b": "nonexistent",
                    "name_b": "skill"
                }),
            )
            .await;

        assert!(result.is_error);
        assert!(result.all_text().contains("not found"));
    }

    #[tokio::test]
    async fn test_mcp_compare_skills_content_size() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "compare_skills",
                serde_json::json!({
                    "owner_a": "joshrotenberg",
                    "name_a": "rust-dev",
                    "owner_b": "acme",
                    "name_b": "python-dev"
                }),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(text.contains("Content"), "should have Content section");
        assert!(text.contains("bytes"), "should show content size in bytes");
    }

    // ── skill_status tests ───────────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_skill_status_no_installs() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // skill_status reads from the installed manifest on disk.
        // In test context there are no installed skills, so it should report empty.
        let result = client
            .call_tool("skill_status", serde_json::json!({}))
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(
            text.contains("No skills installed") || text.contains("Installed Skills"),
            "should show no installs or installed skills: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_skill_status_with_filter() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "skill_status",
                serde_json::json!({"owner": "nonexistent", "name": "skill"}),
            )
            .await;
        let text = result.all_text();

        assert!(!result.is_error);
        assert!(
            text.contains("No installed skill matching"),
            "should indicate no match: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_compare_skills_excluded_when_not_in_caps() {
        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let skill_index = index::load_index(&registry_path).expect("load index");
        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ["search"].iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let tools = client.list_tools().await;
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(!names.contains(&"compare_skills"));
        assert!(!names.contains(&"skill_status"));
    }

    // ── validate_skill tool tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_validate_skill_valid() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let path = test_registry_path()
            .join("joshrotenberg/rust-dev")
            .display()
            .to_string();
        let result = client
            .call_tool("validate_skill", serde_json::json!({"path": path}))
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("Validation passed"),
            "should pass validation: {text}"
        );
        assert!(
            text.contains("joshrotenberg/rust-dev"),
            "should show owner/name: {text}"
        );
        assert!(
            text.contains("**SKILL.md**: ok"),
            "should show SKILL.md ok: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_validate_skill_nonexistent_path() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "validate_skill",
                serde_json::json!({"path": "/tmp/nonexistent-skill-xyzzy-12345"}),
            )
            .await;

        assert!(result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("error") || text.contains("Error"),
            "should report error: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_validate_skill_unsafe() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let path = test_registry_path()
            .join("acme/unsafe-demo")
            .display()
            .to_string();
        let result = client
            .call_tool("validate_skill", serde_json::json!({"path": path}))
            .await;

        // validate_skill returns the safety report as text, not as an error
        let text = result.all_text();
        assert!(
            text.contains("Safety scan") || text.contains("safety issues"),
            "should report safety issues: {text}"
        );
        assert!(
            text.contains("danger") || text.contains("Danger"),
            "should show danger findings: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_validate_skill_skip_safety() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let path = test_registry_path()
            .join("acme/unsafe-demo")
            .display()
            .to_string();
        let result = client
            .call_tool(
                "validate_skill",
                serde_json::json!({"path": path, "skip_safety": true}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("Validation passed"),
            "should pass when safety skipped: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_validate_skill_with_extra_files() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let path = test_registry_path()
            .join("acme/python-dev")
            .display()
            .to_string();
        let result = client
            .call_tool("validate_skill", serde_json::json!({"path": path}))
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("extra files"),
            "should list extra files: {text}"
        );
        assert!(
            text.contains("scripts/lint.sh"),
            "should mention lint.sh: {text}"
        );
    }

    // ── list_installed tool tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_list_installed_empty() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool("list_installed", serde_json::json!({}))
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("No skills installed") || text.contains("Installed Skills"),
            "should handle empty state: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_list_installed_with_owner_filter() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "list_installed",
                serde_json::json!({"owner": "nonexistent_owner"}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("No installed skills from"),
            "should report no skills from owner: {text}"
        );
    }

    // ── audit_skills tool tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_audit_skills_no_filter() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool("audit_skills", serde_json::json!({}))
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        // May find real installed skills or report none -- either is valid
        assert!(
            text.contains("Audit Results") || text.contains("No installed skills to audit"),
            "should run audit: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_audit_skills_with_filter() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        let result = client
            .call_tool(
                "audit_skills",
                serde_json::json!({"owner": "nonexistent", "name": "no-skill"}),
            )
            .await;

        assert!(!result.is_error);
        let text = result.all_text();
        assert!(
            text.contains("No installed skills to audit"),
            "should find nothing to audit: {text}"
        );
    }

    // ── setup_config tool tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_setup_config_already_exists() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // setup_config checks ~/.config/skillet/config.toml.
        // If it exists (likely in dev), it should report already exists.
        // If it doesn't exist, it would create one. Either way, not an error.
        let result = client
            .call_tool("setup_config", serde_json::json!({}))
            .await;

        // Should not be a hard error -- either creates config or says it exists
        let text = result.all_text();
        assert!(
            text.contains("config") || text.contains("Config"),
            "should mention config: {text}"
        );
    }

    // ── Resource edge case tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_mcp_read_versioned_skill_resource() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // rust-dev has version 2026.02.24 in the test-registry
        let result = client
            .read_resource("skillet://skills/joshrotenberg/rust-dev/2026.02.24")
            .await;
        let text = result.first_text().expect("should have versioned content");
        assert!(
            text.contains("Rust"),
            "versioned content should mention Rust: {text}"
        );
    }

    #[tokio::test]
    #[should_panic(expected = "Version '99.99.99' not found")]
    async fn test_mcp_read_versioned_skill_resource_not_found() {
        let router = test_router();
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // TestClient::read_resource panics on MCP errors, so we verify
        // the error message via should_panic
        let _ = client
            .read_resource("skillet://skills/joshrotenberg/rust-dev/99.99.99")
            .await;
    }

    // ── Local discovery via MCP (#144) ───────────────────────────────

    #[tokio::test]
    async fn test_mcp_local_skill_in_search() {
        use skillet_mcp::state::{SkillEntry, SkillInfo, SkillMetadata, SkillSource, SkillVersion};
        use std::collections::HashMap;

        // Build state with a synthetic local skill
        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let mut skill_index = index::load_index(&registry_path).expect("load index");

        skill_index.skills.insert(
            ("local".to_string(), "discovered-tool".to_string()),
            SkillEntry {
                owner: "local".to_string(),
                name: "discovered-tool".to_string(),
                registry_path: None,
                versions: vec![SkillVersion {
                    version: "0.0.0".to_string(),
                    metadata: SkillMetadata {
                        skill: SkillInfo {
                            name: "discovered-tool".to_string(),
                            owner: "local".to_string(),
                            version: "0.0.0".to_string(),
                            description: "A locally discovered tool".to_string(),
                            trigger: None,
                            license: None,
                            author: None,
                            classification: None,
                            compatibility: None,
                        },
                    },
                    skill_md: "# Discovered Tool\n\nLocally found.\n".to_string(),
                    skill_toml_raw: String::new(),
                    yanked: false,
                    files: HashMap::new(),
                    published: None,
                    has_content: true,
                    content_hash: None,
                    integrity_ok: None,
                }],
                source: SkillSource::Local {
                    platform: "claude".to_string(),
                    path: PathBuf::from("/home/user/.claude/skills/discovered-tool"),
                },
            },
        );

        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Local skill should appear in wildcard search
        let result = client
            .call_tool("search_skills", serde_json::json!({"query": "*"}))
            .await;
        let text = result.all_text();
        assert!(
            text.contains("discovered-tool"),
            "local skill should appear in search: {text}"
        );

        // Registry skills should also still appear
        assert!(
            text.contains("rust-dev"),
            "registry skills should still appear: {text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_local_skill_readable_via_resource() {
        use skillet_mcp::state::{SkillEntry, SkillInfo, SkillMetadata, SkillSource, SkillVersion};
        use std::collections::HashMap;

        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let mut skill_index = index::load_index(&registry_path).expect("load index");

        skill_index.skills.insert(
            ("local".to_string(), "readable-local".to_string()),
            SkillEntry {
                owner: "local".to_string(),
                name: "readable-local".to_string(),
                registry_path: None,
                versions: vec![SkillVersion {
                    version: "0.0.0".to_string(),
                    metadata: SkillMetadata {
                        skill: SkillInfo {
                            name: "readable-local".to_string(),
                            owner: "local".to_string(),
                            version: "0.0.0".to_string(),
                            description: "A readable local skill".to_string(),
                            trigger: None,
                            license: None,
                            author: None,
                            classification: None,
                            compatibility: None,
                        },
                    },
                    skill_md: "# Readable Local\n\nThis is local content.\n".to_string(),
                    skill_toml_raw: "[skill]\nname = \"readable-local\"\n".to_string(),
                    yanked: false,
                    files: HashMap::new(),
                    published: None,
                    has_content: true,
                    content_hash: None,
                    integrity_ok: None,
                }],
                source: SkillSource::Local {
                    platform: "agents".to_string(),
                    path: PathBuf::from("/home/user/.agents/skills/readable-local"),
                },
            },
        );

        let skill_search = search::SkillSearch::build(&skill_index);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            skill_index,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Read skill content via resource
        let result = client
            .read_resource("skillet://skills/local/readable-local")
            .await;
        let text = result.first_text().expect("should have content");
        assert!(
            text.contains("Readable Local"),
            "should return local skill content: {text}"
        );

        // Read metadata via resource
        let metadata = client
            .read_resource("skillet://metadata/local/readable-local")
            .await;
        let meta_text = metadata.first_text().expect("should have metadata");
        assert!(
            meta_text.contains("readable-local"),
            "should return local metadata: {meta_text}"
        );
    }

    #[tokio::test]
    async fn test_mcp_registry_wins_over_local_on_collision() {
        use skillet_mcp::state::{SkillEntry, SkillInfo, SkillMetadata, SkillSource, SkillVersion};
        use std::collections::HashMap;

        let registry_path = test_registry_path();
        let config = index::load_config(&registry_path).expect("load config");
        let skill_index = index::load_index(&registry_path).expect("load index");

        // Merge a local skill with the same (owner, name) as a registry skill
        // Since we build the index from the registry first, and the merge is
        // first-wins, the registry should win.
        let mut merged = skill_index;

        // Try to insert a local version of "joshrotenberg/rust-dev"
        // Since it already exists from the registry, merge should keep the registry version
        let local_index = {
            let mut idx = skillet_mcp::state::SkillIndex::default();
            idx.skills.insert(
                ("joshrotenberg".to_string(), "rust-dev".to_string()),
                SkillEntry {
                    owner: "joshrotenberg".to_string(),
                    name: "rust-dev".to_string(),
                    registry_path: None,
                    versions: vec![SkillVersion {
                        version: "0.0.0".to_string(),
                        metadata: SkillMetadata {
                            skill: SkillInfo {
                                name: "rust-dev".to_string(),
                                owner: "joshrotenberg".to_string(),
                                version: "0.0.0".to_string(),
                                description: "LOCAL VERSION SHOULD NOT WIN".to_string(),
                                trigger: None,
                                license: None,
                                author: None,
                                classification: None,
                                compatibility: None,
                            },
                        },
                        skill_md: "# LOCAL rust-dev\n\nThis should not appear.\n".to_string(),
                        skill_toml_raw: String::new(),
                        yanked: false,
                        files: HashMap::new(),
                        published: None,
                        has_content: true,
                        content_hash: None,
                        integrity_ok: None,
                    }],
                    source: SkillSource::Local {
                        platform: "claude".to_string(),
                        path: PathBuf::from("/tmp/local-rust-dev"),
                    },
                },
            );
            idx
        };
        merged.merge(local_index);

        let skill_search = search::SkillSearch::build(&merged);
        let state = AppState::new(
            vec![registry_path],
            Vec::new(),
            merged,
            skill_search,
            config,
        );
        let caps = ServerCapabilities {
            tools: ALL_TOOL_NAMES.iter().map(|&s| s.to_string()).collect(),
            resources: ALL_RESOURCE_NAMES.iter().map(|&s| s.to_string()).collect(),
        };
        let router = build_router(state, &caps);
        let mut client = tower_mcp::TestClient::from_router(router);
        client.initialize().await;

        // Info should show the registry version, not the local one
        let result = client
            .call_tool(
                "info_skill",
                serde_json::json!({"owner": "joshrotenberg", "name": "rust-dev"}),
            )
            .await;
        let text = result.all_text();
        assert!(
            !text.contains("LOCAL VERSION SHOULD NOT WIN"),
            "registry should win over local: {text}"
        );
        assert!(
            text.contains("2026.02.24"),
            "should show registry version: {text}"
        );
    }
}
