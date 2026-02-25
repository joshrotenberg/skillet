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

                let results: Vec<SkillSummary> = if input.query == "*" {
                    // Wildcard: return all skills, apply structured filters only
                    index
                        .skills
                        .values()
                        .filter_map(SkillSummary::from_entry)
                        .collect()
                } else {
                    // BM25 search, then look up summaries
                    let search = state.search.read().await;
                    search
                        .search(&input.query, 100)
                        .into_iter()
                        .filter_map(|(owner, name, _score)| {
                            let entry = index.skills.get(&(owner, name))?;
                            SkillSummary::from_entry(entry)
                        })
                        .collect()
                };

                // Apply structured filters (category, tag, verified_with)
                let results: Vec<SkillSummary> = results
                    .into_iter()
                    .filter(|summary| {
                        if let Some(ref cat) = input.category {
                            let cat_lower = cat.to_lowercase();
                            if !summary
                                .categories
                                .iter()
                                .any(|c| c.to_lowercase() == cat_lower)
                            {
                                return false;
                            }
                        }
                        if let Some(ref tag) = input.tag {
                            let tag_lower = tag.to_lowercase();
                            if !summary.tags.iter().any(|t| t.to_lowercase() == tag_lower) {
                                return false;
                            }
                        }
                        if let Some(ref model) = input.verified_with {
                            let model_lower = model.to_lowercase();
                            if !summary
                                .verified_with
                                .iter()
                                .any(|v| v.to_lowercase() == model_lower)
                            {
                                return false;
                            }
                        }
                        true
                    })
                    .collect();

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
                    if let Some(ref status) = s.integrity {
                        let label = if status == "verified" {
                            "**Integrity:** verified"
                        } else {
                            "**Integrity:** FAILED"
                        };
                        output.push_str(&format!("{label}\n"));
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
