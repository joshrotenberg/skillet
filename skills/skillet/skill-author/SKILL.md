---
name: skill-author
description: Authoring skills for skillet. Covers the skill format, discovery via suggest, and project manifests.
version: 2026.02.27
trigger: Use when the user wants to create a new skill, validate a skillpack, or publish a skill to a registry
license: MIT OR Apache-2.0
author: Josh Rotenberg
categories:
  - tools
  - development
tags:
  - skillet
  - skills
  - authoring
  - publishing
  - validate
---

## Skill Authoring Guide

### Skill Format

A skill is a directory with a required `SKILL.md` file:

```
owner/skill-name/
  SKILL.md         # the skill prompt with YAML frontmatter (required)
  scripts/         # optional executable scripts
  references/      # optional reference docs
  assets/          # optional templates, configs
```

### SKILL.md

Agent Skills spec-compatible markdown with YAML frontmatter for metadata:

```markdown
---
name: my-skill
description: What this skill does and when to use it.
version: 2026.02.27
trigger: When to activate this skill
license: MIT
author: Your Name
categories:
  - development
tags:
  - rust
  - testing
---

## My Skill

Instructions for the agent...
```

**Frontmatter fields**: `name`, `description`, `version`, `trigger`,
`license`, `author`, `categories`, `tags`. All optional -- skillet infers
what it can from the directory name, git remote, and content.

**Zero-config mode**: a directory with only a bare `SKILL.md` (no
frontmatter) is still discoverable. Frontmatter improves search results.

### Extra Files

- `scripts/` -- executable scripts the skill references
- `references/` -- reference documentation for context
- `assets/` -- templates, configs, or other static files

### Scaffolding

```bash
skillet init-skill owner/skill-name
skillet init-skill owner/skill-name --description "My skill" --category development --tags "rust,testing"
```

### Discovery via Suggest

Use `[[suggest]]` in your repo's `skillet.toml` to create a decentralized
discovery graph. Each suggest entry points to another repo that skillet
can traverse to find more skills:

```toml
[[suggest]]
url = "https://github.com/org/their-skills.git"
description = "Skills from org"
```

This lets users discover skills across repos without a central authority.

### Project Manifest

Embed skills in any repository with `skillet.toml`:

```bash
skillet init-project path --skill     # single inline skill
skillet init-project path --multi     # multi-skill directory
skillet init-project path --registry  # repo configuration
```

```toml
[project]
name = "my-tool"
description = "A CLI tool"

# Single inline skill (SKILL.md at project root)
[skill]
name = "my-tool-usage"
description = "How to use my-tool"

# Or multiple skills in a subdirectory
[skills]
path = ".skillet"
members = ["api", "debug"]
```
