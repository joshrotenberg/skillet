//! Resource templates for the repo catalog.
//!
//! - `skillet://repos/{owner}/{name}` -- single repo entry as JSON
//! - `skillet://repos/` -- list all repos (static resource)

use std::collections::HashMap;
use std::sync::Arc;

use tower_mcp::protocol::{ReadResourceResult, ResourceContent};
use tower_mcp::resource::{Resource, ResourceTemplate, ResourceTemplateBuilder};

use skillet_mcp::state::AppState;

/// Build the single-repo resource template.
///
/// URI: `skillet://repos/{owner}/{name}`
/// Returns metadata for a single repo from the catalog.
pub fn build(state: Arc<AppState>) -> ResourceTemplate {
    ResourceTemplateBuilder::new("skillet://repos/{owner}/{name}")
        .name("Repo Info")
        .description("Get metadata for a curated external skill repo")
        .mime_type("application/json")
        .handler(move |uri: String, vars: HashMap<String, String>| {
            let state = state.clone();
            async move {
                let owner = vars.get("owner").cloned().unwrap_or_default();
                let name = vars.get("name").cloned().unwrap_or_default();
                let full_name = format!("{owner}/{name}");

                let entry = state.repos.find(&full_name).ok_or_else(|| {
                    tower_mcp::Error::tool(format!("Repo '{full_name}' not found in catalog"))
                })?;

                let json = serde_json::to_string_pretty(entry).map_err(|e| {
                    tower_mcp::Error::tool(format!("Failed to serialize repo entry: {e}"))
                })?;

                Ok(ReadResourceResult {
                    contents: vec![ResourceContent {
                        uri,
                        mime_type: Some("application/json".to_string()),
                        text: Some(json),
                        blob: None,
                        meta: None,
                    }],
                    meta: None,
                })
            }
        })
}

/// Build the repo list resource.
///
/// URI: `skillet://repos/`
/// Returns all repos in the catalog as a JSON array.
pub fn build_list(state: Arc<AppState>) -> Resource {
    let json = serde_json::to_string_pretty(state.repos.entries()).unwrap_or_default();
    Resource::builder("skillet://repos/")
        .name("Repo Catalog")
        .description("List all curated external skill repos")
        .mime_type("application/json")
        .text(json)
}
