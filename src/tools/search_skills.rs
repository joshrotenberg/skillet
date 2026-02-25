//! search_skills tool -- full-text search over the skill index

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use crate::state::{AppState, SkillSummary};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSkillsInput {
    /// Search query (matches against skill name, description, tags, and categories)
    query: String,
    /// Filter by category (e.g. "development", "testing")
    #[serde(default)]
    category: Option<String>,
    /// Filter by tag (e.g. "rust", "python")
    #[serde(default)]
    tag: Option<String>,
    /// Filter to skills verified with a specific model (e.g. "claude-opus-4-6")
    #[serde(default)]
    verified_with: Option<String>,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("search_skills")
        .description(
            "Search the skill registry. Returns skills matching the query, \
             with optional filters for category, tag, or model compatibility. \
             Use this to discover skills relevant to your current task.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<SearchSkillsInput>| async move {
                let index = state.index.read().await;
                let query_lower = input.query.to_lowercase();

                let mut results: Vec<SkillSummary> = index
                    .skills
                    .values()
                    .filter_map(|entry| {
                        let summary = SkillSummary::from_entry(entry)?;

                        // Category filter
                        if let Some(ref cat) = input.category {
                            let cat_lower = cat.to_lowercase();
                            if !summary
                                .categories
                                .iter()
                                .any(|c| c.to_lowercase() == cat_lower)
                            {
                                return None;
                            }
                        }

                        // Tag filter
                        if let Some(ref tag) = input.tag {
                            let tag_lower = tag.to_lowercase();
                            if !summary.tags.iter().any(|t| t.to_lowercase() == tag_lower) {
                                return None;
                            }
                        }

                        // Verified-with filter
                        if let Some(ref model) = input.verified_with {
                            let model_lower = model.to_lowercase();
                            if !summary
                                .verified_with
                                .iter()
                                .any(|v| v.to_lowercase() == model_lower)
                            {
                                return None;
                            }
                        }

                        // Text search across name, description, tags, categories, trigger
                        if query_lower != "*" {
                            let searchable = format!(
                                "{} {} {} {} {} {}",
                                summary.owner,
                                summary.name,
                                summary.description,
                                summary.trigger.as_deref().unwrap_or(""),
                                summary.categories.join(" "),
                                summary.tags.join(" "),
                            )
                            .to_lowercase();

                            let matches = query_lower
                                .split_whitespace()
                                .any(|term| searchable.contains(term));

                            if !matches {
                                return None;
                            }
                        }

                        Some(summary)
                    })
                    .collect();

                results.sort_by(|a, b| a.owner.cmp(&b.owner).then_with(|| a.name.cmp(&b.name)));

                if results.is_empty() {
                    return Ok(CallToolResult::text(format!(
                        "No skills found matching '{}'.",
                        input.query
                    )));
                }

                let mut output = format!("Found {} skill(s):\n\n", results.len());
                for s in &results {
                    let version_info = if s.version_count > 1 {
                        format!("v{}, {} versions", s.version, s.version_count)
                    } else {
                        format!("v{}", s.version)
                    };
                    output.push_str(&format!(
                        "## {}/{} ({})\n{}\n",
                        s.owner, s.name, version_info, s.description,
                    ));
                    if let Some(ref trigger) = s.trigger {
                        output.push_str(&format!("**When to use:** {trigger}\n"));
                    }
                    if !s.categories.is_empty() {
                        output.push_str(&format!("**Categories:** {}\n", s.categories.join(", ")));
                    }
                    if !s.tags.is_empty() {
                        output.push_str(&format!("**Tags:** {}\n", s.tags.join(", ")));
                    }
                    if !s.verified_with.is_empty() {
                        output.push_str(&format!(
                            "**Verified with:** {}\n",
                            s.verified_with.join(", ")
                        ));
                    }
                    if !s.files.is_empty() {
                        output.push_str(&format!(
                            "**Files:** {}\n",
                            s.files
                                .iter()
                                .map(|f| format!("`{f}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                    output.push_str(&format!(
                        "**Use:** Read `skillet://skills/{}/{}` to use this skill\n\n",
                        s.owner, s.name
                    ));
                }

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
