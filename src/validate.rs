//! Standalone skillpack validation.
//!
//! Validates a skillpack directory (containing `skill.toml` and `SKILL.md`)
//! without requiring a full registry context. Used by `skillet validate` and
//! internally by `index::load_skill()`.

use std::collections::HashMap;
use std::path::Path;

use crate::error::Error;
use crate::index;
use crate::integrity;
use crate::state::{KNOWN_CAPABILITIES, SkillFile, SkillMetadata};

/// Result of validating a skillpack directory.
#[derive(Debug)]
pub struct ValidationResult {
    pub owner: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub metadata: SkillMetadata,
    pub skill_md: String,
    pub skill_toml_raw: String,
    pub files: HashMap<String, SkillFile>,
    pub hashes: integrity::ContentHashes,
    /// `None` = no manifest, `Some(true)` = verified, `Some(false)` = mismatch
    pub manifest_ok: Option<bool>,
    /// Non-fatal issues found during validation
    pub warnings: Vec<String>,
}

/// Validate a skillpack directory.
///
/// Checks that `skill.toml` and `SKILL.md` exist and parse correctly,
/// required fields are present and well-formed, extra files load, and
/// content hashes are computed. If `MANIFEST.sha256` exists, it is verified.
pub fn validate_skillpack(dir: &Path) -> crate::error::Result<ValidationResult> {
    let mut warnings = Vec::new();

    // 1. skill.toml must exist and parse
    let toml_path = dir.join("skill.toml");
    if !toml_path.is_file() {
        return Err(Error::Validation(format!(
            "skill.toml not found in {}",
            dir.display()
        )));
    }

    let skill_toml_raw = std::fs::read_to_string(&toml_path).map_err(|e| Error::FileRead {
        path: toml_path.clone(),
        source: e,
    })?;

    let metadata: SkillMetadata =
        toml::from_str(&skill_toml_raw).map_err(|e| Error::TomlParse {
            path: toml_path.clone(),
            source: e,
        })?;

    // 2. SKILL.md must exist and be non-empty
    let md_path = dir.join("SKILL.md");
    if !md_path.is_file() {
        return Err(Error::Validation(format!(
            "SKILL.md not found in {}",
            dir.display()
        )));
    }

    let skill_md = std::fs::read_to_string(&md_path).map_err(|e| Error::FileRead {
        path: md_path.clone(),
        source: e,
    })?;

    if skill_md.trim().is_empty() {
        return Err(Error::Validation(format!(
            "SKILL.md is empty in {}",
            dir.display()
        )));
    }

    // 3. Required fields: name, owner, version, description
    let info = &metadata.skill;

    if info.name.is_empty() || info.name.contains(char::is_whitespace) {
        return Err(Error::Validation(format!(
            "Invalid skill name '{}': must be non-empty with no whitespace",
            info.name
        )));
    }

    if info.owner.is_empty() || info.owner.contains(char::is_whitespace) {
        return Err(Error::Validation(format!(
            "Invalid owner '{}': must be non-empty with no whitespace",
            info.owner
        )));
    }

    if info.version.is_empty() || info.version.contains(char::is_whitespace) {
        return Err(Error::Validation(format!(
            "Invalid version '{}': must be non-empty with no whitespace",
            info.version
        )));
    }

    if info.description.is_empty() {
        return Err(Error::Validation(
            "Description must not be empty".to_string(),
        ));
    }

    // 4. Load extra files from scripts/, references/, assets/
    let files = index::load_extra_files(dir)?;

    // 5. Compute content hashes
    let hashes = integrity::compute_hashes(&skill_toml_raw, &skill_md, &files);

    // 6. Verify manifest if present
    let manifest_ok = verify_manifest_if_present(dir, &hashes, &mut warnings);

    // 7. Check SKILL.md frontmatter consistency (warning only)
    check_frontmatter_consistency(&skill_md, info, &mut warnings);

    // 8. Warn on unknown capabilities (non-fatal for forward compatibility)
    if let Some(ref compat) = info.compatibility {
        for cap in &compat.required_capabilities {
            if !KNOWN_CAPABILITIES.contains(&cap.as_str()) {
                warnings.push(format!(
                    "Unknown capability '{cap}'. Known capabilities: {}",
                    KNOWN_CAPABILITIES.join(", ")
                ));
            }
        }
    }

    Ok(ValidationResult {
        owner: info.owner.clone(),
        name: info.name.clone(),
        version: info.version.clone(),
        description: info.description.clone(),
        metadata,
        skill_md,
        skill_toml_raw,
        files,
        hashes,
        manifest_ok,
        warnings,
    })
}

/// Read and verify `MANIFEST.sha256` if it exists.
///
/// Returns `None` if no manifest, `Some(true)` if verified, `Some(false)` if
/// mismatches detected (details added to warnings).
fn verify_manifest_if_present(
    dir: &Path,
    computed: &integrity::ContentHashes,
    warnings: &mut Vec<String>,
) -> Option<bool> {
    let manifest_path = dir.join("MANIFEST.sha256");
    if !manifest_path.is_file() {
        return None;
    }

    let raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("Failed to read MANIFEST.sha256: {e}"));
            return None;
        }
    };

    let expected = match integrity::parse_manifest(&raw) {
        Ok(h) => h,
        Err(e) => {
            warnings.push(format!("Failed to parse MANIFEST.sha256: {e}"));
            return None;
        }
    };

    let mismatches = integrity::verify(computed, &expected);
    if mismatches.is_empty() {
        Some(true)
    } else {
        for m in &mismatches {
            warnings.push(format!("Manifest mismatch: {m}"));
        }
        Some(false)
    }
}

/// Check that SKILL.md frontmatter name/description match skill.toml.
fn check_frontmatter_consistency(
    skill_md: &str,
    info: &crate::state::SkillInfo,
    warnings: &mut Vec<String>,
) {
    // Simple frontmatter parser: look for --- delimited block at start
    let trimmed = skill_md.trim_start();
    if !trimmed.starts_with("---") {
        return; // No frontmatter, nothing to check
    }

    let after_first = &trimmed[3..];
    let Some(end) = after_first.find("---") else {
        return; // Unclosed frontmatter
    };

    let frontmatter = &after_first[..end];

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("name:") {
            let fm_name = value.trim().trim_matches('"').trim_matches('\'');
            if fm_name != info.name {
                warnings.push(format!(
                    "SKILL.md frontmatter name '{}' differs from skill.toml name '{}'",
                    fm_name, info.name
                ));
            }
        }
        if let Some(value) = line.strip_prefix("description:") {
            let fm_desc = value.trim().trim_matches('"').trim_matches('\'');
            // Only warn if they differ significantly (not just trigger suffix)
            if !info.description.starts_with(fm_desc)
                && !fm_desc.starts_with(&info.description)
                && fm_desc != info.description
            {
                warnings.push(
                    "SKILL.md frontmatter description differs from skill.toml description"
                        .to_string(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-registry")
    }

    #[test]
    fn test_validate_valid_skill() {
        let dir = test_registry().join("joshrotenberg/rust-dev");
        if !dir.exists() {
            return;
        }
        let result = validate_skillpack(&dir).expect("should validate");
        assert_eq!(result.owner, "joshrotenberg");
        assert_eq!(result.name, "rust-dev");
        assert!(!result.version.is_empty());
        assert!(!result.skill_md.is_empty());
        assert!(!result.hashes.composite.is_empty());
    }

    #[test]
    fn test_validate_skill_with_extra_files() {
        let dir = test_registry().join("joshrotenberg/skillet-dev");
        if !dir.exists() {
            return;
        }
        let result = validate_skillpack(&dir).expect("should validate");
        assert_eq!(result.owner, "joshrotenberg");
        assert_eq!(result.name, "skillet-dev");
        assert!(!result.files.is_empty(), "should have extra files");
    }

    #[test]
    fn test_validate_missing_skill_toml() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("SKILL.md"), "# Hello").unwrap();

        let result = validate_skillpack(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("skill.toml"));
    }

    #[test]
    fn test_validate_missing_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skill.toml"),
            r#"[skill]
name = "test"
owner = "testowner"
version = "1.0"
description = "test skill"
"#,
        )
        .unwrap();

        let result = validate_skillpack(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SKILL.md"));
    }

    #[test]
    fn test_validate_empty_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skill.toml"),
            r#"[skill]
name = "test"
owner = "testowner"
version = "1.0"
description = "test skill"
"#,
        )
        .unwrap();
        std::fs::write(tmp.path().join("SKILL.md"), "  \n  ").unwrap();

        let result = validate_skillpack(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_validate_empty_name() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skill.toml"),
            r#"[skill]
name = ""
owner = "testowner"
version = "1.0"
description = "test"
"#,
        )
        .unwrap();
        std::fs::write(tmp.path().join("SKILL.md"), "# Test").unwrap();

        let result = validate_skillpack(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name"));
    }

    #[test]
    fn test_validate_whitespace_in_version() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("skill.toml"),
            r#"[skill]
name = "test"
owner = "testowner"
version = "1.0 beta"
description = "test"
"#,
        )
        .unwrap();
        std::fs::write(tmp.path().join("SKILL.md"), "# Test").unwrap();

        let result = validate_skillpack(tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn test_validate_with_verified_manifest() {
        let dir = test_registry().join("joshrotenberg/code-review");
        if !dir.exists() {
            return;
        }
        let result = validate_skillpack(&dir).expect("should validate");
        assert_eq!(result.manifest_ok, Some(true));
    }

    #[test]
    fn test_validate_with_mismatched_manifest() {
        let dir = test_registry().join("acme/git-conventions");
        if !dir.exists() {
            return;
        }
        let result = validate_skillpack(&dir).expect("should validate");
        assert_eq!(result.manifest_ok, Some(false));
        assert!(
            result.warnings.iter().any(|w| w.contains("Manifest")),
            "should have manifest warning"
        );
    }
}
