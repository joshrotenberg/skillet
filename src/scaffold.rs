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

/// Options for generating a `skillet.toml` project manifest.
pub struct InitProjectOptions<'a> {
    /// Project name (defaults to directory name)
    pub name: &'a str,
    /// Project description
    pub description: Option<&'a str>,
    /// Include a `[skill]` section
    pub include_skill: bool,
    /// Include a `[skills]` section
    pub include_multi: bool,
    /// Include a `[registry]` section
    pub include_registry: bool,
}

/// Generate a `skillet.toml` project manifest at the given path.
///
/// Creates the file (does not create the directory). Returns an error if
/// `skillet.toml` already exists.
pub fn init_project(path: &Path, opts: &InitProjectOptions) -> crate::error::Result<()> {
    let manifest_path = path.join("skillet.toml");
    if manifest_path.exists() {
        return Err(Error::Scaffold(format!(
            "skillet.toml already exists at {}",
            path.display()
        )));
    }

    let content = render_skillet_toml(opts);
    std::fs::write(&manifest_path, content)?;

    // If [skill] is included, create a template SKILL.md at the project root
    if opts.include_skill {
        let md_path = path.join("SKILL.md");
        if !md_path.exists() {
            let desc = opts.description.unwrap_or("A skill for this project");
            let md = render_skill_md(opts.name, desc);
            std::fs::write(&md_path, md)?;
        }
    }

    // If [skills] is included, create the .skillet/ directory
    if opts.include_multi {
        let skills_dir = path.join(".skillet");
        if !skills_dir.exists() {
            std::fs::create_dir_all(&skills_dir)?;
        }
    }

    Ok(())
}

/// Render a `skillet.toml` template.
fn render_skillet_toml(opts: &InitProjectOptions) -> String {
    let mut content = String::new();

    // [project] section (always included)
    content.push_str("[project]\n");
    content.push_str(&format!("name = \"{}\"\n", opts.name));
    if let Some(desc) = opts.description {
        content.push_str(&format!("description = \"{desc}\"\n"));
    }

    // Auto-populate author from git config
    let github_user = std::process::Command::new("git")
        .args(["config", "--global", "github.user"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let git_name = std::process::Command::new("git")
        .args(["config", "--global", "user.name"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    if github_user.is_some() || git_name.is_some() {
        content.push_str("\n[[project.authors]]\n");
        if let Some(ref name) = git_name {
            content.push_str(&format!("name = \"{name}\"\n"));
        }
        if let Some(ref gh) = github_user {
            content.push_str(&format!("github = \"{gh}\"\n"));
        }
    }

    // [skill] section
    if opts.include_skill {
        content.push_str(&format!("\n[skill]\nname = \"{name}\"\n", name = opts.name));
        if let Some(desc) = opts.description {
            content.push_str(&format!("description = \"{desc}\"\n"));
        }
    }

    // [skills] section
    if opts.include_multi {
        content.push_str("\n[skills]\npath = \".skillet\"\n");
    }

    // [registry] section
    if opts.include_registry {
        content.push_str(&format!(
            "\n[registry]\nname = \"{name}\"\nversion = 1\n",
            name = opts.name
        ));
        if let Some(desc) = opts.description {
            content.push_str(&format!("description = \"{desc}\"\n"));
        }
    }

    content
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

    #[test]
    fn test_init_project_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        let opts = InitProjectOptions {
            name: "test-project",
            description: Some("A test project"),
            include_skill: false,
            include_multi: false,
            include_registry: false,
        };

        init_project(path, &opts).unwrap();

        let content = std::fs::read_to_string(path.join("skillet.toml")).unwrap();
        assert!(content.contains("name = \"test-project\""));
        assert!(content.contains("description = \"A test project\""));
        assert!(!content.contains("[skill]"));
        assert!(!content.contains("[skills]"));
        assert!(!content.contains("[registry]"));

        // Should parse as valid SkilletToml
        let manifest = crate::project::load_skillet_toml(path).unwrap().unwrap();
        assert_eq!(
            manifest.project.as_ref().unwrap().name.as_deref(),
            Some("test-project")
        );
    }

    #[test]
    fn test_init_project_with_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        let opts = InitProjectOptions {
            name: "my-tool",
            description: Some("A CLI tool"),
            include_skill: true,
            include_multi: false,
            include_registry: false,
        };

        init_project(path, &opts).unwrap();

        let content = std::fs::read_to_string(path.join("skillet.toml")).unwrap();
        assert!(content.contains("[skill]"));
        assert!(content.contains("name = \"my-tool\""));

        // SKILL.md should be created
        assert!(path.join("SKILL.md").is_file());

        // Should parse correctly
        let manifest = crate::project::load_skillet_toml(path).unwrap().unwrap();
        assert!(manifest.skill.is_some());
    }

    #[test]
    fn test_init_project_with_multi() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        let opts = InitProjectOptions {
            name: "my-project",
            description: None,
            include_skill: false,
            include_multi: true,
            include_registry: false,
        };

        init_project(path, &opts).unwrap();

        let content = std::fs::read_to_string(path.join("skillet.toml")).unwrap();
        assert!(content.contains("[skills]"));
        assert!(content.contains("path = \".skillet\""));

        // .skillet/ directory should be created
        assert!(path.join(".skillet").is_dir());
    }

    #[test]
    fn test_init_project_with_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        let opts = InitProjectOptions {
            name: "my-registry",
            description: Some("Skills registry"),
            include_skill: false,
            include_multi: false,
            include_registry: true,
        };

        init_project(path, &opts).unwrap();

        let content = std::fs::read_to_string(path.join("skillet.toml")).unwrap();
        assert!(content.contains("[registry]"));
        assert!(content.contains("version = 1"));

        // Should be loadable as a registry config
        let manifest = crate::project::load_skillet_toml(path).unwrap().unwrap();
        let config = manifest.into_registry_config().unwrap();
        assert_eq!(config.registry.name, "my-registry");
    }

    #[test]
    fn test_init_project_errors_on_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();

        // Create skillet.toml first
        std::fs::write(path.join("skillet.toml"), "[project]\n").unwrap();

        let opts = InitProjectOptions {
            name: "test",
            description: None,
            include_skill: false,
            include_multi: false,
            include_registry: false,
        };

        let result = init_project(path, &opts);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }
}
