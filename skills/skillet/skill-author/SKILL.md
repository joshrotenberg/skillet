---
name: skill-author
description: Authoring skills for skillet. Covers the skill format, discovery via suggest, and project manifests.
---

## Skill Authoring Guide

### Skill Format

A skill is a directory with a required `SKILL.md` file:

```
owner/skill-name/
  SKILL.md         # the skill prompt (required)
  skill.toml       # metadata for indexing (optional, inferred if absent)
  scripts/         # optional executable scripts
  references/      # optional reference docs
  assets/          # optional templates, configs
```

**Zero-config mode**: a directory with only `SKILL.md` is discoverable.
Metadata (name, owner, version, description) is inferred from the directory
name, git remote, and SKILL.md content. `skill.toml` adds explicit metadata
for better search results.

### skill.toml

```toml
[skill]
name = "my-skill"
owner = "myname"
version = "2026.02.27"
description = "What this skill does"
trigger = "When to activate this skill"
license = "MIT"

[skill.author]
name = "Your Name"
github = "yourgithub"

[skill.classification]
categories = ["development"]
tags = ["rust", "testing"]

[skill.compatibility]
requires_tool_use = true
requires_vision = false
min_context_tokens = 4096
required_capabilities = ["shell_exec", "file_read"]
required_mcp_servers = []
verified_with = ["claude-opus-4-6"]
```

### SKILL.md

Agent Skills spec-compatible markdown. Optional YAML frontmatter:

```markdown
---
name: my-skill
description: Short description for discovery.
---

## My Skill

Instructions for the agent...
```

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
