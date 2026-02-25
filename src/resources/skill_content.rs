//! Resource template for skill content (SKILL.md)
//!
//! Exposes skill prompts via URI template: skillet://skills/{owner}/{name}

use std::collections::HashMap;
use std::sync::Arc;

use tower_mcp::protocol::{ReadResourceResult, ResourceContent};
use tower_mcp::resource::{ResourceTemplate, ResourceTemplateBuilder};

use skillet_mcp::state::AppState;

/// Build the skill content resource template.
///
/// URI: `skillet://skills/{owner}/{name}`
/// Returns the SKILL.md content for the latest version of the skill.
pub fn build(state: Arc<AppState>) -> ResourceTemplate {
    ResourceTemplateBuilder::new("skillet://skills/{owner}/{name}")
        .name("Skill Content")
        .description("Get a skill's SKILL.md prompt content (latest version)")
        .mime_type("text/markdown")
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
                        mime_type: Some("text/markdown".to_string()),
                        text: Some(version.skill_md.clone()),
                        blob: None,
                        meta: None,
                    }],
                    meta: None,
                })
            }
        })
}

/// Build the versioned skill content resource template.
///
/// URI: `skillet://skills/{owner}/{name}/{version}`
/// Returns the SKILL.md content for a specific version of the skill.
pub fn build_versioned(state: Arc<AppState>) -> ResourceTemplate {
    ResourceTemplateBuilder::new("skillet://skills/{owner}/{name}/{version}")
        .name("Skill Content (Versioned)")
        .description("Get a skill's SKILL.md prompt content for a specific version")
        .mime_type("text/markdown")
        .handler(move |uri: String, vars: HashMap<String, String>| {
            let state = state.clone();
            async move {
                let owner = vars.get("owner").cloned().unwrap_or_default();
                let name = vars.get("name").cloned().unwrap_or_default();
                let version = vars.get("version").cloned().unwrap_or_default();

                let index = state.index.read().await;
                let entry = index
                    .skills
                    .get(&(owner.clone(), name.clone()))
                    .ok_or_else(|| {
                        tower_mcp::Error::tool(format!("Skill '{owner}/{name}' not found"))
                    })?;

                let skill_version = entry
                    .versions
                    .iter()
                    .find(|v| v.version == version)
                    .ok_or_else(|| {
                        let available: Vec<&str> =
                            entry.versions.iter().map(|v| v.version.as_str()).collect();
                        tower_mcp::Error::tool(format!(
                            "Version '{version}' not found for '{owner}/{name}'. \
                             Available versions: {}",
                            available.join(", ")
                        ))
                    })?;

                if !skill_version.has_content {
                    let latest_ver = entry
                        .latest()
                        .map(|v| v.version.as_str())
                        .unwrap_or("unknown");
                    return Err(tower_mcp::Error::tool(format!(
                        "Content for '{owner}/{name}' v{version} is not available. \
                         Historical version content is stored in git history. \
                         Use the latest version (v{latest_ver}) for full content.",
                    )));
                }

                Ok(ReadResourceResult {
                    contents: vec![ResourceContent {
                        uri,
                        mime_type: Some("text/markdown".to_string()),
                        text: Some(skill_version.skill_md.clone()),
                        blob: None,
                        meta: None,
                    }],
                    meta: None,
                })
            }
        })
}
