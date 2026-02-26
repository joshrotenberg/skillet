//! audit_skills tool -- verify installed skills against pinned content hashes

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
pub struct AuditSkillsInput {
    /// Optional owner to audit (e.g. "joshrotenberg"). Audits all if omitted.
    #[serde(default)]
    owner: Option<String>,
    /// Optional skill name to audit (e.g. "rust-dev"). Requires owner.
    #[serde(default)]
    name: Option<String>,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("audit_skills")
        .description(
            "Audit installed skills against pinned content hashes. \
             Checks that installed skill files haven't been modified since \
             installation. Returns per-skill status: OK, MODIFIED, MISSING, \
             or UNPINNED.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(_state): State<Arc<AppState>>, Json(input): Json<AuditSkillsInput>| async move {
                let installed = match manifest::load() {
                    Ok(m) => m,
                    Err(e) => {
                        return Ok(CallToolResult::error(format!(
                            "Failed to load installation manifest: {e}"
                        )));
                    }
                };

                let trust_state = match trust::load() {
                    Ok(s) => s,
                    Err(e) => {
                        return Ok(CallToolResult::error(format!(
                            "Failed to load trust state: {e}"
                        )));
                    }
                };

                let results = trust::audit(
                    &installed,
                    &trust_state,
                    input.owner.as_deref(),
                    input.name.as_deref(),
                );

                if results.is_empty() {
                    return Ok(CallToolResult::text(
                        "No installed skills to audit.\n\n\
                         Install skills with install_skill, then audit to verify integrity.",
                    ));
                }

                let mut output = format!(
                    "## Audit Results ({} skill{})\n\n",
                    results.len(),
                    if results.len() == 1 { "" } else { "s" }
                );

                let mut has_problems = false;
                for r in &results {
                    if matches!(
                        r.status,
                        trust::AuditStatus::Modified | trust::AuditStatus::Missing
                    ) {
                        has_problems = true;
                    }
                    output.push_str(&format!(
                        "- **[{}]** {}/{} v{} -> `{}`\n",
                        r.status,
                        r.owner,
                        r.name,
                        r.version,
                        r.installed_to.display(),
                    ));
                }

                if has_problems {
                    output.push_str(
                        "\nSome skills have integrity issues. Consider re-installing \
                         affected skills or investigating the changes.",
                    );
                } else {
                    output.push_str("\nAll audited skills passed integrity checks.");
                }

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
