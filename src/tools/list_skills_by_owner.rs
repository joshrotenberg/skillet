//! list_skills_by_owner tool -- list all skills by a publisher

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use crate::state::{AppState, SkillSummary};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListByOwnerInput {
    /// The owner/publisher name (e.g. "joshrotenberg")
    owner: String,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("list_skills_by_owner")
        .description(
            "List all skills published by a specific owner. \
             Returns skill names, descriptions, and versions.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<ListByOwnerInput>| async move {
                let index = state.index.read().await;
                let owner_lower = input.owner.to_lowercase();

                let mut results: Vec<SkillSummary> = index
                    .skills
                    .values()
                    .filter(|entry| entry.owner.to_lowercase() == owner_lower)
                    .filter_map(SkillSummary::from_entry)
                    .collect();

                results.sort_by(|a, b| a.name.cmp(&b.name));

                if results.is_empty() {
                    return Ok(CallToolResult::text(format!(
                        "No skills found for owner '{}'.",
                        input.owner
                    )));
                }

                let mut output =
                    format!("## Skills by {} ({} total)\n\n", input.owner, results.len());
                for s in &results {
                    output.push_str(&format!(
                        "- **{}** (v{}) -- {}\n",
                        s.name, s.version, s.description,
                    ));
                }

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
