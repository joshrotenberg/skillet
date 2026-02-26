//! setup_config tool -- generate initial skillet configuration

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use skillet_mcp::config;
use skillet_mcp::registry;
use skillet_mcp::state::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetupConfigInput {
    /// Default install target (default: "agents"). Options: agents, claude, cursor, copilot, windsurf, gemini, all
    #[serde(default)]
    target: Option<String>,
    /// Additional remote registry URLs to add beyond the official registry
    #[serde(default)]
    remotes: Vec<String>,
    /// Overwrite existing config if present (default: false)
    #[serde(default)]
    force: Option<bool>,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("setup_config")
        .description(
            "Generate initial skillet configuration at ~/.config/skillet/config.toml. \
             Sets up the official registry, default install target, and standard \
             defaults. Use this to help users get started with skillet.",
        )
        .extractor_handler(
            state,
            |State(_state): State<Arc<AppState>>, Json(input): Json<SetupConfigInput>| async move {
                let config_path = config::config_dir().join("config.toml");

                if config_path.exists() && !input.force.unwrap_or(false) {
                    return Ok(CallToolResult::text(format!(
                        "Config already exists at `{}`.\n\n\
                         Pass `force: true` to overwrite, or edit the file directly.",
                        config_path.display()
                    )));
                }

                let target = input.target.as_deref().unwrap_or("agents");

                // Build remotes: official + user-provided
                let mut remotes = vec![registry::DEFAULT_REGISTRY_URL.to_string()];
                remotes.extend(input.remotes);

                let generated = match config::generate_default_config(remotes, vec![], target) {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(CallToolResult::error(format!("Invalid configuration: {e}")));
                    }
                };

                let path = match config::write_config(&generated) {
                    Ok(p) => p,
                    Err(e) => {
                        return Ok(CallToolResult::error(format!(
                            "Failed to write config: {e}"
                        )));
                    }
                };

                let content = std::fs::read_to_string(&path).unwrap_or_default();

                Ok(CallToolResult::text(format!(
                    "Wrote config to `{}`\n\n\
                     ```toml\n{content}```\n\n\
                     Skillet is now configured. Skills will be installed to \
                     `.{target}/skills/` by default.",
                    path.display(),
                )))
            },
        )
        .build()
}
