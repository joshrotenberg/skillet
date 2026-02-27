//! Pack a skillpack: validate, generate MANIFEST.sha256, update versions.toml.

use std::path::Path;

use crate::config;
use crate::error::Error;
use crate::integrity;
use crate::state::{VersionRecord, VersionsManifest};
use crate::validate::{self, ValidationResult};

/// Result of packing a skillpack directory.
#[derive(Debug)]
pub struct PackResult {
    pub validation: ValidationResult,
    pub manifest_written: bool,
    pub versions_updated: bool,
}

/// Validate a skillpack, generate MANIFEST.sha256, and update versions.toml.
///
/// Fails if validation fails. Idempotent: re-running on an already-packed
/// directory with the same version is a no-op for versions.toml and
/// overwrites MANIFEST.sha256 with identical content.
pub fn pack(dir: &Path) -> crate::error::Result<PackResult> {
    let validation = validate::validate_skillpack(dir)?;

    // Generate and write MANIFEST.sha256
    let manifest_content = integrity::format_manifest(&validation.hashes);
    let manifest_path = dir.join("MANIFEST.sha256");
    std::fs::write(&manifest_path, &manifest_content).map_err(|e| Error::WriteFile {
        path: manifest_path.clone(),
        source: e,
    })?;

    // Update versions.toml
    let versions_updated = update_versions_toml(dir, &validation)?;

    Ok(PackResult {
        validation,
        manifest_written: true,
        versions_updated,
    })
}

/// Update versions.toml with the current version.
///
/// If versions.toml exists, appends the current version if not already listed.
/// If it doesn't exist, creates it with a single entry. Returns true if the
/// file was modified.
fn update_versions_toml(dir: &Path, validation: &ValidationResult) -> crate::error::Result<bool> {
    let versions_path = dir.join("versions.toml");
    let version = &validation.version;
    let now = now_iso8601();

    if versions_path.is_file() {
        let raw = std::fs::read_to_string(&versions_path).map_err(|e| Error::FileRead {
            path: versions_path.clone(),
            source: e,
        })?;
        let mut manifest: VersionsManifest =
            toml::from_str(&raw).map_err(|e| Error::TomlParse {
                path: versions_path.clone(),
                source: e,
            })?;

        // Check if this version is already listed
        if manifest.versions.iter().any(|v| v.version == *version) {
            return Ok(false);
        }

        manifest.versions.push(VersionRecord {
            version: version.clone(),
            published: now,
            yanked: false,
        });

        let content = toml::to_string_pretty(&manifest).map_err(Error::ManifestSerialize)?;
        std::fs::write(&versions_path, content).map_err(|e| Error::WriteFile {
            path: versions_path.clone(),
            source: e,
        })?;
    } else {
        let manifest = VersionsManifest {
            versions: vec![VersionRecord {
                version: version.clone(),
                published: now,
                yanked: false,
            }],
        };

        let content = toml::to_string_pretty(&manifest).map_err(Error::ManifestSerialize)?;
        std::fs::write(&versions_path, content).map_err(|e| Error::WriteFile {
            path: versions_path.clone(),
            source: e,
        })?;
    }

    Ok(true)
}

/// Current time as ISO 8601 string (UTC).
fn now_iso8601() -> String {
    config::now_iso8601()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-registry")
    }

    #[test]
    fn test_now_iso8601_format() {
        let ts = now_iso8601();
        // Should be YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn test_pack_creates_manifest() {
        let src = test_registry().join("joshrotenberg/code-review");
        if !src.exists() {
            return;
        }

        let tmp = tempfile::tempdir().unwrap();
        copy_dir(&src, tmp.path());

        // Remove existing manifest to test generation
        let manifest_path = tmp.path().join("MANIFEST.sha256");
        let _ = std::fs::remove_file(&manifest_path);

        let result = pack(tmp.path()).expect("pack should succeed");
        assert!(result.manifest_written);
        assert!(manifest_path.is_file());

        // Re-validate: manifest should now verify
        let revalidated =
            validate::validate_skillpack(tmp.path()).expect("re-validation should succeed");
        assert_eq!(revalidated.manifest_ok, Some(true));
    }

    #[test]
    fn test_pack_creates_versions_toml() {
        let src = test_registry().join("skillet/setup");
        if !src.exists() {
            return;
        }

        let tmp = tempfile::tempdir().unwrap();
        copy_dir(&src, tmp.path());

        // This skill has no versions.toml
        let versions_path = tmp.path().join("versions.toml");
        assert!(!versions_path.is_file());

        let result = pack(tmp.path()).expect("pack should succeed");
        assert!(result.versions_updated);
        assert!(versions_path.is_file());

        let raw = std::fs::read_to_string(&versions_path).unwrap();
        let manifest: VersionsManifest = toml::from_str(&raw).unwrap();
        assert_eq!(manifest.versions.len(), 1);
        assert_eq!(manifest.versions[0].version, result.validation.version);
    }

    #[test]
    fn test_pack_idempotent_versions() {
        let src = test_registry().join("joshrotenberg/rust-dev");
        if !src.exists() {
            return;
        }

        let tmp = tempfile::tempdir().unwrap();
        copy_dir(&src, tmp.path());

        // First pack
        let result1 = pack(tmp.path()).expect("first pack should succeed");
        // versions.toml already has this version
        assert!(!result1.versions_updated);

        // Second pack should also be idempotent
        let result2 = pack(tmp.path()).expect("second pack should succeed");
        assert!(!result2.versions_updated);
    }

    /// Copy a directory recursively (test helper).
    fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                std::fs::create_dir_all(&dst_path).unwrap();
                copy_dir(&src_path, &dst_path);
            } else {
                std::fs::copy(&src_path, &dst_path).unwrap();
            }
        }
    }
}
