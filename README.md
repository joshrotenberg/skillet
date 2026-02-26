# skillet

A skill registry toolkit for AI agents.

## What is this

AI agent skills (structured prompts that guide agent behavior) are scattered
across GitHub repos, npm packages, and copy-paste threads. There's no
standard way to discover or distribute them at runtime.

Skillet is a toolkit for building and serving skill registries. Create your
own registry, publish skills to it, and serve it to agents over MCP -- or
use the CLI directly. Skills follow the
[Agent Skills specification](https://docs.anthropic.com/en/docs/claude-code/skills)
and work standalone in `.claude/skills/` or any compatible agent.

Think of it like git: the tool is the thing, registries are distributed.

## Quick start

### Create a registry

```bash
skillet init-registry my-skills
cd my-skills
```

This scaffolds a git repo with the right structure. Add skills as
`owner/skill-name/` directories, each with a `skill.toml` and `SKILL.md`.

### Serve it

As an MCP server (add to your agent's MCP config):

```json
{
  "mcpServers": {
    "skillet": {
      "command": "skillet",
      "args": ["--registry", "/path/to/my-skills"]
    }
  }
}
```

From a remote git repo:

```json
{
  "mcpServers": {
    "skillet": {
      "command": "skillet",
      "args": [
        "--remote", "https://github.com/joshrotenberg/skillet-registry.git"
      ]
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

```
User: Set up a new Rust project with CI

Agent: Let me search for relevant skills.
       [calls search_skills(query: "rust", category: "development")]

       Found joshrotenberg/rust-dev -- Rust development standards and conventions.
       Let me fetch it.
       [reads skillet://skills/joshrotenberg/rust-dev]

       I'll follow the rust-dev skill's conventions for project setup...
```

## How it works

Agents discover skills via MCP tools, fetch content via resource templates,
and use it inline within their current context. There is no installation
step -- the skill content goes directly into the agent's working memory.

| Tool / Resource | Purpose |
|---|---|
| `search_skills` | Full-text search with category, tag, and model filters |
| `list_categories` | Browse all skill categories |
| `list_skills_by_owner` | List everything by one publisher |
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

## Publishing skills

Skillet provides a three-step pipeline for publishing skills to a registry:

1. `skillet validate <path>` -- check that skill.toml and SKILL.md are valid
2. `skillet pack <path>` -- validate, generate content manifest, update version history
3. `skillet publish <path> --repo <owner/repo>` -- pack and open a PR against the registry

## CLI reference

```
skillet [serve] [OPTIONS]
skillet validate <PATH>
skillet pack <PATH>
skillet publish <PATH> --repo <OWNER/REPO> [--dry-run]
skillet init-registry <PATH> [--name <NAME>]
```

Server options:

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

Working prototype. The skill schema is stable, the MCP interface is
functional, and there's a
[sample registry](https://github.com/joshrotenberg/skillet-registry)
with skills across development, devops, and security categories. The
registry format is flat files, git-backed, with auditable history.

See [issues](https://github.com/joshrotenberg/grimoire/issues) for the
roadmap.
