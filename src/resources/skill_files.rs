//! Resource template for skillpack files (scripts/, references/, assets/)
//!
//! Exposes extra files via URI template: skillet://files/{owner}/{name}/{path}

use std::collections::HashMap;
use std::sync::Arc;

use tower_mcp::protocol::{ReadResourceResult, ResourceContent};
use tower_mcp::resource::{ResourceTemplate, ResourceTemplateBuilder};

use skillet_mcp::state::AppState;

/// Build the skillpack files resource template.
///
/// URI: `skillet://files/{owner}/{name}/{path}`
/// Returns content of a file from the skillpack (scripts/, references/, assets/).
pub fn build(state: Arc<AppState>) -> ResourceTemplate {
    ResourceTemplateBuilder::new("skillet://files/{owner}/{name}/{+path}")
        .name("Skillpack File")
        .description(
            "Get a file from a skillpack (scripts, references, or assets). \
             Use the file paths shown in search results or metadata.",
        )
        .handler(move |uri: String, vars: HashMap<String, String>| {
            let state = state.clone();
            async move {
                let owner = vars.get("owner").cloned().unwrap_or_default();
                let name = vars.get("name").cloned().unwrap_or_default();
                let path = vars.get("path").cloned().unwrap_or_default();

                let index = state.index.read().await;
                let entry = index
                    .skills
                    .get(&(owner.clone(), name.clone()))
                    .ok_or_else(|| {
                        tower_mcp::Error::tool(format!("Skill '{owner}/{name}' not found"))
                    })?;

                let version = entry.latest().ok_or_else(|| {
                    tower_mcp::Error::tool(format!("No published versions for '{owner}/{name}'"))
                })?;

                let file = version.files.get(&path).ok_or_else(|| {
                    let available: Vec<&String> = version.files.keys().collect();
                    if available.is_empty() {
                        tower_mcp::Error::tool(format!(
                            "No extra files in '{owner}/{name}'. This skillpack contains only SKILL.md and skill.toml."
                        ))
                    } else {
                        tower_mcp::Error::tool(format!(
                            "File '{path}' not found in '{owner}/{name}'. Available files: {}",
                            available
                                .iter()
                                .map(|f| f.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    }
                })?;

                Ok(ReadResourceResult {
                    contents: vec![ResourceContent {
                        uri,
                        mime_type: Some(file.mime_type.clone()),
                        text: Some(file.content.clone()),
                        blob: None,
                        meta: None,
                    }],
                    meta: None,
                })
            }
        })
}
