//! install_skill tool -- install a skill to the local filesystem

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use skillet_mcp::config::{self, ALL_TARGETS, InstallTarget};
use skillet_mcp::install::{self, InstallOptions};
use skillet_mcp::integrity;
use skillet_mcp::manifest;
use skillet_mcp::safety;
use skillet_mcp::state::AppState;
use skillet_mcp::trust;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InstallSkillInput {
    /// Skill owner (e.g. "joshrotenberg")
    owner: String,
    /// Skill name (e.g. "rust-dev")
    name: String,
    /// Install target: "agents", "claude", "cursor", "copilot", "windsurf", "gemini", or "all"
    /// (default: "agents")
    #[serde(default)]
    target: Option<String>,
    /// Install globally instead of into the current project (default: false)
    #[serde(default)]
    global: Option<bool>,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("install_skill")
        .description(
            "Install a skill to the local filesystem for persistent use. \
             Writes SKILL.md (and any extra files) to the appropriate agent \
             skills directory. Use this when the user wants to install a skill \
             rather than just reading it inline.",
        )
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<InstallSkillInput>| async move {
                let index = state.index.read().await;

                // Look up the skill
                let entry = match index.skills.get(&(input.owner.clone(), input.name.clone())) {
                    Some(e) => e,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "Skill '{}/{}' not found in any registry.",
                            input.owner, input.name
                        )));
                    }
                };

                // Get latest version
                let version = match entry.latest() {
                    Some(v) => v,
                    None => {
                        return Ok(CallToolResult::error(format!(
                            "No available versions for '{}/{}' (all yanked).",
                            input.owner, input.name
                        )));
                    }
                };

                // Resolve target
                let targets = match resolve_mcp_target(input.target.as_deref()) {
                    Ok(t) => t,
                    Err(msg) => return Ok(CallToolResult::error(msg)),
                };

                let global = input.global.unwrap_or(false);

                // Determine registry identifier from state
                let registry_id = if let Some(path) = state.registry_paths.first() {
                    format!("local:{}", path.display())
                } else {
                    "unknown".to_string()
                };

                // Trust checking (warn-only in MCP path, never blocks)
                let content_hash = integrity::sha256_hex(&version.skill_md);
                let cli_config = config::load_config().unwrap_or_default();
                let trust_state = trust::load().unwrap_or_default();
                let trust_check = trust::check_trust(
                    &trust_state,
                    &registry_id,
                    &input.owner,
                    &input.name,
                    &content_hash,
                );

                // Load manifest
                let mut installed_manifest = match manifest::load() {
                    Ok(m) => m,
                    Err(e) => {
                        return Ok(CallToolResult::error(format!(
                            "Failed to load installation manifest: {e}"
                        )));
                    }
                };

                let options = InstallOptions {
                    targets,
                    global,
                    registry: registry_id.clone(),
                };

                // Install
                let results = match install::install_skill(
                    &input.owner,
                    &input.name,
                    version,
                    &options,
                    &mut installed_manifest,
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        return Ok(CallToolResult::error(format!(
                            "Failed to install skill: {e}"
                        )));
                    }
                };

                // Save manifest
                if let Err(e) = manifest::save(&installed_manifest) {
                    return Ok(CallToolResult::error(format!(
                        "Skill files written but failed to save manifest: {e}"
                    )));
                }

                // Auto-pin content hash after successful install
                if cli_config.trust.auto_pin {
                    let mut trust_state = trust_state;
                    trust_state.pin_skill(
                        &input.owner,
                        &input.name,
                        &version.version,
                        &registry_id,
                        &content_hash,
                    );
                    let _ = trust::save(&trust_state);
                }

                // Build response
                let scope = if global { "global" } else { "project" };
                let mut output = format!(
                    "Installed {}/{} v{} ({scope}):\n\n",
                    input.owner, input.name, version.version,
                );
                for r in &results {
                    output.push_str(&format!(
                        "- **{}**: `{}` ({} file{})\n",
                        r.target,
                        r.path.display(),
                        r.files_written.len(),
                        if r.files_written.len() == 1 { "" } else { "s" },
                    ));
                }

                output.push_str(
                    "\nThe skill is now installed. \
                     A restart may be required for the agent to pick it up.",
                );

                // Trust info
                output.push_str(&format!(
                    "\n\n**Trust**: {} ({})",
                    trust_check.tier, trust_check.reason
                ));
                if trust_check.tier == trust::TrustTier::Reviewed
                    && trust_check.pinned_hash.as_deref() != Some(&content_hash)
                {
                    output.push_str("\n**Warning**: content changed since pinned");
                }
                let report = safety::scan(
                    &version.skill_md,
                    &version.skill_toml_raw,
                    &version.files,
                    &version.metadata,
                    &cli_config.safety.suppress,
                );

                if !report.is_empty() {
                    let danger_count = report
                        .findings
                        .iter()
                        .filter(|f| f.severity == safety::Severity::Danger)
                        .count();
                    let warning_count = report
                        .findings
                        .iter()
                        .filter(|f| f.severity == safety::Severity::Warning)
                        .count();

                    output.push_str(&format!(
                        "\n\n**Safety scan**: {danger_count} danger, {warning_count} warning\n"
                    ));

                    for f in &report.findings {
                        let line_info = match f.line {
                            Some(n) => format!("{}:{n}", f.file),
                            None => f.file.clone(),
                        };
                        output.push_str(&format!(
                            "\n- **[{severity}]** {line_info}: {msg} (`{matched}`)",
                            severity = f.severity,
                            msg = f.message,
                            matched = f.matched,
                        ));
                    }
                }

                Ok(CallToolResult::text(output))
            },
        )
        .build()
}

/// Resolve a target string from MCP input to a list of InstallTargets.
fn resolve_mcp_target(target: Option<&str>) -> Result<Vec<InstallTarget>, String> {
    let target_str = target.unwrap_or("agents");
    match InstallTarget::parse(target_str) {
        Ok(Some(t)) => Ok(vec![t]),
        Ok(None) => Ok(ALL_TARGETS.to_vec()),
        Err(e) => Err(e.to_string()),
    }
}
