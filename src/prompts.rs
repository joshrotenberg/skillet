//! Register skills as MCP prompts via tower-mcp's DynamicPromptRegistry.
//!
//! Each skill in the index becomes an MCP prompt, namespaced as `owner_skill-name`.
//! On index refresh, stale prompts are unregistered and new ones registered,
//! with a `prompts/list_changed` notification emitted automatically.

use tower_mcp::PromptBuilder;
use tower_mcp::registry::DynamicPromptRegistry;

use crate::state::SkillIndex;

/// Register all skills from the index as MCP prompts.
///
/// Prompt names are namespaced as `owner_skill-name` to avoid collisions
/// across repos. The prompt description comes from skill.toml metadata.
/// The prompt content is the full SKILL.md text.
pub fn register_all(registry: &DynamicPromptRegistry, index: &SkillIndex) {
    for ((owner, name), entry) in &index.skills {
        let Some(latest) = entry.latest() else {
            continue;
        };

        let prompt_name = format!("{owner}_{name}");
        let description = &latest.metadata.skill.description;
        let content = &latest.skill_md;

        if content.is_empty() {
            tracing::debug!(
                prompt = %prompt_name,
                "Skipping prompt with empty SKILL.md"
            );
            continue;
        }

        let prompt = PromptBuilder::new(&prompt_name)
            .description(description)
            .user_message(content);

        registry.register(prompt);

        tracing::debug!(prompt = %prompt_name, "Registered skill as prompt");
    }
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
    use std::collections::HashMap;

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
                content_hash: None,
                integrity_ok: None,
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
        let _ = router; // keep alive

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

        // We can't directly inspect the registry, but we can verify no panics
        // and that the function completes. Full integration testing happens
        // via the HTTP/MCP tests.
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
        // Should not panic on empty content
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
        // old-skill should be unregistered, new-skill should be registered
    }
}
