//! list_categories tool -- browse the category taxonomy

use std::sync::Arc;

use tower_mcp::{
    CallToolResult, NoParams, Tool, ToolBuilder,
    extract::{Json, State},
};

use crate::state::AppState;

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("list_categories")
        .description(
            "List all skill categories with the number of skills in each. \
             Use this to browse what kinds of skills are available.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(_): Json<NoParams>| async move {
                let index = state.index.read().await;

                if index.categories.is_empty() {
                    return Ok(CallToolResult::text(
                        "No categories found. The registry may be empty.",
                    ));
                }

                let mut output =
                    format!("## Skill Categories ({} total)\n\n", index.categories.len());
                for (name, count) in &index.categories {
                    let plural = if *count == 1 { "skill" } else { "skills" };
                    output.push_str(&format!("- **{name}** ({count} {plural})\n"));
                }

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
