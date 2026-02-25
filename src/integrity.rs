//! Content hashing and integrity verification for skill packages.
//!
//! Each skill directory can contain a `MANIFEST.sha256` file with per-file
//! SHA256 hashes and a composite hash. On index load, hashes are computed
//! over on-disk content and compared against the manifest. Mismatches log
//! warnings but do not fail (graceful degradation).

use std::collections::{BTreeMap, HashMap};

use sha2::{Digest, Sha256};

use crate::state::SkillFile;

/// Per-file hashes for a skill version, plus a composite hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHashes {
    /// path -> "sha256:<hex>" for each file
    pub files: BTreeMap<String, String>,
    /// Hash of the sorted (path + hash) pairs
    pub composite: String,
}

/// Compute SHA256 of a string, returned as `"sha256:<hex>"`.
pub fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

/// Compute hashes for a skill's on-disk content.
///
/// Always includes `SKILL.md` and `skill.toml`. Extra files from
/// `scripts/`, `references/`, and `assets/` are included when present.
pub fn compute_hashes(
    skill_toml: &str,
    skill_md: &str,
    extra_files: &HashMap<String, SkillFile>,
) -> ContentHashes {
    let mut files = BTreeMap::new();

    files.insert("SKILL.md".to_string(), sha256_hex(skill_md));
    files.insert("skill.toml".to_string(), sha256_hex(skill_toml));

    for (path, file) in extra_files {
        files.insert(path.clone(), sha256_hex(&file.content));
    }

    let composite = compute_composite(&files);

    ContentHashes { files, composite }
}

/// Compute a composite hash from sorted (path, hash) pairs.
fn compute_composite(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (path, hash) in files {
        hasher.update(path.as_bytes());
        hasher.update(hash.as_bytes());
    }
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

/// Parse a `MANIFEST.sha256` file into a `ContentHashes`.
///
/// Format: one line per entry, `<hash>  <path>`. The composite hash uses
/// `*` as its path. Files are expected to be sorted alphabetically.
/// Blank lines and lines starting with `#` are ignored.
pub fn parse_manifest(content: &str) -> anyhow::Result<ContentHashes> {
    let mut files = BTreeMap::new();
    let mut composite = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Format: "sha256:<hex>  <path>"
        let Some((hash, path)) = line.split_once("  ") else {
            anyhow::bail!("Invalid manifest line (expected two-space separator): {line}");
        };

        let hash = hash.trim().to_string();
        let path = path.trim().to_string();

        if path == "*" {
            composite = Some(hash);
        } else {
            files.insert(path, hash);
        }
    }

    let composite = composite.ok_or_else(|| {
        anyhow::anyhow!("MANIFEST.sha256 missing composite hash (line with '*' path)")
    })?;

    Ok(ContentHashes { files, composite })
}

/// Write a `ContentHashes` to `MANIFEST.sha256` format.
///
/// Composite hash first (with `*` path), then files sorted alphabetically.
/// Used by `publish_skill` and `validate --generate-manifest` (future).
pub fn format_manifest(hashes: &ContentHashes) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}  *\n", hashes.composite));
    for (path, hash) in &hashes.files {
        out.push_str(&format!("{hash}  {path}\n"));
    }
    out
}

/// Compare computed hashes against expected hashes.
///
/// Returns a list of human-readable mismatch descriptions. Empty means
/// everything matches.
pub fn verify(computed: &ContentHashes, expected: &ContentHashes) -> Vec<String> {
    let mut mismatches = Vec::new();

    if computed.composite != expected.composite {
        mismatches.push(format!(
            "composite hash mismatch: expected {}, computed {}",
            expected.composite, computed.composite
        ));
    }

    // Check each expected file
    for (path, expected_hash) in &expected.files {
        match computed.files.get(path) {
            Some(computed_hash) if computed_hash != expected_hash => {
                mismatches.push(format!(
                    "{path}: expected {expected_hash}, computed {computed_hash}"
                ));
            }
            None => {
                mismatches.push(format!("{path}: listed in manifest but not found on disk"));
            }
            _ => {}
        }
    }

    // Check for files on disk not in the manifest
    for path in computed.files.keys() {
        if !expected.files.contains_key(path) {
            mismatches.push(format!("{path}: found on disk but not in manifest"));
        }
    }

    mismatches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hex() {
        // SHA256 of "hello" is well-known
        let hash = sha256_hex("hello");
        assert_eq!(
            hash,
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_compute_hashes_deterministic() {
        let files = HashMap::new();
        let h1 = compute_hashes("toml content", "md content", &files);
        let h2 = compute_hashes("toml content", "md content", &files);
        assert_eq!(h1.composite, h2.composite);
        assert_eq!(h1.files, h2.files);
    }

    #[test]
    fn test_compute_hashes_includes_extra_files() {
        let mut extra = HashMap::new();
        extra.insert(
            "scripts/lint.sh".to_string(),
            SkillFile {
                content: "#!/bin/bash\necho lint".to_string(),
                mime_type: "text/x-shellscript".to_string(),
            },
        );

        let hashes = compute_hashes("toml", "md", &extra);
        assert_eq!(hashes.files.len(), 3);
        assert!(hashes.files.contains_key("scripts/lint.sh"));
        assert!(hashes.files.contains_key("SKILL.md"));
        assert!(hashes.files.contains_key("skill.toml"));
    }

    #[test]
    fn test_parse_and_format_roundtrip() {
        let mut files = BTreeMap::new();
        files.insert("SKILL.md".to_string(), sha256_hex("md content"));
        files.insert("skill.toml".to_string(), sha256_hex("toml content"));
        files.insert(
            "scripts/lint.sh".to_string(),
            sha256_hex("#!/bin/bash\necho lint"),
        );

        let composite = compute_composite(&files);
        let original = ContentHashes { files, composite };

        let formatted = format_manifest(&original);
        let parsed = parse_manifest(&formatted).expect("should parse");

        assert_eq!(original, parsed);

        // Roundtrip again
        let reformatted = format_manifest(&parsed);
        assert_eq!(formatted, reformatted);
    }

    #[test]
    fn test_parse_manifest_ignores_comments_and_blanks() {
        let content = "# This is a comment\n\
                       sha256:abc123  *\n\
                       \n\
                       sha256:def456  SKILL.md\n\
                       # another comment\n\
                       sha256:789abc  skill.toml\n";

        let hashes = parse_manifest(content).expect("should parse");
        assert_eq!(hashes.composite, "sha256:abc123");
        assert_eq!(hashes.files.len(), 2);
        assert_eq!(hashes.files["SKILL.md"], "sha256:def456");
        assert_eq!(hashes.files["skill.toml"], "sha256:789abc");
    }

    #[test]
    fn test_parse_manifest_missing_composite() {
        let content = "sha256:def456  SKILL.md\n";
        let result = parse_manifest(content);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("composite"),
            "Error should mention composite hash"
        );
    }

    #[test]
    fn test_verify_match() {
        let extra = HashMap::new();
        let hashes = compute_hashes("toml", "md", &extra);
        let mismatches = verify(&hashes, &hashes);
        assert!(mismatches.is_empty());
    }

    #[test]
    fn test_verify_mismatch_file_content() {
        let extra = HashMap::new();
        let computed = compute_hashes("toml", "md", &extra);
        let mut expected = computed.clone();
        expected.files.insert(
            "SKILL.md".to_string(),
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        );

        let mismatches = verify(&computed, &expected);
        assert!(!mismatches.is_empty());
        assert!(mismatches.iter().any(|m| m.contains("SKILL.md")));
    }

    #[test]
    fn test_verify_mismatch_extra_file_on_disk() {
        let extra = HashMap::new();
        let computed = compute_hashes("toml", "md", &extra);

        // Expected manifest doesn't include skill.toml
        let mut expected_files = computed.files.clone();
        expected_files.remove("skill.toml");
        let expected = ContentHashes {
            files: expected_files,
            composite: computed.composite.clone(),
        };

        let mismatches = verify(&computed, &expected);
        assert!(
            mismatches
                .iter()
                .any(|m| m.contains("skill.toml") && m.contains("not in manifest"))
        );
    }

    #[test]
    fn test_verify_mismatch_missing_file_on_disk() {
        let extra = HashMap::new();
        let computed = compute_hashes("toml", "md", &extra);

        let mut expected = computed.clone();
        expected
            .files
            .insert("scripts/gone.sh".to_string(), sha256_hex("disappeared"));

        let mismatches = verify(&computed, &expected);
        assert!(
            mismatches
                .iter()
                .any(|m| m.contains("scripts/gone.sh") && m.contains("not found on disk"))
        );
    }
}
