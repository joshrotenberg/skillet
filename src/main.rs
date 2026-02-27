//! Skillet CLI and MCP Server
//!
//! Binary entry point. CLI parsing (clap), MCP server setup (tower-mcp),
//! and transport management. Core logic lives in the library crate.

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
use skillet_mcp::install::{self, InstallOptions};
use skillet_mcp::manifest;
use skillet_mcp::registry::{cache_dir_for_url, default_cache_dir, parse_duration};
use skillet_mcp::state::AppState;
use skillet_mcp::{
    discover, git, index, integrity, pack, publish, registry, safety, scaffold, search, state,
    trust, validate,
};

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
    /// Scaffold a new skillpack directory
    InitSkill(InitSkillArgs),
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
struct PublishArgs {
    /// Path to the skillpack directory
    path: PathBuf,

    /// Target registry repo in owner/repo format (e.g. "joshrotenberg/skillet-registry")
    #[arg(long)]
    repo: String,

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
        Some(Command::Validate(args)) => run_validate(args),
        Some(Command::Pack(args)) => run_pack(args),
        Some(Command::Publish(args)) => run_publish(args),
        Some(Command::InitRegistry(args)) => run_init_registry(args),
        Some(Command::InitSkill(args)) => run_init_skill(args),
        Some(Command::Install(args)) => run_install(args),
        Some(Command::Search(args)) => run_search(args),
        Some(Command::Categories(args)) => run_categories(args),
        Some(Command::Info(args)) => run_info(args),
        Some(Command::List(args)) => run_list(args),
        Some(Command::Trust(args)) => run_trust(args),
        Some(Command::Audit(args)) => run_audit(args),
        Some(Command::Setup(args)) => run_setup(args),
        Some(Command::Serve(args)) => run_serve(args).await,
        None => run_serve(cli.serve).await,
    };

    // Check for new version after CLI commands (not during serve)
    if !is_serve {
        skillet_mcp::version::check_and_notify();
    }

    exit_code
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

    // Safety scanning
    if !args.skip_safety {
        let cli_config = config::load_config().unwrap_or_default();
        let report = safety::scan(
            &result.skill_md,
            &result.skill_toml_raw,
            &result.files,
            &result.metadata,
            &cli_config.safety.suppress,
        );

        if !report.is_empty() {
            println!();
            print_safety_report(&report);
        }

        if report.has_danger() {
            eprintln!("\nValidation failed: safety issues detected.");
            return ExitCode::from(2);
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

    // Safety scanning
    if !args.skip_safety {
        let cli_config = config::load_config().unwrap_or_default();
        let report = safety::scan(
            &v.skill_md,
            &v.skill_toml_raw,
            &v.files,
            &v.metadata,
            &cli_config.safety.suppress,
        );

        if !report.is_empty() {
            println!();
            print_safety_report(&report);
        }

        if report.has_danger() {
            eprintln!("\nPack failed: safety issues detected.");
            return ExitCode::from(2);
        }
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

    // Safety scanning
    if !args.skip_safety {
        let cli_config = config::load_config().unwrap_or_default();
        let report = safety::scan(
            &v.skill_md,
            &v.skill_toml_raw,
            &v.files,
            &v.metadata,
            &cli_config.safety.suppress,
        );

        if !report.is_empty() {
            println!();
            print_safety_report(&report);
        }

        if report.has_danger() {
            eprintln!("\nPublish failed: safety issues detected.");
            return ExitCode::from(2);
        }
    }

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

    if let Err(e) = registry::init_registry(path, &name, args.description.as_deref()) {
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

/// Run the `init-skill` subcommand.
fn run_init_skill(args: InitSkillArgs) -> ExitCode {
    let path = &args.path;

    // Infer owner/name from path components
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => {
            eprintln!("Error: could not infer skill name from path");
            return ExitCode::from(1);
        }
    };

    let owner = match path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    {
        Some(o) if !o.is_empty() => o.to_string(),
        _ => {
            eprintln!(
                "Error: could not infer owner from path. Use owner/skill-name format (e.g. myname/my-skill)"
            );
            return ExitCode::from(1);
        }
    };

    let description = args
        .description
        .unwrap_or_else(|| format!("A skill for {name}"));

    if let Err(e) = scaffold::init_skill(
        path,
        &owner,
        &name,
        &description,
        &args.category,
        &args.tags,
    ) {
        eprintln!("Error: {e}");
        return ExitCode::from(1);
    }

    println!("Created skillpack at {}", path.display());
    println!();
    println!("  owner ................. {owner}");
    println!("  name .................. {name}");
    println!();
    println!("Next steps:");
    println!(
        "  1. Edit {}/skill.toml to customize metadata",
        path.display()
    );
    println!(
        "  2. Edit {}/SKILL.md to write your skill prompt",
        path.display()
    );
    println!("  3. Validate: skillet validate {}", path.display());

    ExitCode::SUCCESS
}

/// Parse an "owner/name" skill reference.
fn parse_skill_ref(s: &str) -> Result<(&str, &str), String> {
    let parts: Vec<&str> = s.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(format!(
            "Invalid skill reference: '{s}'. Expected format: owner/name"
        ));
    }
    Ok((parts[0], parts[1]))
}

/// Run the `install` subcommand.
fn run_install(args: InstallArgs) -> ExitCode {
    let (owner, name) = match parse_skill_ref(&args.skill) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut cli_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    if args.registries.no_cache {
        cli_config.cache.enabled = false;
    }

    let targets = match config::resolve_targets(&args.target, &cli_config) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let global = args.global || cli_config.install.global;

    let (skill_index, registry_paths) = match registry::load_registries(
        &args.registries.registry,
        &args.registries.remote,
        &cli_config,
        args.registries.subdir.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading registries: {e}");
            return ExitCode::from(1);
        }
    };

    // Look up the skill
    let entry = match skill_index
        .skills
        .get(&(owner.to_string(), name.to_string()))
    {
        Some(e) => e,
        None => {
            eprintln!("Error: skill '{owner}/{name}' not found in any registry");
            return ExitCode::from(1);
        }
    };

    // Resolve version
    let version = if let Some(ref requested) = args.version {
        match entry.versions.iter().find(|v| v.version == *requested) {
            Some(v) if !v.has_content => {
                eprintln!(
                    "Error: version {requested} exists but content is not available \
                     (only the latest version has full content)"
                );
                return ExitCode::from(1);
            }
            Some(v) => v,
            None => {
                let available: Vec<&str> =
                    entry.versions.iter().map(|v| v.version.as_str()).collect();
                eprintln!(
                    "Error: version '{requested}' not found for {owner}/{name}\n\
                     Available versions: {}",
                    available.join(", ")
                );
                return ExitCode::from(1);
            }
        }
    } else {
        match entry.latest() {
            Some(v) => v,
            None => {
                eprintln!("Error: no available versions for {owner}/{name} (all yanked)");
                return ExitCode::from(1);
            }
        }
    };

    // Load manifest
    let mut installed_manifest = match manifest::load() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading installation manifest: {e}");
            return ExitCode::from(1);
        }
    };

    // Determine registry identifier
    let registry_id = if !registry_paths.is_empty() {
        registry::registry_id(&registry_paths[0], &args.registries.remote)
    } else {
        "unknown".to_string()
    };

    // Trust checking
    let content_hash = integrity::sha256_hex(&version.skill_md);
    let trust_state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    let trust_check = trust::check_trust(&trust_state, &registry_id, owner, name, &content_hash);

    match trust_check.tier {
        trust::TrustTier::Trusted => {}
        trust::TrustTier::Reviewed => {
            if trust_check.pinned_hash.as_deref() != Some(&content_hash) {
                eprintln!(
                    "Warning: {} (tier: reviewed, content changed since pinned)",
                    trust_check.reason
                );
            }
        }
        trust::TrustTier::Unknown => {
            // --require-trusted flag or config overrides policy
            if args.require_trusted || cli_config.trust.require_trusted {
                eprintln!(
                    "Error: {reason}\n\n\
                     Install blocked: --require-trusted is set.\n\
                     To install this skill, either:\n\
                     \n  1. Trust the registry:\n\
                     \n     skillet trust add-registry {registry_id}\n\
                     \n  2. Review and pin the skill:\n\
                     \n     skillet info {owner}/{name}\n\
                     \n     skillet trust pin {owner}/{name}\n\
                     \n     skillet install {owner}/{name}\n",
                    reason = trust_check.reason,
                );
                return ExitCode::from(1);
            }

            let policy = &cli_config.trust.unknown_policy;
            match policy.as_str() {
                "block" => {
                    eprintln!(
                        "Error: {reason}\n\n\
                         Install blocked by trust policy (unknown_policy = \"block\").\n\
                         To install this skill, either:\n\
                         \n  1. Trust the registry:\n\
                         \n     skillet trust add-registry {registry_id}\n\
                         \n  2. Review and pin the skill:\n\
                         \n     skillet info {owner}/{name}\n\
                         \n     skillet trust pin {owner}/{name}\n\
                         \n     skillet install {owner}/{name}\n",
                        reason = trust_check.reason,
                    );
                    return ExitCode::from(1);
                }
                "prompt" => {
                    eprintln!(
                        "Warning: {}\nProceed with install? [y/N] ",
                        trust_check.reason
                    );
                    let mut input = String::new();
                    if std::io::stdin().read_line(&mut input).is_err()
                        || !input.trim().eq_ignore_ascii_case("y")
                    {
                        eprintln!("Install cancelled.");
                        return ExitCode::from(1);
                    }
                }
                _ => {
                    // "warn" (default) -- explicit guidance
                    eprintln!(
                        "Warning: {reason}\n\
                         To verify before installing:\n\
                         \n  skillet info {owner}/{name}\n\
                         \n  skillet trust pin {owner}/{name}\n",
                        reason = trust_check.reason,
                    );
                }
            }
        }
    }

    let options = InstallOptions {
        targets,
        global,
        registry: registry_id.clone(),
    };

    let results =
        match install::install_skill(owner, name, version, &options, &mut installed_manifest) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error installing skill: {e}");
                return ExitCode::from(1);
            }
        };

    // Save manifest
    if let Err(e) = manifest::save(&installed_manifest) {
        eprintln!("Error saving installation manifest: {e}");
        return ExitCode::from(1);
    }

    // Auto-pin content hash after successful install
    if cli_config.trust.auto_pin {
        let mut trust_state = trust_state;
        trust_state.pin_skill(owner, name, &version.version, &registry_id, &content_hash);
        if let Err(e) = trust::save(&trust_state) {
            eprintln!("Warning: failed to save trust state: {e}");
        }
    }

    // Print results
    println!(
        "Installed {owner}/{name} v{version}",
        version = version.version
    );
    println!();
    for r in &results {
        let file_count = r.files_written.len();
        let scope = if options.global { "global" } else { "project" };
        println!(
            "  {target} ({scope}) ... {file_count} file{s} -> {path}",
            target = r.target,
            s = if file_count == 1 { "" } else { "s" },
            path = r.path.display(),
        );
    }

    // Safety scanning (informational only -- never blocks install)
    let report = safety::scan(
        &version.skill_md,
        &version.skill_toml_raw,
        &version.files,
        &version.metadata,
        &cli_config.safety.suppress,
    );

    if !report.is_empty() {
        println!();
        print_safety_report(&report);
    }

    ExitCode::SUCCESS
}

/// Run the `categories` subcommand.
fn run_categories(args: CategoriesArgs) -> ExitCode {
    let mut cli_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    if args.registries.no_cache {
        cli_config.cache.enabled = false;
    }

    let (skill_index, _registry_paths) = match registry::load_registries(
        &args.registries.registry,
        &args.registries.remote,
        &cli_config,
        args.registries.subdir.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading registries: {e}");
            return ExitCode::from(1);
        }
    };

    if skill_index.categories.is_empty() {
        println!("No categories found.");
        return ExitCode::SUCCESS;
    }

    let total: usize = skill_index.categories.values().sum();
    println!(
        "{} categor{} ({total} skill{}):\n",
        skill_index.categories.len(),
        if skill_index.categories.len() == 1 {
            "y"
        } else {
            "ies"
        },
        if total == 1 { "" } else { "s" },
    );
    for (name, count) in &skill_index.categories {
        println!("  {name} ({count})");
    }

    ExitCode::SUCCESS
}

/// Run the `search` subcommand.
fn run_search(args: SearchArgs) -> ExitCode {
    let mut cli_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    if args.registries.no_cache {
        cli_config.cache.enabled = false;
    }

    let (skill_index, _registry_paths) = match registry::load_registries(
        &args.registries.registry,
        &args.registries.remote,
        &cli_config,
        args.registries.subdir.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading registries: {e}");
            return ExitCode::from(1);
        }
    };

    let skill_search = search::SkillSearch::build(&skill_index);

    // Wildcard: list all skills
    let results: Vec<state::SkillSummary> = if args.query == "*" {
        let mut keys: Vec<_> = skill_index.skills.keys().collect();
        keys.sort();
        keys.iter()
            .filter_map(|k| {
                let entry = skill_index.skills.get(*k)?;
                state::SkillSummary::from_entry(entry)
            })
            .collect()
    } else {
        let hits = skill_search.search(&args.query, 20);
        hits.iter()
            .filter_map(|(owner, name, _score)| {
                let entry = skill_index.skills.get(&(owner.clone(), name.clone()))?;
                state::SkillSummary::from_entry(entry)
            })
            .collect()
    };

    // Apply structured filters
    let results: Vec<_> = results
        .into_iter()
        .filter(|s| {
            if let Some(ref cat) = args.category
                && !s.categories.iter().any(|c| c.eq_ignore_ascii_case(cat))
            {
                return false;
            }
            if let Some(ref tag) = args.tag
                && !s.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
            {
                return false;
            }
            if let Some(ref owner) = args.owner
                && !s.owner.eq_ignore_ascii_case(owner)
            {
                return false;
            }
            true
        })
        .collect();

    if results.is_empty() {
        println!("No skills found.");
        return ExitCode::SUCCESS;
    }

    println!(
        "Found {} skill{}:\n",
        results.len(),
        if results.len() == 1 { "" } else { "s" }
    );
    for s in &results {
        println!("  {}/{} v{}", s.owner, s.name, s.version);
        println!("    {}", s.description);
        if !s.categories.is_empty() {
            println!("    categories: {}", s.categories.join(", "));
        }
        if !s.tags.is_empty() {
            println!("    tags: {}", s.tags.join(", "));
        }
        println!();
    }

    ExitCode::SUCCESS
}

/// Run the `info` subcommand.
fn run_info(args: InfoArgs) -> ExitCode {
    let (owner, name) = match parse_skill_ref(&args.skill) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut cli_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    if args.registries.no_cache {
        cli_config.cache.enabled = false;
    }

    let (skill_index, _registry_paths) = match registry::load_registries(
        &args.registries.registry,
        &args.registries.remote,
        &cli_config,
        args.registries.subdir.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading registries: {e}");
            return ExitCode::from(1);
        }
    };

    let entry = match skill_index
        .skills
        .get(&(owner.to_string(), name.to_string()))
    {
        Some(e) => e,
        None => {
            eprintln!("Error: skill '{owner}/{name}' not found in any registry");
            return ExitCode::from(1);
        }
    };

    let latest = match entry.latest() {
        Some(v) => v,
        None => {
            eprintln!("Error: no available versions for {owner}/{name} (all yanked)");
            return ExitCode::from(1);
        }
    };

    let info = &latest.metadata.skill;

    println!("{owner}/{name}\n");
    println!("  version ............... {}", info.version);
    println!("  description ........... {}", info.description);

    if let Some(ref trigger) = info.trigger {
        println!("  trigger ............... {trigger}");
    }
    if let Some(ref license) = info.license {
        println!("  license ............... {license}");
    }
    if let Some(ref author) = info.author {
        if let Some(ref name) = author.name {
            println!("  author ................ {name}");
        }
        if let Some(ref github) = author.github {
            println!("  github ................ {github}");
        }
    }
    if let Some(ref classification) = info.classification {
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
    if let Some(ref compat) = info.compatibility
        && !compat.verified_with.is_empty()
    {
        println!(
            "  verified with ......... {}",
            compat.verified_with.join(", ")
        );
    }

    // Extra files
    if !latest.files.is_empty() {
        let mut file_paths: Vec<&String> = latest.files.keys().collect();
        file_paths.sort();
        println!(
            "  files ................. {}",
            file_paths
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Published timestamp
    if let Some(ref published) = latest.published {
        println!("  published ............. {published}");
    }

    // Content hash
    if let Some(ref hash) = latest.content_hash {
        let display = if hash.len() > 17 {
            format!("{}...", &hash[..17])
        } else {
            hash.clone()
        };
        println!("  content hash .......... {display}");
    }

    // Integrity
    match latest.integrity_ok {
        Some(true) => println!("  integrity ............. verified"),
        Some(false) => println!("  integrity ............. MISMATCH"),
        None => {}
    }

    // Version history
    let available: Vec<&str> = entry
        .versions
        .iter()
        .filter(|v| !v.yanked)
        .map(|v| v.version.as_str())
        .collect();
    if available.len() > 1 {
        println!("  versions .............. {}", available.join(", "));
    }

    ExitCode::SUCCESS
}

/// Run the `list` subcommand.
fn run_list(_args: ListArgs) -> ExitCode {
    let installed_manifest = match manifest::load() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading installation manifest: {e}");
            return ExitCode::from(1);
        }
    };

    if installed_manifest.skills.is_empty() {
        println!("No skills installed.");
        return ExitCode::SUCCESS;
    }

    // Group by (owner, name)
    let mut groups: std::collections::BTreeMap<(String, String), Vec<&manifest::InstalledSkill>> =
        std::collections::BTreeMap::new();
    for skill in &installed_manifest.skills {
        groups
            .entry((skill.owner.clone(), skill.name.clone()))
            .or_default()
            .push(skill);
    }

    println!(
        "{} skill{} installed:\n",
        groups.len(),
        if groups.len() == 1 { "" } else { "s" }
    );

    for ((owner, name), entries) in &groups {
        let version = &entries[0].version;
        println!("  {owner}/{name} v{version}");
        for entry in entries {
            println!(
                "    -> {}  ({})",
                entry.installed_to.display(),
                entry.installed_at,
            );
        }
        println!();
    }

    ExitCode::SUCCESS
}

/// Run the `trust` subcommand.
fn run_trust(args: TrustArgs) -> ExitCode {
    match args.action {
        TrustAction::AddRegistry(a) => run_trust_add_registry(a),
        TrustAction::RemoveRegistry(a) => run_trust_remove_registry(a),
        TrustAction::List(a) => run_trust_list(a),
        TrustAction::Pin(a) => run_trust_pin(a),
        TrustAction::Unpin(a) => run_trust_unpin(a),
    }
}

fn run_trust_add_registry(args: TrustAddRegistryArgs) -> ExitCode {
    let mut state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if state.is_trusted(&args.url) {
        println!("Registry already trusted: {}", args.url);
        return ExitCode::SUCCESS;
    }

    state.add_registry(&args.url, args.note.as_deref());

    if let Err(e) = trust::save(&state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    println!("Trusted: {}", args.url);
    ExitCode::SUCCESS
}

fn run_trust_remove_registry(args: TrustRemoveRegistryArgs) -> ExitCode {
    let mut state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if !state.remove_registry(&args.url) {
        eprintln!("Registry not found in trusted list: {}", args.url);
        return ExitCode::from(1);
    }

    if let Err(e) = trust::save(&state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    println!("Removed: {}", args.url);
    ExitCode::SUCCESS
}

fn run_trust_list(args: TrustListArgs) -> ExitCode {
    let state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if state.trusted_registries.is_empty() && state.pinned_skills.is_empty() {
        println!("No trusted registries or pinned skills.");
        return ExitCode::SUCCESS;
    }

    if !state.trusted_registries.is_empty() {
        println!("Trusted registries ({}):\n", state.trusted_registries.len());
        for r in &state.trusted_registries {
            print!("  {}", r.registry);
            if let Some(ref note) = r.note {
                print!("  ({note})");
            }
            println!("  [{}]", r.trusted_at);
        }
    }

    if !args.registries_only && !state.pinned_skills.is_empty() {
        if !state.trusted_registries.is_empty() {
            println!();
        }
        println!("Pinned skills ({}):\n", state.pinned_skills.len());
        for p in &state.pinned_skills {
            let hash_display = if p.content_hash.len() > 17 {
                format!("{}...", &p.content_hash[..17])
            } else {
                p.content_hash.clone()
            };
            println!(
                "  {}/{} v{}  {}  [{}]",
                p.owner, p.name, p.version, hash_display, p.pinned_at
            );
        }
    }

    ExitCode::SUCCESS
}

fn run_trust_pin(args: TrustPinArgs) -> ExitCode {
    let (owner, name) = match parse_skill_ref(&args.skill) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut cli_config = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::from(1);
        }
    };

    if args.registries.no_cache {
        cli_config.cache.enabled = false;
    }

    let (skill_index, registry_paths) = match registry::load_registries(
        &args.registries.registry,
        &args.registries.remote,
        &cli_config,
        args.registries.subdir.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error loading registries: {e}");
            return ExitCode::from(1);
        }
    };

    let entry = match skill_index
        .skills
        .get(&(owner.to_string(), name.to_string()))
    {
        Some(e) => e,
        None => {
            eprintln!("Error: skill '{owner}/{name}' not found in any registry");
            return ExitCode::from(1);
        }
    };

    let version = match entry.latest() {
        Some(v) => v,
        None => {
            eprintln!("Error: no available versions for {owner}/{name} (all yanked)");
            return ExitCode::from(1);
        }
    };

    let registry_id = if !registry_paths.is_empty() {
        registry::registry_id(&registry_paths[0], &args.registries.remote)
    } else {
        "unknown".to_string()
    };

    let content_hash = integrity::sha256_hex(&version.skill_md);

    let mut trust_state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    trust_state.pin_skill(owner, name, &version.version, &registry_id, &content_hash);

    if let Err(e) = trust::save(&trust_state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    let hash_display = if content_hash.len() > 17 {
        format!("{}...", &content_hash[..17])
    } else {
        content_hash.clone()
    };
    println!(
        "Pinned {owner}/{name} v{} ({hash_display})",
        version.version
    );
    ExitCode::SUCCESS
}

fn run_trust_unpin(args: TrustUnpinArgs) -> ExitCode {
    let (owner, name) = match parse_skill_ref(&args.skill) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let mut state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    if !state.unpin_skill(owner, name) {
        eprintln!("Skill '{owner}/{name}' is not pinned.");
        return ExitCode::from(1);
    }

    if let Err(e) = trust::save(&state) {
        eprintln!("Error saving trust state: {e}");
        return ExitCode::from(1);
    }

    println!("Unpinned {owner}/{name}");
    ExitCode::SUCCESS
}

/// Run the `audit` subcommand.
fn run_audit(args: AuditArgs) -> ExitCode {
    let installed = match manifest::load() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading installation manifest: {e}");
            return ExitCode::from(1);
        }
    };

    let trust_state = match trust::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error loading trust state: {e}");
            return ExitCode::from(1);
        }
    };

    let (filter_owner, filter_name) = if let Some(ref skill) = args.skill {
        match parse_skill_ref(skill) {
            Ok((o, n)) => (Some(o), Some(n)),
            Err(e) => {
                eprintln!("Error: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        (None, None)
    };

    let results = trust::audit(&installed, &trust_state, filter_owner, filter_name);

    if results.is_empty() {
        println!("No installed skills to audit.");
        return ExitCode::SUCCESS;
    }

    for r in &results {
        println!(
            "  [{status}] {owner}/{name} v{version} -> {path}",
            status = r.status,
            owner = r.owner,
            name = r.name,
            version = r.version,
            path = r.installed_to.display(),
        );
    }

    if trust::audit_has_problems(&results) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Run the `setup` subcommand.
fn run_setup(args: SetupArgs) -> ExitCode {
    let config_path = config::config_dir().join("config.toml");

    // Check for existing config
    if config_path.exists() && !args.force {
        eprintln!(
            "Config already exists at {}\nUse --force to overwrite.",
            config_path.display()
        );
        return ExitCode::from(1);
    }

    // Build remotes list: official first (unless opted out), then user-provided
    let mut remotes = Vec::new();
    if !args.no_official_registry {
        remotes.push(registry::DEFAULT_REGISTRY_URL.to_string());
    }
    remotes.extend(args.remote);

    let config = match config::generate_default_config(remotes, args.registry, &args.target) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(1);
        }
    };

    let path = match config::write_config(&config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error writing config: {e}");
            return ExitCode::from(1);
        }
    };

    // Read back the written content to display
    let content = std::fs::read_to_string(&path).unwrap_or_default();

    println!("Wrote {}\n", path.display());
    println!("{content}");
    println!(
        "To use skillet with your agent, add this to your MCP config:\n\n\
         {{\n  \"mcpServers\": {{\n    \"skillet\": {{\n      \"command\": \"skillet\"\n    }}\n  }}\n}}"
    );
    println!(
        "\nNext steps:\n  \
         skillet search *            # browse available skills\n  \
         skillet info owner/name     # see skill details\n  \
         skillet install owner/name  # install a skill"
    );

    ExitCode::SUCCESS
}

/// Print a safety report to stdout/stderr.
fn print_safety_report(report: &safety::SafetyReport) {
    let danger_count = report
        .findings
        .iter()
        .filter(|f| f.severity == safety::Severity::Danger)
        .count();
    let warning_count = report
        .findings
        .iter()
        .filter(|f| f.severity == safety::Severity::Warning)
        .count();

    println!("Safety scan: {danger_count} danger, {warning_count} warning\n");

    for f in &report.findings {
        let line_info = match f.line {
            Some(n) => format!("{}:{n}", f.file),
            None => f.file.clone(),
        };
        println!("  [{severity}] {line_info}", severity = f.severity);
        println!("    rule: {}", f.rule_id);
        println!("    {}", f.message);
        println!("    matched: {}", f.matched);
    }
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
            None => target,
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
}
