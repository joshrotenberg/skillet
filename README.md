# skillet

A skill registry for AI agents, served over MCP.

## What is this

AI agent skills (structured prompts that guide agent behavior) are scattered
across GitHub repos, npm packages, and copy-paste threads. There's no
standard way to discover or distribute them at runtime. Skillet is an MCP
server that gives agents searchable, categorized access to a skill registry.
Connect it, search for what you need, and use skills inline -- no
installation step, no package manager, no build process. Skills follow the
[Agent Skills specification](https://docs.anthropic.com/en/docs/claude-code/skills)
and work standalone in `.claude/skills/` or any compatible agent.

## Quick start

Add skillet to your MCP configuration to connect to a remote registry:

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

Once connected, agents discover and use skills through normal MCP
tool calls. Here's what that looks like in practice:

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
required_tools = ["bash", "read", "write", "edit"]
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

## Running the server

```
skillet [OPTIONS]
skillet validate <PATH>
skillet pack <PATH>
skillet publish <PATH> --repo <OWNER/REPO> [--dry-run]
```

Server options:

| Flag | Description |
|---|---|
| `--registry <path>` | Path to a local registry directory |
| `--remote <url>` | Git URL to clone and serve from |
| `--refresh-interval <duration>` | How often to pull from remote (default: `5m`, `0` to disable) |
| `--cache-dir <path>` | Directory to clone remote registries into |
| `--subdir <path>` | Subdirectory within registry containing skills |
| `--watch` | Watch local registry for changes and auto-reload |
| `--log-level <level>` | Log level (default: `info`) |

Building from source:

```
cargo build --release
```

Requires Rust 1.90 or later.

## Status

Working prototype. The skill schema is stable, the MCP interface is
functional, and the [default registry](https://github.com/joshrotenberg/skillet-registry)
has 11 skills across development, devops, and security categories. The
registry format follows the same principles as crates.io -- flat files,
git-backed, auditable history.

See [issues](https://github.com/joshrotenberg/grimoire/issues) for the
roadmap.
