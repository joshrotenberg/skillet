//! Trust tiers and content hash pinning for skill registries.
//!
//! The trust state lives at `~/.config/skillet/trust.toml` and tracks which
//! registries the user trusts and which installed skills have pinned content
//! hashes. This lets users distinguish trusted registries (their own, their
//! team's) from unknown ones, and detect when installed skill content changes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::Error;
use crate::integrity;
use crate::manifest::InstalledManifest;

/// Trust tier assigned to a registry during install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustTier {
    /// Registry is explicitly trusted by the user.
    Trusted,
    /// Skill has a pinned content hash (previously installed).
    Reviewed,
    /// Registry is not trusted and skill is not pinned.
    Unknown,
}

impl std::fmt::Display for TrustTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trusted => write!(f, "trusted"),
            Self::Reviewed => write!(f, "reviewed"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// A registry the user has explicitly marked as trusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedRegistry {
    pub registry: String,
    pub trusted_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A skill with a pinned content hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedSkill {
    pub owner: String,
    pub name: String,
    pub version: String,
    pub registry: String,
    pub content_hash: String,
    pub pinned_at: String,
}

/// Persistent trust state: trusted registries and pinned skills.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrustState {
    #[serde(default)]
    pub trusted_registries: Vec<TrustedRegistry>,
    #[serde(default)]
    pub pinned_skills: Vec<PinnedSkill>,
}

/// Result of evaluating trust for a skill install.
#[derive(Debug, Clone)]
pub struct TrustCheck {
    pub tier: TrustTier,
    /// The pinned hash, if the skill was previously pinned.
    pub pinned_hash: Option<String>,
    /// Human-readable explanation.
    pub reason: String,
}

/// Status of a single skill in an audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditStatus {
    /// Pinned hash matches installed content.
    Ok,
    /// Pinned hash does not match installed content.
    Modified,
    /// Skill is installed but not pinned.
    Unpinned,
    /// SKILL.md is missing from the installed path.
    Missing,
}

impl std::fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok => write!(f, "ok"),
            Self::Modified => write!(f, "MODIFIED"),
            Self::Unpinned => write!(f, "unpinned"),
            Self::Missing => write!(f, "MISSING"),
        }
    }
}

/// Result of auditing a single installed skill.
#[derive(Debug, Clone)]
pub struct AuditResult {
    pub owner: String,
    pub name: String,
    pub version: String,
    pub installed_to: PathBuf,
    pub status: AuditStatus,
}

// -- State I/O --

/// Default trust state path: `~/.config/skillet/trust.toml`.
pub fn trust_path() -> PathBuf {
    config::config_dir().join("trust.toml")
}

/// Load the trust state, returning empty if the file is absent.
pub fn load() -> crate::error::Result<TrustState> {
    load_from(&trust_path())
}

/// Load the trust state from a specific path.
pub fn load_from(path: &Path) -> crate::error::Result<TrustState> {
    if !path.is_file() {
        return Ok(TrustState::default());
    }
    let raw = std::fs::read_to_string(path).map_err(|e| Error::TrustRead {
        path: path.to_path_buf(),
        source: e,
    })?;
    let state: TrustState = toml::from_str(&raw).map_err(|e| Error::TrustParse {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(state)
}

/// Save the trust state to the default path.
pub fn save(state: &TrustState) -> crate::error::Result<()> {
    save_to(state, &trust_path())
}

/// Save the trust state to a specific path.
pub fn save_to(state: &TrustState, path: &Path) -> crate::error::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::CreateDir {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let content = toml::to_string_pretty(state).map_err(Error::TrustSerialize)?;
    std::fs::write(path, content).map_err(|e| Error::TrustWrite {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

// -- TrustState methods --

impl TrustState {
    /// Add a trusted registry. No-op if already present.
    pub fn add_registry(&mut self, registry: &str, note: Option<&str>) {
        if self.is_trusted(registry) {
            return;
        }
        self.trusted_registries.push(TrustedRegistry {
            registry: registry.to_string(),
            trusted_at: config::now_iso8601(),
            note: note.map(|s| s.to_string()),
        });
    }

    /// Remove a trusted registry. Returns true if it was present.
    pub fn remove_registry(&mut self, registry: &str) -> bool {
        let before = self.trusted_registries.len();
        self.trusted_registries.retain(|r| r.registry != registry);
        self.trusted_registries.len() < before
    }

    /// Check whether a registry is trusted.
    pub fn is_trusted(&self, registry: &str) -> bool {
        self.trusted_registries
            .iter()
            .any(|r| r.registry == registry)
    }

    /// Pin a skill's content hash. Replaces an existing pin for the same owner/name.
    pub fn pin_skill(
        &mut self,
        owner: &str,
        name: &str,
        version: &str,
        registry: &str,
        content_hash: &str,
    ) {
        // Upsert: remove existing pin for this owner/name, then add new
        self.pinned_skills
            .retain(|p| !(p.owner == owner && p.name == name));
        self.pinned_skills.push(PinnedSkill {
            owner: owner.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            registry: registry.to_string(),
            content_hash: content_hash.to_string(),
            pinned_at: config::now_iso8601(),
        });
    }

    /// Remove a pin. Returns true if it was present.
    pub fn unpin_skill(&mut self, owner: &str, name: &str) -> bool {
        let before = self.pinned_skills.len();
        self.pinned_skills
            .retain(|p| !(p.owner == owner && p.name == name));
        self.pinned_skills.len() < before
    }

    /// Find a pin by owner and name.
    pub fn find_pin(&self, owner: &str, name: &str) -> Option<&PinnedSkill> {
        self.pinned_skills
            .iter()
            .find(|p| p.owner == owner && p.name == name)
    }
}

// -- Trust checking --

/// Evaluate trust for a skill install.
///
/// Returns a `TrustCheck` describing the tier, any pinned hash, and a
/// human-readable reason.
pub fn check_trust(
    state: &TrustState,
    registry_id: &str,
    owner: &str,
    name: &str,
    content_hash: &str,
) -> TrustCheck {
    // Check if the registry is trusted
    if state.is_trusted(registry_id) {
        return TrustCheck {
            tier: TrustTier::Trusted,
            pinned_hash: state.find_pin(owner, name).map(|p| p.content_hash.clone()),
            reason: format!("registry '{registry_id}' is trusted"),
        };
    }

    // Check if there's a pinned hash for this skill
    if let Some(pin) = state.find_pin(owner, name) {
        if pin.content_hash == content_hash {
            return TrustCheck {
                tier: TrustTier::Reviewed,
                pinned_hash: Some(pin.content_hash.clone()),
                reason: format!("{owner}/{name} pinned hash matches (v{})", pin.version),
            };
        } else {
            return TrustCheck {
                tier: TrustTier::Reviewed,
                pinned_hash: Some(pin.content_hash.clone()),
                reason: format!(
                    "{owner}/{name} content changed since pinned (was v{})",
                    pin.version
                ),
            };
        }
    }

    // Unknown
    TrustCheck {
        tier: TrustTier::Unknown,
        pinned_hash: None,
        reason: format!("registry '{registry_id}' is not trusted and {owner}/{name} is not pinned"),
    }
}

// -- Audit --

/// Audit installed skills against the trust state.
///
/// For each installed skill:
/// - If pinned and hash matches installed SKILL.md -> Ok
/// - If pinned and hash differs -> Modified
/// - If not pinned -> Unpinned
/// - If SKILL.md missing -> Missing
///
/// Optionally filter to a single skill by owner/name.
pub fn audit(
    installed: &InstalledManifest,
    trust_state: &TrustState,
    filter_owner: Option<&str>,
    filter_name: Option<&str>,
) -> Vec<AuditResult> {
    let mut results = Vec::new();

    for skill in &installed.skills {
        // Apply filter if specified
        if let Some(fo) = filter_owner
            && skill.owner != fo
        {
            continue;
        }
        if let Some(fn_) = filter_name
            && skill.name != fn_
        {
            continue;
        }

        let status = match trust_state.find_pin(&skill.owner, &skill.name) {
            Some(pin) => {
                // Read installed SKILL.md and compare hash
                let skill_md_path = skill.installed_to.join("SKILL.md");
                match std::fs::read_to_string(&skill_md_path) {
                    Ok(content) => {
                        let computed = integrity::sha256_hex(&content);
                        if computed == pin.content_hash {
                            AuditStatus::Ok
                        } else {
                            AuditStatus::Modified
                        }
                    }
                    Err(_) => AuditStatus::Missing,
                }
            }
            None => AuditStatus::Unpinned,
        };

        results.push(AuditResult {
            owner: skill.owner.clone(),
            name: skill.name.clone(),
            version: skill.version.clone(),
            installed_to: skill.installed_to.clone(),
            status,
        });
    }

    results
}

/// Check if any audit results indicate a problem (Modified or Missing).
pub fn audit_has_problems(results: &[AuditResult]) -> bool {
    results
        .iter()
        .any(|r| matches!(r.status, AuditStatus::Modified | AuditStatus::Missing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest;

    // -- State I/O tests --

    #[test]
    fn test_load_empty_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let state = load_from(&path).unwrap();
        assert!(state.trusted_registries.is_empty());
        assert!(state.pinned_skills.is_empty());
    }

    #[test]
    fn test_save_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("trust.toml");

        let mut state = TrustState::default();
        state.add_registry("https://github.com/owner/repo.git", Some("my registry"));
        state.pin_skill(
            "owner",
            "skill",
            "1.0.0",
            "https://github.com/owner/repo.git",
            "sha256:abc123",
        );

        save_to(&state, &path).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.trusted_registries.len(), 1);
        assert_eq!(
            loaded.trusted_registries[0].registry,
            "https://github.com/owner/repo.git"
        );
        assert_eq!(
            loaded.trusted_registries[0].note.as_deref(),
            Some("my registry")
        );
        assert_eq!(loaded.pinned_skills.len(), 1);
        assert_eq!(loaded.pinned_skills[0].owner, "owner");
        assert_eq!(loaded.pinned_skills[0].content_hash, "sha256:abc123");
    }

    #[test]
    fn test_malformed_toml_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("trust.toml");
        std::fs::write(&path, "this is not valid toml {{{").unwrap();
        assert!(load_from(&path).is_err());
    }

    // -- Trust checking tests --

    #[test]
    fn test_check_trust_trusted_registry() {
        let mut state = TrustState::default();
        state.add_registry("https://github.com/owner/repo.git", None);

        let check = check_trust(
            &state,
            "https://github.com/owner/repo.git",
            "owner",
            "skill",
            "sha256:abc",
        );
        assert_eq!(check.tier, TrustTier::Trusted);
        assert!(check.reason.contains("trusted"));
    }

    #[test]
    fn test_check_trust_pinned_match() {
        let mut state = TrustState::default();
        state.pin_skill(
            "owner",
            "skill",
            "1.0.0",
            "https://github.com/other/repo.git",
            "sha256:abc",
        );

        let check = check_trust(
            &state,
            "https://github.com/other/repo.git",
            "owner",
            "skill",
            "sha256:abc",
        );
        assert_eq!(check.tier, TrustTier::Reviewed);
        assert_eq!(check.pinned_hash.as_deref(), Some("sha256:abc"));
        assert!(check.reason.contains("matches"));
    }

    #[test]
    fn test_check_trust_pinned_mismatch() {
        let mut state = TrustState::default();
        state.pin_skill(
            "owner",
            "skill",
            "1.0.0",
            "https://github.com/other/repo.git",
            "sha256:old",
        );

        let check = check_trust(
            &state,
            "https://github.com/other/repo.git",
            "owner",
            "skill",
            "sha256:new",
        );
        assert_eq!(check.tier, TrustTier::Reviewed);
        assert_eq!(check.pinned_hash.as_deref(), Some("sha256:old"));
        assert!(check.reason.contains("changed"));
    }

    #[test]
    fn test_check_trust_unknown() {
        let state = TrustState::default();
        let check = check_trust(
            &state,
            "https://github.com/unknown/repo.git",
            "owner",
            "skill",
            "sha256:abc",
        );
        assert_eq!(check.tier, TrustTier::Unknown);
        assert!(check.pinned_hash.is_none());
        assert!(check.reason.contains("not trusted"));
    }

    // -- Registry management tests --

    #[test]
    fn test_add_registry() {
        let mut state = TrustState::default();
        state.add_registry("https://github.com/owner/repo.git", None);
        assert!(state.is_trusted("https://github.com/owner/repo.git"));
        assert!(!state.is_trusted("https://github.com/other/repo.git"));
    }

    #[test]
    fn test_add_registry_idempotent() {
        let mut state = TrustState::default();
        state.add_registry("https://github.com/owner/repo.git", None);
        state.add_registry("https://github.com/owner/repo.git", Some("dup"));
        assert_eq!(state.trusted_registries.len(), 1);
    }

    #[test]
    fn test_remove_registry() {
        let mut state = TrustState::default();
        state.add_registry("https://github.com/owner/repo.git", None);

        let removed = state.remove_registry("https://github.com/owner/repo.git");
        assert!(removed);
        assert!(!state.is_trusted("https://github.com/owner/repo.git"));

        let removed = state.remove_registry("https://github.com/owner/repo.git");
        assert!(!removed);
    }

    // -- Pin management tests --

    #[test]
    fn test_pin_skill() {
        let mut state = TrustState::default();
        state.pin_skill("owner", "skill", "1.0.0", "reg", "sha256:abc");
        assert!(state.find_pin("owner", "skill").is_some());
        assert!(state.find_pin("owner", "other").is_none());
    }

    #[test]
    fn test_unpin_skill() {
        let mut state = TrustState::default();
        state.pin_skill("owner", "skill", "1.0.0", "reg", "sha256:abc");

        let removed = state.unpin_skill("owner", "skill");
        assert!(removed);
        assert!(state.find_pin("owner", "skill").is_none());

        let removed = state.unpin_skill("owner", "skill");
        assert!(!removed);
    }

    #[test]
    fn test_pin_upsert_replaces() {
        let mut state = TrustState::default();
        state.pin_skill("owner", "skill", "1.0.0", "reg", "sha256:old");
        state.pin_skill("owner", "skill", "2.0.0", "reg", "sha256:new");

        assert_eq!(state.pinned_skills.len(), 1);
        let pin = state.find_pin("owner", "skill").unwrap();
        assert_eq!(pin.version, "2.0.0");
        assert_eq!(pin.content_hash, "sha256:new");
    }

    // -- Audit tests --

    #[test]
    fn test_audit_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let content = "# My Skill\nDo the thing.";
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();
        let hash = integrity::sha256_hex(content);

        let installed = InstalledManifest {
            skills: vec![manifest::InstalledSkill {
                owner: "owner".to_string(),
                name: "skill".to_string(),
                version: "1.0.0".to_string(),
                registry: "reg".to_string(),
                checksum: hash.clone(),
                installed_to: skill_dir,
                installed_at: "2026-01-01T00:00:00Z".to_string(),
            }],
        };

        let mut trust_state = TrustState::default();
        trust_state.pin_skill("owner", "skill", "1.0.0", "reg", &hash);

        let results = audit(&installed, &trust_state, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AuditStatus::Ok);
    }

    #[test]
    fn test_audit_modified() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        std::fs::write(skill_dir.join("SKILL.md"), "modified content").unwrap();

        let installed = InstalledManifest {
            skills: vec![manifest::InstalledSkill {
                owner: "owner".to_string(),
                name: "skill".to_string(),
                version: "1.0.0".to_string(),
                registry: "reg".to_string(),
                checksum: "sha256:whatever".to_string(),
                installed_to: skill_dir,
                installed_at: "2026-01-01T00:00:00Z".to_string(),
            }],
        };

        let mut trust_state = TrustState::default();
        trust_state.pin_skill(
            "owner",
            "skill",
            "1.0.0",
            "reg",
            &integrity::sha256_hex("original content"),
        );

        let results = audit(&installed, &trust_state, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AuditStatus::Modified);
        assert!(audit_has_problems(&results));
    }

    #[test]
    fn test_audit_unpinned() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "content").unwrap();

        let installed = InstalledManifest {
            skills: vec![manifest::InstalledSkill {
                owner: "owner".to_string(),
                name: "skill".to_string(),
                version: "1.0.0".to_string(),
                registry: "reg".to_string(),
                checksum: "sha256:abc".to_string(),
                installed_to: skill_dir,
                installed_at: "2026-01-01T00:00:00Z".to_string(),
            }],
        };

        let trust_state = TrustState::default(); // no pins

        let results = audit(&installed, &trust_state, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AuditStatus::Unpinned);
    }

    #[test]
    fn test_audit_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("my-skill");
        // Don't create the directory or SKILL.md

        let installed = InstalledManifest {
            skills: vec![manifest::InstalledSkill {
                owner: "owner".to_string(),
                name: "skill".to_string(),
                version: "1.0.0".to_string(),
                registry: "reg".to_string(),
                checksum: "sha256:abc".to_string(),
                installed_to: skill_dir,
                installed_at: "2026-01-01T00:00:00Z".to_string(),
            }],
        };

        let mut trust_state = TrustState::default();
        trust_state.pin_skill("owner", "skill", "1.0.0", "reg", "sha256:abc");

        let results = audit(&installed, &trust_state, None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, AuditStatus::Missing);
        assert!(audit_has_problems(&results));
    }

    #[test]
    fn test_audit_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let dir1 = tmp.path().join("skill1");
        let dir2 = tmp.path().join("skill2");
        std::fs::create_dir_all(&dir1).unwrap();
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(dir1.join("SKILL.md"), "content1").unwrap();
        std::fs::write(dir2.join("SKILL.md"), "content2").unwrap();

        let installed = InstalledManifest {
            skills: vec![
                manifest::InstalledSkill {
                    owner: "alice".to_string(),
                    name: "skill1".to_string(),
                    version: "1.0.0".to_string(),
                    registry: "reg".to_string(),
                    checksum: "sha256:a".to_string(),
                    installed_to: dir1,
                    installed_at: "2026-01-01T00:00:00Z".to_string(),
                },
                manifest::InstalledSkill {
                    owner: "bob".to_string(),
                    name: "skill2".to_string(),
                    version: "1.0.0".to_string(),
                    registry: "reg".to_string(),
                    checksum: "sha256:b".to_string(),
                    installed_to: dir2,
                    installed_at: "2026-01-01T00:00:00Z".to_string(),
                },
            ],
        };

        let trust_state = TrustState::default();

        // No filter -> both
        let results = audit(&installed, &trust_state, None, None);
        assert_eq!(results.len(), 2);

        // Filter by owner
        let results = audit(&installed, &trust_state, Some("alice"), None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].owner, "alice");

        // Filter by owner+name
        let results = audit(&installed, &trust_state, Some("bob"), Some("skill2"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "skill2");

        // Filter with no match
        let results = audit(&installed, &trust_state, Some("nobody"), None);
        assert!(results.is_empty());
    }

    // -- Config backwards compat --

    #[test]
    fn test_config_without_trust_loads_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[install]
targets = ["agents"]
"#,
        )
        .unwrap();

        let config = config::load_config_from(&path).unwrap();
        assert_eq!(config.trust.unknown_policy, "warn");
        assert!(config.trust.auto_pin);
    }
}
