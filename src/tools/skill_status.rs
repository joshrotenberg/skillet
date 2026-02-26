//! skill_status tool -- show installed skill status with version and integrity info

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use skillet_mcp::manifest;
use skillet_mcp::state::AppState;
use skillet_mcp::trust;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillStatusInput {
    /// Optional owner filter (e.g. "joshrotenberg"). If omitted, shows all installed skills.
    #[serde(default)]
    owner: Option<String>,
    /// Optional skill name filter (e.g. "rust-dev"). If omitted, shows all by the owner (or all).
    #[serde(default)]
    name: Option<String>,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("skill_status")
        .description(
            "Show status of installed skills including version, install location, \
             integrity check, trust status, and whether updates are available \
             from the registry.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<SkillStatusInput>| async move {
                let installed = manifest::load().unwrap_or_default();
                let trust_state = trust::load().unwrap_or_default();
                let index = state.index.read().await;

                // Filter installed skills
                let skills: Vec<&manifest::InstalledSkill> = installed
                    .skills
                    .iter()
                    .filter(|s| {
                        if let Some(ref owner) = input.owner
                            && s.owner != *owner
                        {
                            return false;
                        }
                        if let Some(ref name) = input.name
                            && s.name != *name
                        {
                            return false;
                        }
                        true
                    })
                    .collect();

                if skills.is_empty() {
                    let msg = match (&input.owner, &input.name) {
                        (Some(o), Some(n)) => {
                            format!("No installed skill matching '{o}/{n}'.")
                        }
                        (Some(o), None) => {
                            format!("No installed skills matching owner '{o}'.")
                        }
                        _ => "No skills installed.".to_string(),
                    };
                    return Ok(CallToolResult::text(msg));
                }

                let mut output = format!("## Installed Skills ({})\n\n", skills.len());

                for skill in &skills {
                    output.push_str(&format!(
                        "### {}/{} (v{})\n",
                        skill.owner, skill.name, skill.version
                    ));
                    output.push_str(&format!(
                        "**Installed to:** `{}`\n",
                        skill.installed_to.display()
                    ));
                    output.push_str(&format!("**Installed at:** {}\n", skill.installed_at));

                    // Integrity check
                    let integrity = manifest::InstalledManifest::check_integrity(skill);
                    let integrity_label = match integrity {
                        manifest::IntegrityStatus::Ok => "ok",
                        manifest::IntegrityStatus::Modified => "MODIFIED",
                        manifest::IntegrityStatus::Missing => "MISSING",
                    };
                    output.push_str(&format!("**Integrity:** {integrity_label}\n"));

                    // Trust/pin status
                    if let Some(pin) = trust_state.find_pin(&skill.owner, &skill.name) {
                        output.push_str(&format!(
                            "**Pinned:** v{} (hash: {}...)\n",
                            pin.version,
                            &pin.content_hash[..pin.content_hash.len().min(17)]
                        ));
                    } else {
                        output.push_str("**Pinned:** no\n");
                    }

                    // Check for registry update
                    if let Some(entry) =
                        index.skills.get(&(skill.owner.clone(), skill.name.clone()))
                        && let Some(latest) = entry.latest()
                    {
                        let registry_version = &latest.metadata.skill.version;
                        if *registry_version != skill.version {
                            output
                                .push_str(&format!("**Update available:** v{registry_version}\n"));
                        } else {
                            output.push_str("**Up to date:** yes\n");
                        }
                    }

                    output.push('\n');
                }

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
