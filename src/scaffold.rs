//! Skill scaffolding: generate a new skillpack directory with template files.

use std::path::Path;

use crate::error::Error;

/// Initialize a new skillpack at the given path.
///
/// Creates the directory with template `skill.toml` and `SKILL.md` files.
/// The `owner` and `name` are inferred by the caller from the path components.
pub fn init_skill(
    path: &Path,
    owner: &str,
    name: &str,
    description: &str,
    categories: &[String],
    tags: &[String],
) -> crate::error::Result<()> {
    if path.exists() {
        return Err(Error::Scaffold(format!(
            "{} already exists",
            path.display()
        )));
    }

    std::fs::create_dir_all(path)?;

    let skill_toml = render_skill_toml(owner, name, description, categories, tags);
    std::fs::write(path.join("skill.toml"), &skill_toml)?;

    let skill_md = render_skill_md(name, description);
    std::fs::write(path.join("SKILL.md"), &skill_md)?;

    Ok(())
}

/// Render a template `skill.toml` with the given values.
fn render_skill_toml(
    owner: &str,
    name: &str,
    description: &str,
    categories: &[String],
    tags: &[String],
) -> String {
    let categories_str = categories
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let tags_str = tags
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"[skill]
name = "{name}"
owner = "{owner}"
version = "0.1.0"
description = "{description}"
trigger = "Use when ..."
license = "MIT"

[skill.author]
name = "{owner}"
github = "{owner}"

[skill.classification]
categories = [{categories_str}]
tags = [{tags_str}]

[skill.compatibility]
requires_tool_use = true
required_capabilities = ["shell_exec", "file_read", "file_write", "file_edit"]
verified_with = []
"#
    )
}

/// Render a template `SKILL.md` with frontmatter and section scaffolding.
fn render_skill_md(name: &str, description: &str) -> String {
    // Title-case the name: replace hyphens with spaces, capitalize words
    let title = name
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    format!("{upper}{}", chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        r#"---
name: {name}
description: {description}
---

# {title}

{description}

## Usage

<!-- Describe when and how an agent should use this skill -->

## Instructions

<!-- The core instructions for the agent -->
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_skill_creates_files() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_path = tmp.path().join("myowner/my-skill");

        init_skill(&skill_path, "myowner", "my-skill", "A test skill", &[], &[]).unwrap();

        assert!(skill_path.join("skill.toml").is_file());
        assert!(skill_path.join("SKILL.md").is_file());

        // Verify skill.toml content
        let toml_content = std::fs::read_to_string(skill_path.join("skill.toml")).unwrap();
        assert!(toml_content.contains("name = \"my-skill\""));
        assert!(toml_content.contains("owner = \"myowner\""));
        assert!(toml_content.contains("version = \"0.1.0\""));
        assert!(toml_content.contains("description = \"A test skill\""));

        // Verify SKILL.md content
        let md_content = std::fs::read_to_string(skill_path.join("SKILL.md")).unwrap();
        assert!(md_content.contains("name: my-skill"));
        assert!(md_content.contains("# My Skill"));
        assert!(md_content.contains("A test skill"));
    }

    #[test]
    fn test_init_skill_passes_validation() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_path = tmp.path().join("testowner/test-skill");

        init_skill(
            &skill_path,
            "testowner",
            "test-skill",
            "A test skill",
            &[],
            &[],
        )
        .unwrap();

        let result = crate::validate::validate_skillpack(&skill_path)
            .expect("scaffolded skill should pass validation");
        assert_eq!(result.owner, "testowner");
        assert_eq!(result.name, "test-skill");
        assert_eq!(result.version, "0.1.0");
    }

    #[test]
    fn test_init_skill_with_categories_and_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_path = tmp.path().join("acme/docker-ops");

        init_skill(
            &skill_path,
            "acme",
            "docker-ops",
            "Docker operations",
            &["devops".to_string(), "containers".to_string()],
            &["docker".to_string(), "compose".to_string()],
        )
        .unwrap();

        let toml_content = std::fs::read_to_string(skill_path.join("skill.toml")).unwrap();
        assert!(toml_content.contains("\"devops\""));
        assert!(toml_content.contains("\"containers\""));
        assert!(toml_content.contains("\"docker\""));
        assert!(toml_content.contains("\"compose\""));

        // Should still validate
        let result = crate::validate::validate_skillpack(&skill_path)
            .expect("should pass validation with categories/tags");
        assert_eq!(
            result
                .metadata
                .skill
                .classification
                .as_ref()
                .unwrap()
                .categories,
            vec!["devops", "containers"]
        );
        assert_eq!(
            result.metadata.skill.classification.as_ref().unwrap().tags,
            vec!["docker", "compose"]
        );
    }

    #[test]
    fn test_init_skill_errors_on_existing_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_path = tmp.path().join("owner/existing");

        std::fs::create_dir_all(&skill_path).unwrap();

        let result = init_skill(&skill_path, "owner", "existing", "test", &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }
}
