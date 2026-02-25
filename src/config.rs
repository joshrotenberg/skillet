//! CLI configuration: config file loading, install targets, and shared utilities.
//!
//! The skillet config file lives at `~/.config/skillet/config.toml` and controls
//! default install targets, registries, and other CLI behavior.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level skillet CLI configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkilletConfig {
    pub install: InstallConfig,
    pub registries: RegistriesConfig,
}

/// `[install]` section: default targets and global flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallConfig {
    pub targets: Vec<String>,
    pub global: bool,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            targets: vec!["agents".to_string()],
            global: false,
        }
    }
}

/// `[registries]` section: default local and remote registries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RegistriesConfig {
    pub local: Vec<PathBuf>,
    pub remote: Vec<String>,
}

/// An agent platform to install skills into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallTarget {
    Agents,
    Claude,
    Cursor,
    Copilot,
    Windsurf,
    Gemini,
}

/// All known install targets.
pub const ALL_TARGETS: &[InstallTarget] = &[
    InstallTarget::Agents,
    InstallTarget::Claude,
    InstallTarget::Cursor,
    InstallTarget::Copilot,
    InstallTarget::Windsurf,
    InstallTarget::Gemini,
];

impl InstallTarget {
    /// Parse a target string. Returns `Ok(None)` for "all" (caller expands).
    pub fn parse(s: &str) -> anyhow::Result<Option<Self>> {
        match s.to_lowercase().as_str() {
            "all" => Ok(None),
            "agents" => Ok(Some(Self::Agents)),
            "claude" => Ok(Some(Self::Claude)),
            "cursor" => Ok(Some(Self::Cursor)),
            "copilot" => Ok(Some(Self::Copilot)),
            "windsurf" => Ok(Some(Self::Windsurf)),
            "gemini" => Ok(Some(Self::Gemini)),
            other => anyhow::bail!(
                "Unknown install target: {other}. \
                 Valid targets: agents, claude, cursor, copilot, windsurf, gemini, all"
            ),
        }
    }

    /// Project-local install directory for a skill.
    pub fn project_dir(&self, name: &str) -> PathBuf {
        match self {
            Self::Agents => PathBuf::from(format!(".agents/skills/{name}/")),
            Self::Claude => PathBuf::from(format!(".claude/skills/{name}/")),
            Self::Cursor => PathBuf::from(format!(".cursor/skills/{name}/")),
            Self::Copilot => PathBuf::from(format!(".github/skills/{name}/")),
            Self::Windsurf => PathBuf::from(format!(".windsurf/skills/{name}/")),
            Self::Gemini => PathBuf::from(format!(".gemini/skills/{name}/")),
        }
    }

    /// Global install directory for a skill.
    pub fn global_dir(&self, name: &str) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        match self {
            Self::Agents => PathBuf::from(format!("{home}/.agents/skills/{name}/")),
            Self::Claude => PathBuf::from(format!("{home}/.claude/skills/{name}/")),
            Self::Cursor => PathBuf::from(format!("{home}/.cursor/skills/{name}/")),
            Self::Copilot => PathBuf::from(format!("{home}/.copilot/skills/{name}/")),
            Self::Windsurf => PathBuf::from(format!("{home}/.codeium/windsurf/skills/{name}/")),
            Self::Gemini => PathBuf::from(format!("{home}/.gemini/skills/{name}/")),
        }
    }

    /// Human-readable name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Copilot => "copilot",
            Self::Windsurf => "windsurf",
            Self::Gemini => "gemini",
        }
    }
}

impl std::fmt::Display for InstallTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Load CLI configuration from `~/.config/skillet/config.toml`.
///
/// Returns defaults if the file is absent. Errors if present but malformed.
pub fn load_config() -> anyhow::Result<SkilletConfig> {
    let path = config_dir().join("config.toml");
    if !path.is_file() {
        return Ok(SkilletConfig::default());
    }
    load_config_from(&path)
}

/// Load CLI configuration from a specific path (for testing).
pub fn load_config_from(path: &Path) -> anyhow::Result<SkilletConfig> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
    let config: SkilletConfig = toml::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", path.display()))?;
    Ok(config)
}

/// Resolve install targets from CLI flags and config.
///
/// Priority: flag targets > config targets > default ("agents").
pub fn resolve_targets(
    flag_targets: &[String],
    config: &SkilletConfig,
) -> anyhow::Result<Vec<InstallTarget>> {
    let raw = if !flag_targets.is_empty() {
        flag_targets
    } else if !config.install.targets.is_empty() {
        &config.install.targets
    } else {
        return Ok(vec![InstallTarget::Agents]);
    };

    let mut targets = Vec::new();
    for s in raw {
        match InstallTarget::parse(s)? {
            Some(t) => {
                if !targets.contains(&t) {
                    targets.push(t);
                }
            }
            None => {
                // "all" -- expand to all targets
                for &t in ALL_TARGETS {
                    if !targets.contains(&t) {
                        targets.push(t);
                    }
                }
            }
        }
    }
    Ok(targets)
}

/// The skillet config directory: `~/.config/skillet/`.
pub fn config_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("skillet")
    } else {
        PathBuf::from("/tmp").join("skillet").join("config")
    }
}

/// Current time as ISO 8601 string (UTC).
///
/// Uses `std::time` to avoid adding a chrono dependency.
pub fn now_iso8601() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from days since epoch (algorithm from Howard Hinnant)
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_defaults_when_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        // load_config_from should fail on missing file, but load_config()
        // returns defaults. Test the default path:
        let config = SkilletConfig::default();
        assert_eq!(config.install.targets, vec!["agents"]);
        assert!(!config.install.global);
        assert!(config.registries.local.is_empty());
        assert!(config.registries.remote.is_empty());

        // load_config_from on missing file should error
        assert!(load_config_from(&path).is_err());
    }

    #[test]
    fn test_parse_full_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[install]
targets = ["claude", "cursor"]
global = true

[registries]
local = ["/path/to/local"]
remote = ["https://github.com/owner/repo.git"]
"#,
        )
        .unwrap();

        let config = load_config_from(&path).unwrap();
        assert_eq!(config.install.targets, vec!["claude", "cursor"]);
        assert!(config.install.global);
        assert_eq!(
            config.registries.local,
            vec![PathBuf::from("/path/to/local")]
        );
        assert_eq!(
            config.registries.remote,
            vec!["https://github.com/owner/repo.git"]
        );
    }

    #[test]
    fn test_parse_partial_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[install]
targets = ["gemini"]
"#,
        )
        .unwrap();

        let config = load_config_from(&path).unwrap();
        assert_eq!(config.install.targets, vec!["gemini"]);
        assert!(!config.install.global);
        assert!(config.registries.local.is_empty());
    }

    #[test]
    fn test_malformed_toml_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml {{{").unwrap();
        assert!(load_config_from(&path).is_err());
    }

    #[test]
    fn test_resolve_targets_flag_overrides_config() {
        let config = SkilletConfig {
            install: InstallConfig {
                targets: vec!["claude".to_string()],
                global: false,
            },
            registries: RegistriesConfig::default(),
        };
        let flags = vec!["cursor".to_string()];
        let targets = resolve_targets(&flags, &config).unwrap();
        assert_eq!(targets, vec![InstallTarget::Cursor]);
    }

    #[test]
    fn test_resolve_targets_falls_back_to_config() {
        let config = SkilletConfig {
            install: InstallConfig {
                targets: vec!["claude".to_string(), "cursor".to_string()],
                global: false,
            },
            registries: RegistriesConfig::default(),
        };
        let targets = resolve_targets(&[], &config).unwrap();
        assert_eq!(targets, vec![InstallTarget::Claude, InstallTarget::Cursor]);
    }

    #[test]
    fn test_resolve_targets_default_agents() {
        let config = SkilletConfig {
            install: InstallConfig {
                targets: Vec::new(),
                global: false,
            },
            registries: RegistriesConfig::default(),
        };
        let targets = resolve_targets(&[], &config).unwrap();
        assert_eq!(targets, vec![InstallTarget::Agents]);
    }

    #[test]
    fn test_all_expands_to_all_targets() {
        let config = SkilletConfig::default();
        let flags = vec!["all".to_string()];
        let targets = resolve_targets(&flags, &config).unwrap();
        assert_eq!(targets.len(), ALL_TARGETS.len());
        for &t in ALL_TARGETS {
            assert!(targets.contains(&t));
        }
    }

    #[test]
    fn test_invalid_target_errors() {
        let result = InstallTarget::parse("vscode");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_dir_paths() {
        assert_eq!(
            InstallTarget::Agents.project_dir("my-skill"),
            PathBuf::from(".agents/skills/my-skill/")
        );
        assert_eq!(
            InstallTarget::Claude.project_dir("my-skill"),
            PathBuf::from(".claude/skills/my-skill/")
        );
        assert_eq!(
            InstallTarget::Cursor.project_dir("my-skill"),
            PathBuf::from(".cursor/skills/my-skill/")
        );
        assert_eq!(
            InstallTarget::Copilot.project_dir("my-skill"),
            PathBuf::from(".github/skills/my-skill/")
        );
        assert_eq!(
            InstallTarget::Windsurf.project_dir("my-skill"),
            PathBuf::from(".windsurf/skills/my-skill/")
        );
        assert_eq!(
            InstallTarget::Gemini.project_dir("my-skill"),
            PathBuf::from(".gemini/skills/my-skill/")
        );
    }

    #[test]
    fn test_global_dir_paths() {
        // Set HOME for deterministic test
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        assert_eq!(
            InstallTarget::Agents.global_dir("my-skill"),
            PathBuf::from(format!("{home}/.agents/skills/my-skill/"))
        );
        assert_eq!(
            InstallTarget::Copilot.global_dir("my-skill"),
            PathBuf::from(format!("{home}/.copilot/skills/my-skill/"))
        );
        assert_eq!(
            InstallTarget::Windsurf.global_dir("my-skill"),
            PathBuf::from(format!("{home}/.codeium/windsurf/skills/my-skill/"))
        );
    }

    #[test]
    fn test_now_iso8601_format() {
        let ts = now_iso8601();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn test_dedup_targets() {
        let config = SkilletConfig::default();
        let flags = vec!["claude".to_string(), "claude".to_string()];
        let targets = resolve_targets(&flags, &config).unwrap();
        assert_eq!(targets, vec![InstallTarget::Claude]);
    }
}
