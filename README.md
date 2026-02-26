# skillet

A skill registry toolkit for AI agents.

## What is this

AI agent skills (structured prompts that guide agent behavior) are scattered
across GitHub repos, npm packages, and copy-paste threads. There's no
standard way to discover or distribute them at runtime.

Skillet solves this. Use the CLI to search, install, and manage skills
from the command line -- or serve registries to agents over MCP. Skills
follow the
[Agent Skills specification](https://docs.anthropic.com/en/docs/claude-code/skills)
and work with Claude Code, Cursor, Copilot, Windsurf, Gemini CLI, and
any compatible agent.

Think of it like git: skillet is the tool, registries are distributed.
Anyone can create a registry (a git repo with a flat directory structure),
publish skills to it, and share it.

## Quick start

### Use skills

Search a remote registry and install a skill:

```bash
# Search for skills
skillet search rust --remote https://github.com/joshrotenberg/skillet-registry.git

# Show details about a skill
skillet info joshrotenberg/rust-dev --remote https://github.com/joshrotenberg/skillet-registry.git

# Install into your project (for Claude Code)
skillet install joshrotenberg/rust-dev --remote https://github.com/joshrotenberg/skillet-registry.git

# Install for a different agent
skillet install joshrotenberg/rust-dev --target cursor --remote https://github.com/joshrotenberg/skillet-registry.git

# Install for all supported agents at once
skillet install joshrotenberg/rust-dev --target all --remote https://github.com/joshrotenberg/skillet-registry.git

# List installed skills
skillet list
```

Install targets: `claude`, `cursor`, `copilot`, `windsurf`, `gemini`, `all`.

### Create skills

Scaffold a new skill and publish it:

```bash
# Scaffold a new skillpack
skillet init-skill myname/my-skill --description "What this skill does" --category development --tags "rust,testing"

# Edit the generated skill.toml and SKILL.md
$EDITOR myname/my-skill/SKILL.md

# Validate it
skillet validate myname/my-skill

# Pack it (validate + generate manifest + update version history)
skillet pack myname/my-skill

# Publish to a registry (pack + open PR)
skillet publish myname/my-skill --repo joshrotenberg/skillet-registry
```

### Create a registry

```bash
skillet init-registry my-registry
cd my-registry
# Add skills as owner/skill-name/ directories
```

This scaffolds a git repo with the right structure. Add skills as
`owner/skill-name/` directories, each with a `skill.toml` and `SKILL.md`.

### Serve over MCP

Add skillet to your agent's MCP config to give it runtime access to skills:

```json
{
  "mcpServers": {
    "skillet": {
      "command": "skillet",
      "args": ["--remote", "https://github.com/joshrotenberg/skillet-registry.git"]
    }
  }
}
```

Combine multiple registries (local and remote, first match wins):

```json
{
  "mcpServers": {
    "skillet": {
      "command": "skillet",
      "args": [
        "--registry", "/path/to/my-skills",
        "--remote", "https://github.com/joshrotenberg/skillet-registry.git"
      ]
    }
  }
}
```

### What it looks like

**CLI:**

```
$ skillet search rust --remote https://github.com/joshrotenberg/skillet-registry.git

joshrotenberg/rust-dev    Rust development standards and conventions
joshrotenberg/rust-ci     Rust CI/CD pipeline setup
```

**MCP (agent integration):**

```
User: Set up a new Rust project with CI

Agent: Let me search for relevant skills.
       [calls search_skills(query: "rust", category: "development")]

       Found joshrotenberg/rust-dev -- Rust development standards and conventions.
       Let me fetch it.
       [reads skillet://skills/joshrotenberg/rust-dev]

       I'll follow the rust-dev skill's conventions for project setup...
```

## MCP interface

When running as an MCP server, agents discover skills via tools and fetch
content via resource templates.

| Tool / Resource | Purpose |
|---|---|
| `search_skills` | Full-text search with category, tag, and model filters |
| `list_categories` | Browse all skill categories |
| `list_skills_by_owner` | List everything by one publisher |
| `install_skill` | Install a skill to the local filesystem for persistent use |
| `skillet://skills/{owner}/{name}` | Fetch SKILL.md content (latest version) |
| `skillet://skills/{owner}/{name}/{version}` | Fetch a specific version |
| `skillet://metadata/{owner}/{name}` | Fetch skill.toml metadata |
| `skillet://files/{owner}/{name}/{path}` | Fetch extra files (scripts, references, assets) |

## Skill format

A skill is a directory with two required files:

```
owner/skill-name/
  skill.toml       # Registry metadata (classification, compatibility, author)
  SKILL.md         # Agent Skills spec-compatible prompt
  scripts/         # Optional shell/code scripts
  references/      # Optional reference documentation
  assets/          # Optional templates, configs
```

The `skill.toml` carries all the metadata the registry needs for indexing
and filtering:

```toml
[skill]
name = "rust-dev"
owner = "joshrotenberg"
version = "2026.02.24"
description = "Rust development standards and conventions"
trigger = "Use when writing or reviewing Rust code"
license = "MIT"

[skill.author]
name = "Josh Rotenberg"
github = "joshrotenberg"

[skill.classification]
categories = ["development", "rust"]
tags = ["rust", "cargo", "clippy", "fmt", "testing"]

[skill.compatibility]
requires_tool_use = true
requires_vision = false
min_context_tokens = 4096
required_capabilities = ["shell_exec", "file_read", "file_write", "file_edit"]
verified_with = ["claude-opus-4-6", "claude-sonnet-4-6"]
```

The `SKILL.md` is the prompt itself. It uses minimal YAML frontmatter and
is fully compatible with the Agent Skills spec -- you can drop it into
`.claude/skills/` and it works standalone.

## CLI reference

### Use skills

| Command | Description |
|---|---|
| `skillet search <query>` | Search for skills (`*` for all). Supports `--category`, `--tag` filters |
| `skillet info <owner/name>` | Show detailed information about a skill |
| `skillet install <owner/name>` | Install a skill. Supports `--target`, `--global`, `--version` |
| `skillet list` | List installed skills |

### Author skills

| Command | Description |
|---|---|
| `skillet init-skill <path>` | Scaffold a new skillpack. Supports `--description`, `--category`, `--tags` |
| `skillet validate <path>` | Validate a skillpack directory |
| `skillet pack <path>` | Validate + generate manifest + update version history |
| `skillet publish <path> --repo <owner/repo>` | Pack + open a PR against the registry. Supports `--dry-run` |

### Manage registries

| Command | Description |
|---|---|
| `skillet init-registry <path>` | Scaffold a new registry git repo. Supports `--name` |
| `skillet [serve]` | Run the MCP server (default when no subcommand is given) |

### Server options

| Flag | Description |
|---|---|
| `--registry <path>` | Local registry directory (repeatable) |
| `--remote <url>` | Git URL to clone and serve from (repeatable) |
| `--refresh-interval <duration>` | How often to pull from remotes (default: `5m`, `0` to disable) |
| `--cache-dir <path>` | Directory to clone remote registries into |
| `--subdir <path>` | Subdirectory within registries containing skills |
| `--watch` | Watch local registries for changes and auto-reload |
| `--http <addr>` | Serve over HTTP instead of stdio (e.g. `0.0.0.0:8080`) |
| `--log-level <level>` | Log level (default: `info`) |

## Installation

Building from source:

```
cargo install --git https://github.com/joshrotenberg/grimoire.git
```

Requires Rust 1.90 or later.

## Status

Working prototype. The skill schema is stable, both the CLI and MCP
interface are functional, and there's a
[sample registry](https://github.com/joshrotenberg/skillet-registry)
with skills across development, devops, and security categories. The
registry format is flat files, git-backed, with auditable history.

See [issues](https://github.com/joshrotenberg/grimoire/issues) for the
roadmap.
