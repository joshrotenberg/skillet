//! list_installed tool -- list all installed skills

use std::collections::BTreeMap;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use skillet_mcp::manifest;
use skillet_mcp::state::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListInstalledInput {
    /// Optional owner filter (e.g. "joshrotenberg")
    #[serde(default)]
    owner: Option<String>,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("list_installed")
        .description(
            "List all skills currently installed on the local filesystem. \
             Shows skill name, version, install target, path, and timestamp. \
             Use this to see what skills are available before searching for more.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(_state): State<Arc<AppState>>, Json(input): Json<ListInstalledInput>| async move {
                let installed = manifest::load().unwrap_or_default();

                let skills: Vec<&manifest::InstalledSkill> = installed
                    .skills
                    .iter()
                    .filter(|s| {
                        if let Some(ref owner) = input.owner
                            && s.owner != *owner
                        {
                            return false;
                        }
                        true
                    })
                    .collect();

                if skills.is_empty() {
                    let msg = if let Some(ref owner) = input.owner {
                        format!(
                            "No installed skills from '{owner}'.\n\n\
                             Use search_skills to discover skills to install."
                        )
                    } else {
                        "No skills installed.\n\n\
                         Use search_skills to discover skills, then install_skill to install them."
                            .to_string()
                    };
                    return Ok(CallToolResult::text(msg));
                }

                // Group by (owner, name) for cleaner output
                let mut grouped: BTreeMap<(String, String), Vec<&manifest::InstalledSkill>> =
                    BTreeMap::new();
                for skill in &skills {
                    grouped
                        .entry((skill.owner.clone(), skill.name.clone()))
                        .or_default()
                        .push(skill);
                }

                let mut output = format!(
                    "## Installed Skills ({} skill{}, {} target{})\n\n",
                    grouped.len(),
                    if grouped.len() == 1 { "" } else { "s" },
                    skills.len(),
                    if skills.len() == 1 { "" } else { "s" },
                );

                for ((owner, name), entries) in &grouped {
                    let version = &entries[0].version;
                    output.push_str(&format!("**{owner}/{name}** v{version}\n"));
                    for entry in entries {
                        output.push_str(&format!(
                            "  - `{}` (installed {})\n",
                            entry.installed_to.display(),
                            entry.installed_at,
                        ));
                    }
                    output.push('\n');
                }

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
