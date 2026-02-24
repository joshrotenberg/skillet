//! Resource template for skill metadata (skill.toml)
//!
//! Exposes skill metadata via URI template: skillet://metadata/{owner}/{name}

use std::collections::HashMap;
use std::sync::Arc;

use tower_mcp::protocol::{ReadResourceResult, ResourceContent};
use tower_mcp::resource::{ResourceTemplate, ResourceTemplateBuilder};

use crate::state::AppState;

/// Build the skill metadata resource template.
///
/// URI: `skillet://metadata/{owner}/{name}`
/// Returns the raw skill.toml content.
pub fn build(state: Arc<AppState>) -> ResourceTemplate {
    ResourceTemplateBuilder::new("skillet://metadata/{owner}/{name}")
        .name("Skill Metadata")
        .description("Get a skill's full metadata (skill.toml)")
        .mime_type("application/toml")
        .handler(move |uri: String, vars: HashMap<String, String>| {
            let state = state.clone();
            async move {
                let owner = vars.get("owner").cloned().unwrap_or_default();
                let name = vars.get("name").cloned().unwrap_or_default();

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

                Ok(ReadResourceResult {
                    contents: vec![ResourceContent {
                        uri,
                        mime_type: Some("application/toml".to_string()),
                        text: Some(version.skill_toml_raw.clone()),
                        blob: None,
                        meta: None,
                    }],
                    meta: None,
                })
            }
        })
}
