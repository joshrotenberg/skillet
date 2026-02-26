//! validate_skill tool -- validate a skillpack directory

use std::path::PathBuf;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::{
    CallToolResult, Tool, ToolBuilder,
    extract::{Json, State},
};

use skillet_mcp::config;
use skillet_mcp::safety;
use skillet_mcp::state::AppState;
use skillet_mcp::validate;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateSkillInput {
    /// Path to the skillpack directory to validate
    path: String,
    /// Skip safety scanning (default: false)
    #[serde(default)]
    skip_safety: Option<bool>,
}

pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("validate_skill")
        .description(
            "Validate a skillpack directory for correctness and safety. \
             Checks skill.toml structure, SKILL.md presence, metadata fields, \
             content hashes, and runs safety scanning. Use this when authoring \
             or reviewing skills.",
        )
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(_state): State<Arc<AppState>>, Json(input): Json<ValidateSkillInput>| async move {
                let path = PathBuf::from(&input.path);

                let result = match validate::validate_skillpack(&path) {
                    Ok(r) => r,
                    Err(e) => {
                        return Ok(CallToolResult::error(format!(
                            "Validation error: {e}"
                        )));
                    }
                };

                let mut output = format!("## Validation: {}/{}\n\n", result.owner, result.name);
                output.push_str("- **skill.toml**: ok\n");
                output.push_str(&format!(
                    "- **SKILL.md**: ok ({} lines)\n",
                    result.skill_md.lines().count()
                ));
                output.push_str(&format!("- **version**: {}\n", result.version));
                output.push_str(&format!("- **description**: {}\n", result.description));

                if let Some(ref classification) = result.metadata.skill.classification {
                    if !classification.categories.is_empty() {
                        output.push_str(&format!(
                            "- **categories**: {}\n",
                            classification.categories.join(", ")
                        ));
                    }
                    if !classification.tags.is_empty() {
                        output.push_str(&format!(
                            "- **tags**: {}\n",
                            classification.tags.join(", ")
                        ));
                    }
                }

                if !result.files.is_empty() {
                    let mut file_paths: Vec<&String> = result.files.keys().collect();
                    file_paths.sort();
                    output.push_str(&format!(
                        "- **extra files**: {}\n",
                        file_paths
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }

                // Content hash
                let hash_display = if result.hashes.composite.len() > 17 {
                    format!("{}...", &result.hashes.composite[..17])
                } else {
                    result.hashes.composite.clone()
                };
                output.push_str(&format!("- **content hash**: {hash_display}\n"));

                // Manifest status
                match result.manifest_ok {
                    Some(true) => output.push_str("- **manifest**: verified\n"),
                    Some(false) => output.push_str("- **manifest**: MISMATCH\n"),
                    None => output.push_str("- **manifest**: not found (generated on pack)\n"),
                }

                // Warnings
                if !result.warnings.is_empty() {
                    output.push('\n');
                    for w in &result.warnings {
                        output.push_str(&format!("**Warning**: {w}\n"));
                    }
                }

                // Safety scanning
                if !input.skip_safety.unwrap_or(false) {
                    let cli_config = config::load_config().unwrap_or_default();
                    let report = safety::scan(
                        &result.skill_md,
                        &result.skill_toml_raw,
                        &result.files,
                        &result.metadata,
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
                            "\n**Safety scan**: {danger_count} danger, {warning_count} warning\n"
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

                        if report.has_danger() {
                            output.push_str(
                                "\n\nValidation failed: safety issues detected. \
                                 Fix danger findings before packing or publishing.",
                            );
                            return Ok(CallToolResult::text(output));
                        }
                    }
                }

                output.push_str("\n\nValidation passed.");
                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
