---
name: skill-author
description: Authoring and publishing skills for the skillet registry. Covers the skill format, validation, packing, and publishing workflow.
---

## Skill Authoring Guide

### Skill Format

A skillpack is a directory with two required files:

```
owner/skill-name/
  skill.toml       # metadata (required for publishing)
  SKILL.md         # the skill prompt (required)
  scripts/         # optional executable scripts
  references/      # optional reference docs
  assets/          # optional templates, configs
```

**Zero-config mode**: a directory with only `SKILL.md` is discoverable.
Metadata is inferred from the directory name and content. `skill.toml` is
only required for publishing.

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
- `rules/` -- rule files (npm-style registries)

All extra files are included in the manifest and installed alongside SKILL.md.

### Scaffolding

```bash
skillet init-skill owner/skill-name
skillet init-skill owner/skill-name --description "My skill" --category development --tags "rust,testing"
```

### Validation

```bash
skillet validate path/to/skill      # structural check + safety scan
skillet validate path/to/skill --skip-safety   # skip safety scan
skillet validate path/to/skill --lenient       # allow missing optional fields
```

Validation checks: skill.toml parse, required fields, SKILL.md exists,
owner/name consistency, and safety scanning.

Exit codes: 0 = pass, 1 = structural error, 2 = safety danger.

### Safety Scanning

Skillet runs static analysis on skill content by default. 13 regex rules
check for:

- **Danger** (blocks publish): shell injection, hardcoded credentials,
  private keys, known token patterns
- **Warning** (informational): exfiltration patterns, safety bypasses,
  obfuscation, over-broad capabilities

Suppression via `[safety].suppress` in `~/.config/skillet/config.toml`.

### Packing

```bash
skillet pack path/to/skill
```

Validates, generates `MANIFEST.sha256` (content hashes), and updates
`versions.toml` (version history). Run before publishing.

### Publishing

```bash
skillet publish path/to/skill --repo owner/registry-repo
skillet publish path/to/skill --repo owner/registry-repo --dry-run
skillet publish path/to/skill --repo owner/registry-repo --registry-path custom/path
```

Publishing: packs the skill, forks the registry repo, creates a branch,
copies files, and opens a PR via `gh` CLI.

### Project Manifest

Embed skills in any repository with `skillet.toml`:

```bash
skillet init-project path --skill     # single inline skill
skillet init-project path --multi     # multi-skill directory
skillet init-project path --registry  # registry configuration
```
