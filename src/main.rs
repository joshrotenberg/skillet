//! Skillet MCP Server
//!
//! An MCP-native skill registry for AI agents. Serves skills from a local
//! registry directory (git checkout) via tools and resource templates.

mod index;
mod resources;
mod state;
mod tools;

use std::path::PathBuf;

use clap::Parser;
use tower_mcp::{McpRouter, StdioTransport};

use crate::state::AppState;

#[derive(Parser, Debug)]
#[command(name = "skillet")]
#[command(about = "MCP-native skill registry for AI agents")]
struct Args {
    /// Path to the registry directory (contains owner/skill-name/ directories)
    #[arg(long, default_value = "test-registry")]
    registry: PathBuf,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
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

    tracing::info!(registry = %args.registry.display(), "Starting skillet server");

    // Load the skill index
    let skill_index = index::load_index(&args.registry)?;
    let state = AppState::new(args.registry, skill_index);

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
        .server_info("skillet", env!("CARGO_PKG_VERSION"))
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
