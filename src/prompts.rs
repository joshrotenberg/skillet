//! Register skills as MCP prompts via tower-mcp's DynamicPromptRegistry.
//!
//! Each skill in the index becomes an MCP prompt, namespaced as `owner_skill-name`.
//! Prompts support an optional `section` argument for filtering by heading.
//! On index refresh, stale prompts are unregistered and new ones registered,
//! with a `prompts/list_changed` notification emitted automatically.

use std::collections::HashMap;

use tower_mcp::PromptBuilder;
use tower_mcp::protocol::{Content, GetPromptResult, PromptMessage, PromptRole};
use tower_mcp::registry::DynamicPromptRegistry;

use crate::state::SkillIndex;

/// Register all skills from the index as MCP prompts.
///
/// Prompt names are namespaced as `owner_skill-name` to avoid collisions
/// across repos. Each prompt accepts an optional `section` argument
/// to return only a specific section (by heading) of the SKILL.md.
pub fn register_all(registry: &DynamicPromptRegistry, index: &SkillIndex) {
    for ((owner, name), entry) in &index.skills {
        let Some(latest) = entry.latest() else {
            continue;
        };

        let prompt_name = format!("{owner}_{name}");
        let description = latest.metadata.skill.description.clone();
        let content = latest.skill_md.clone();

        if content.is_empty() {
            tracing::debug!(
                prompt = %prompt_name,
                "Skipping prompt with empty SKILL.md"
            );
            continue;
        }

        let prompt = PromptBuilder::new(&prompt_name)
            .description(&description)
            .optional_arg("section", "Return only a specific section (by heading)")
            .handler(move |args: HashMap<String, String>| {
                let content = content.clone();
                let description = description.clone();
                async move {
                    let text = if let Some(section) = args.get("section") {
                        extract_section(&content, section).unwrap_or_else(|| {
                            format!(
                                "Section '{section}' not found. Available sections:\n{}",
                                list_sections(&content)
                            )
                        })
                    } else {
                        content
                    };

                    Ok(GetPromptResult {
                        description: Some(description),
                        messages: vec![PromptMessage {
                            role: PromptRole::User,
                            content: Content::text(text),
                            meta: None,
                        }],
                        meta: None,
                    })
                }
            })
            .build();

        registry.register(prompt);

        tracing::debug!(prompt = %prompt_name, "Registered skill as prompt");
    }
}

/// Extract a section from markdown by heading.
///
/// Matches headings case-insensitively. Returns the heading and everything
/// until the next heading at the same or higher level, or end of document.
fn extract_section(content: &str, section_name: &str) -> Option<String> {
    let section_lower = section_name.to_lowercase();
    let lines: Vec<&str> = content.lines().collect();

    let mut start = None;
    let mut start_level = 0;

    for (i, line) in lines.iter().enumerate() {
        if let Some((level, heading)) = parse_heading(line) {
            let heading_lower = heading.to_lowercase();
            if start.is_none()
                && (heading_lower == section_lower || heading_lower.contains(&section_lower))
            {
                start = Some(i);
                start_level = level;
            } else if let Some(s) = start {
                // Found the next heading at same or higher level -- stop
                if level <= start_level && i > s {
                    let section: Vec<&str> = lines[s..i].to_vec();
                    return Some(section.join("\n").trim_end().to_string());
                }
            }
        }
    }

    // Section extends to end of document
    start.map(|s| lines[s..].join("\n").trim_end().to_string())
}

/// List all top-level and second-level headings in markdown.
fn list_sections(content: &str) -> String {
    content
        .lines()
        .filter_map(|line| {
            let (level, heading) = parse_heading(line)?;
            if level <= 3 {
                let indent = "  ".repeat(level.saturating_sub(1));
                Some(format!("{indent}- {heading}"))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse a markdown heading line, returning (level, text).
fn parse_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level > 6 {
        return None;
    }
    let text = trimmed[level..].trim();
    if text.is_empty() {
        return None;
    }
    Some((level, text))
}

/// Sync the prompt registry with a new index.
///
/// Unregisters prompts that are no longer in the index, registers new ones,
/// and updates any that changed. Each mutation emits a `prompts/list_changed`
/// notification to connected clients.
pub fn sync(registry: &DynamicPromptRegistry, old_index: &SkillIndex, new_index: &SkillIndex) {
    // Unregister prompts for skills no longer present
    for (owner, name) in old_index.skills.keys() {
        if !new_index
            .skills
            .contains_key(&(owner.clone(), name.clone()))
        {
            let prompt_name = format!("{owner}_{name}");
            if registry.unregister(&prompt_name) {
                tracing::debug!(prompt = %prompt_name, "Unregistered removed skill prompt");
            }
        }
    }

    // Register or update prompts for current skills
    register_all(registry, new_index);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SkillEntry, SkillMetadata, SkillSource, SkillVersion};

    fn make_entry(owner: &str, name: &str, description: &str, content: &str) -> SkillEntry {
        SkillEntry {
            owner: owner.to_string(),
            name: name.to_string(),
            repo_path: None,
            source: SkillSource::Repo,
            trust_tier: Default::default(),
            discovered_via: Vec::new(),
            versions: vec![SkillVersion {
                version: "1.0.0".to_string(),
                metadata: SkillMetadata {
                    skill: crate::state::SkillInfo {
                        name: name.to_string(),
                        owner: owner.to_string(),
                        version: "1.0.0".to_string(),
                        description: description.to_string(),
                        trigger: None,
                        license: None,
                        author: None,
                        classification: None,
                        compatibility: None,
                    },
                },
                skill_md: content.to_string(),
                skill_toml_raw: String::new(),
                yanked: false,
                files: HashMap::new(),
                published: None,
                has_content: true,
            }],
        }
    }

    fn make_index(entries: Vec<SkillEntry>) -> SkillIndex {
        let mut index = SkillIndex::default();
        for entry in entries {
            let key = (entry.owner.clone(), entry.name.clone());
            index.skills.insert(key, entry);
        }
        index
    }

    #[test]
    fn test_register_all_creates_prompts() {
        let (router, registry) = tower_mcp::McpRouter::new()
            .server_info("test", "0.1.0")
            .with_dynamic_prompts();
        let _ = router;

        let index = make_index(vec![
            make_entry(
                "acme",
                "rust-dev",
                "Rust development",
                "# Rust Dev\n\nUse cargo.",
            ),
            make_entry(
                "acme",
                "python-dev",
                "Python development",
                "# Python Dev\n\nUse pip.",
            ),
        ]);

        register_all(&registry, &index);
    }

    #[test]
    fn test_register_skips_empty_content() {
        let (router, registry) = tower_mcp::McpRouter::new()
            .server_info("test", "0.1.0")
            .with_dynamic_prompts();
        let _ = router;

        let index = make_index(vec![
            make_entry("acme", "empty", "Empty skill", ""),
            make_entry("acme", "real", "Real skill", "# Real content"),
        ]);

        register_all(&registry, &index);
    }

    #[test]
    fn test_sync_removes_old_adds_new() {
        let (router, registry) = tower_mcp::McpRouter::new()
            .server_info("test", "0.1.0")
            .with_dynamic_prompts();
        let _ = router;

        let old_index = make_index(vec![
            make_entry("acme", "old-skill", "Old", "# Old"),
            make_entry("acme", "kept-skill", "Kept", "# Kept"),
        ]);

        let new_index = make_index(vec![
            make_entry("acme", "kept-skill", "Kept", "# Kept"),
            make_entry("acme", "new-skill", "New", "# New"),
        ]);

        register_all(&registry, &old_index);
        sync(&registry, &old_index, &new_index);
    }

    // -- Section extraction --

    const SAMPLE_MD: &str = "\
# Main Title

Introduction text.

## Setup

Setup instructions here.

### Prerequisites

Need these things.

## Usage

How to use it.

## Advanced

Advanced topics.

### Configuration

Config details.

### Troubleshooting

Debug tips.
";

    #[test]
    fn extract_exact_section() {
        let result = extract_section(SAMPLE_MD, "Setup").unwrap();
        assert!(result.starts_with("## Setup"));
        assert!(result.contains("Setup instructions"));
        assert!(result.contains("Prerequisites"));
        // Should not include Usage
        assert!(!result.contains("How to use it"));
    }

    #[test]
    fn extract_section_case_insensitive() {
        let result = extract_section(SAMPLE_MD, "setup").unwrap();
        assert!(result.starts_with("## Setup"));
    }

    #[test]
    fn extract_section_partial_match() {
        let result = extract_section(SAMPLE_MD, "trouble").unwrap();
        assert!(result.contains("Debug tips"));
    }

    #[test]
    fn extract_section_not_found() {
        assert!(extract_section(SAMPLE_MD, "nonexistent").is_none());
    }

    #[test]
    fn extract_last_section() {
        let result = extract_section(SAMPLE_MD, "Troubleshooting").unwrap();
        assert!(result.starts_with("### Troubleshooting"));
        assert!(result.contains("Debug tips"));
    }

    #[test]
    fn list_sections_shows_headings() {
        let sections = list_sections(SAMPLE_MD);
        assert!(sections.contains("Main Title"));
        assert!(sections.contains("Setup"));
        assert!(sections.contains("Usage"));
        assert!(sections.contains("Prerequisites"));
    }

    #[test]
    fn parse_heading_works() {
        assert_eq!(parse_heading("# Title"), Some((1, "Title")));
        assert_eq!(parse_heading("## Sub"), Some((2, "Sub")));
        assert_eq!(parse_heading("### Deep"), Some((3, "Deep")));
        assert_eq!(parse_heading("not a heading"), None);
        assert_eq!(parse_heading("#"), None); // no text
        assert_eq!(parse_heading(""), None);
    }
}
