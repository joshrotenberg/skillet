//! info_skill tool -- get detailed information about a specific skill

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use skillet_mcp::state::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InfoSkillInput {
    /// Skill owner (e.g. "joshrotenberg")
    owner: String,
    /// Skill name (e.g. "rust-dev")
    name: String,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("info_skill")
        .description(
            "Get detailed information about a specific skill including version, \
             description, author, categories, tags, files, and version history.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<InfoSkillInput>| async move {
                let index = state.index.read().await;

                let entry = match index.skills.get(&(input.owner.clone(), input.name.clone())) {
                    Some(e) => e,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "Skill '{}/{}' not found in any repo.",
                            input.owner, input.name
                        )));
                    }
                };

                let latest = match entry.latest() {
                    Some(v) => v,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "No available versions for '{}/{}' (all yanked).",
                            input.owner, input.name
                        )));
                    }
                };

                let info = &latest.metadata.skill;
                let mut output = format!("## {}/{}\n\n", input.owner, input.name);

                output.push_str(&format!("**Version:** {}\n", info.version));
                output.push_str(&format!("**Description:** {}\n", info.description));

                if let Some(ref trigger) = info.trigger {
                    output.push_str(&format!("**Trigger:** {trigger}\n"));
                }
                if let Some(ref license) = info.license {
                    output.push_str(&format!("**License:** {license}\n"));
                }
                if let Some(ref author) = info.author {
                    if let Some(ref name) = author.name {
                        output.push_str(&format!("**Author:** {name}\n"));
                    }
                    if let Some(ref github) = author.github {
                        output.push_str(&format!("**GitHub:** {github}\n"));
                    }
                }
                if let Some(ref classification) = info.classification {
                    if !classification.categories.is_empty() {
                        output.push_str(&format!(
                            "**Categories:** {}\n",
                            classification.categories.join(", ")
                        ));
                    }
                    if !classification.tags.is_empty() {
                        output.push_str(&format!("**Tags:** {}\n", classification.tags.join(", ")));
                    }
                }
                if let Some(ref compat) = info.compatibility
                    && !compat.verified_with.is_empty()
                {
                    output.push_str(&format!(
                        "**Verified with:** {}\n",
                        compat.verified_with.join(", ")
                    ));
                }

                // Extra files
                if !latest.files.is_empty() {
                    let mut file_paths: Vec<&String> = latest.files.keys().collect();
                    file_paths.sort();
                    output.push_str(&format!(
                        "**Files:** {}\n",
                        file_paths
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }

                // Published timestamp
                if let Some(ref published) = latest.published {
                    output.push_str(&format!("**Published:** {published}\n"));
                }

                // Version history
                let available: Vec<&str> = entry
                    .versions
                    .iter()
                    .filter(|v| !v.yanked)
                    .map(|v| v.version.as_str())
                    .collect();
                if available.len() > 1 {
                    output.push_str(&format!("**Versions:** {}\n", available.join(", ")));
                }

                // Repo path for nested skills
                if let Some(ref rpath) = entry.repo_path {
                    output.push_str(&format!("**Repo path:** {rpath}\n"));
                }

                // Source label for embedded skills
                if let Some(label) = entry.source.label() {
                    output.push_str(&format!("**Source:** {label}\n"));
                }

                // Trust tier and provenance
                if entry.trust_tier != skillet_mcp::state::TrustTier::Direct {
                    output.push_str(&format!("**Trust:** {}\n", entry.trust_tier));
                    if !entry.discovered_via.is_empty() {
                        output.push_str(&format!(
                            "**Discovered via:** {}\n",
                            entry.discovered_via.join(" -> ")
                        ));
                    }
                }

                // Prompt name for agent use
                output.push_str(&format!("\n**Prompt:** `{}_{}`\n", input.owner, input.name));

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
