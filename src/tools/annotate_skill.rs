//! annotate_skill tool -- attach persistent notes to skills

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{CallToolResult, Tool, ToolBuilder};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnnotateSkillInput {
    /// Skill owner (e.g. "redis")
    owner: String,
    /// Skill name (e.g. "redis-development")
    name: String,
    /// Note to attach to the skill
    note: String,
}

pub fn build() -> Tool {
    ToolBuilder::new("annotate_skill")
        .description(
            "Attach a persistent note to a skill. Notes survive across sessions \
             and are shown in skill info. Use this to record gaps, tips, or \
             corrections discovered during skill use.",
        )
        .handler(|input: AnnotateSkillInput| async move {
            match skillet_mcp::annotations::annotate(&input.owner, &input.name, &input.note) {
                Ok(count) => Ok(CallToolResult::text(format!(
                    "Annotated {}/{}. Total annotations: {count}",
                    input.owner, input.name
                ))),
                Err(e) => Ok(CallToolResult::error(format!(
                    "Failed to save annotation: {e}"
                ))),
            }
        })
        .build()
}
