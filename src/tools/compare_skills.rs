//! compare_skills tool -- compare two skills side-by-side

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use skillet_mcp::state::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompareSkillsInput {
    /// First skill owner (e.g. "joshrotenberg")
    owner_a: String,
    /// First skill name (e.g. "rust-dev")
    name_a: String,
    /// Second skill owner (e.g. "acme")
    owner_b: String,
    /// Second skill name (e.g. "python-dev")
    name_b: String,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("compare_skills")
        .description(
            "Compare two skills side-by-side showing differences in description, \
             categories, tags, files, and content.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<CompareSkillsInput>| async move {
                let index = state.index.read().await;

                let label_a = format!("{}/{}", input.owner_a, input.name_a);
                let label_b = format!("{}/{}", input.owner_b, input.name_b);

                let entry_a = match index
                    .skills
                    .get(&(input.owner_a.clone(), input.name_a.clone()))
                {
                    Some(e) => e,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "Skill '{label_a}' not found in any registry."
                        )));
                    }
                };
                let entry_b = match index
                    .skills
                    .get(&(input.owner_b.clone(), input.name_b.clone()))
                {
                    Some(e) => e,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "Skill '{label_b}' not found in any registry."
                        )));
                    }
                };

                let ver_a = match entry_a.latest() {
                    Some(v) => v,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "No available versions for '{label_a}' (all yanked)."
                        )));
                    }
                };
                let ver_b = match entry_b.latest() {
                    Some(v) => v,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "No available versions for '{label_b}' (all yanked)."
                        )));
                    }
                };

                let info_a = &ver_a.metadata.skill;
                let info_b = &ver_b.metadata.skill;

                let mut output = format!("## Comparison: {label_a} vs {label_b}\n\n");

                // Version
                output.push_str(&format!("| | {label_a} | {label_b} |\n|---|---|---|\n"));
                output.push_str(&format!(
                    "| **Version** | {} | {} |\n",
                    info_a.version, info_b.version
                ));

                // Description
                output.push_str(&format!(
                    "| **Description** | {} | {} |\n",
                    info_a.description, info_b.description
                ));

                // License
                let lic_a = info_a.license.as_deref().unwrap_or("-");
                let lic_b = info_b.license.as_deref().unwrap_or("-");
                output.push_str(&format!("| **License** | {lic_a} | {lic_b} |\n"));

                // Categories
                let cats_a = info_a
                    .classification
                    .as_ref()
                    .map(|c| c.categories.join(", "))
                    .unwrap_or_default();
                let cats_b = info_b
                    .classification
                    .as_ref()
                    .map(|c| c.categories.join(", "))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "| **Categories** | {} | {} |\n",
                    if cats_a.is_empty() { "-" } else { &cats_a },
                    if cats_b.is_empty() { "-" } else { &cats_b },
                ));

                // Tags
                let tags_a = info_a
                    .classification
                    .as_ref()
                    .map(|c| c.tags.join(", "))
                    .unwrap_or_default();
                let tags_b = info_b
                    .classification
                    .as_ref()
                    .map(|c| c.tags.join(", "))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "| **Tags** | {} | {} |\n",
                    if tags_a.is_empty() { "-" } else { &tags_a },
                    if tags_b.is_empty() { "-" } else { &tags_b },
                ));

                // Files
                let files_a: Vec<&str> = {
                    let mut keys: Vec<&str> = ver_a.files.keys().map(|s| s.as_str()).collect();
                    keys.sort();
                    keys
                };
                let files_b: Vec<&str> = {
                    let mut keys: Vec<&str> = ver_b.files.keys().map(|s| s.as_str()).collect();
                    keys.sort();
                    keys
                };
                output.push_str(&format!(
                    "| **Files** | {} | {} |\n",
                    if files_a.is_empty() {
                        "-".to_string()
                    } else {
                        files_a.join(", ")
                    },
                    if files_b.is_empty() {
                        "-".to_string()
                    } else {
                        files_b.join(", ")
                    },
                ));

                output.push('\n');

                // Shared categories and tags
                let cats_set_a: std::collections::HashSet<&str> = info_a
                    .classification
                    .as_ref()
                    .map(|c| c.categories.iter().map(|s| s.as_str()).collect())
                    .unwrap_or_default();
                let cats_set_b: std::collections::HashSet<&str> = info_b
                    .classification
                    .as_ref()
                    .map(|c| c.categories.iter().map(|s| s.as_str()).collect())
                    .unwrap_or_default();
                let shared_cats: Vec<&str> = {
                    let mut v: Vec<&str> = cats_set_a.intersection(&cats_set_b).copied().collect();
                    v.sort();
                    v
                };
                let tags_set_a: std::collections::HashSet<&str> = info_a
                    .classification
                    .as_ref()
                    .map(|c| c.tags.iter().map(|s| s.as_str()).collect())
                    .unwrap_or_default();
                let tags_set_b: std::collections::HashSet<&str> = info_b
                    .classification
                    .as_ref()
                    .map(|c| c.tags.iter().map(|s| s.as_str()).collect())
                    .unwrap_or_default();
                let shared_tags: Vec<&str> = {
                    let mut v: Vec<&str> = tags_set_a.intersection(&tags_set_b).copied().collect();
                    v.sort();
                    v
                };

                if !shared_cats.is_empty() || !shared_tags.is_empty() {
                    output.push_str("### Overlap\n\n");
                    if !shared_cats.is_empty() {
                        output.push_str(&format!(
                            "**Shared categories:** {}\n",
                            shared_cats.join(", ")
                        ));
                    }
                    if !shared_tags.is_empty() {
                        output.push_str(&format!("**Shared tags:** {}\n", shared_tags.join(", ")));
                    }
                    output.push('\n');
                }

                // Unique categories and tags
                let unique_cats_a: Vec<&str> = {
                    let mut v: Vec<&str> = cats_set_a.difference(&cats_set_b).copied().collect();
                    v.sort();
                    v
                };
                let unique_cats_b: Vec<&str> = {
                    let mut v: Vec<&str> = cats_set_b.difference(&cats_set_a).copied().collect();
                    v.sort();
                    v
                };
                let unique_tags_a: Vec<&str> = {
                    let mut v: Vec<&str> = tags_set_a.difference(&tags_set_b).copied().collect();
                    v.sort();
                    v
                };
                let unique_tags_b: Vec<&str> = {
                    let mut v: Vec<&str> = tags_set_b.difference(&tags_set_a).copied().collect();
                    v.sort();
                    v
                };

                let has_unique = !unique_cats_a.is_empty()
                    || !unique_cats_b.is_empty()
                    || !unique_tags_a.is_empty()
                    || !unique_tags_b.is_empty();

                if has_unique {
                    output.push_str("### Unique\n\n");
                    if !unique_cats_a.is_empty() {
                        output.push_str(&format!(
                            "**{label_a} categories:** {}\n",
                            unique_cats_a.join(", ")
                        ));
                    }
                    if !unique_cats_b.is_empty() {
                        output.push_str(&format!(
                            "**{label_b} categories:** {}\n",
                            unique_cats_b.join(", ")
                        ));
                    }
                    if !unique_tags_a.is_empty() {
                        output.push_str(&format!(
                            "**{label_a} tags:** {}\n",
                            unique_tags_a.join(", ")
                        ));
                    }
                    if !unique_tags_b.is_empty() {
                        output.push_str(&format!(
                            "**{label_b} tags:** {}\n",
                            unique_tags_b.join(", ")
                        ));
                    }
                    output.push('\n');
                }

                // Content size comparison
                let len_a = ver_a.skill_md.len();
                let len_b = ver_b.skill_md.len();
                output.push_str("### Content\n\n");
                output.push_str(&format!(
                    "**{label_a}:** {len_a} bytes\n**{label_b}:** {len_b} bytes\n",
                ));

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
