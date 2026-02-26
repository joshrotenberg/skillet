<p align="center">
  <img src="assets/logo.png" alt="Skillet" width="360">
</p>

<p align="center">
  <a href="https://github.com/joshrotenberg/skillet/actions/workflows/ci.yml"><img src="https://github.com/joshrotenberg/skillet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/skillet-mcp"><img src="https://img.shields.io/crates/v/skillet-mcp.svg" alt="crates.io"></a>
  <a href="https://github.com/joshrotenberg/skillet/blob/main/Cargo.toml"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue" alt="License"></a>
</p>

# skillet

A skill registry toolkit for AI agents.

## What is skillet

AI agent skills -- structured prompts that guide agent behavior -- are
scattered across GitHub repos, npm packages, and copy-paste threads.
There's no standard way to discover, distribute, or manage them.

Skillet fixes this. Search, install, and manage skills from the CLI, or
serve registries to agents over MCP so they can discover skills at
runtime. Skills follow the
[Agent Skills specification](https://docs.anthropic.com/en/docs/claude-code/skills)
and work with Claude Code, Cursor, Copilot, Windsurf, Gemini CLI, and
any compatible agent.

Think of it like git: skillet is the tool, registries are distributed.
Anyone can create a registry (a git repo), publish skills to it, and
share it. The same binary is always both client and server -- the CLI
commands and the MCP server share the same index and search engine.

## Install

```bash
cargo install skillet-mcp
```

Or build from source:

```bash
cargo install --git https://github.com/joshrotenberg/skillet.git
```

Requires Rust 1.90 or later.

## Quick start

### Set up

```bash
skillet setup
```

This writes a default config at `~/.config/skillet/config.toml` with the
[official registry](https://github.com/joshrotenberg/skillet-registry)
and prints an MCP config snippet you can paste into your agent. After
that, CLI commands work without `--remote` flags.

### Search and install skills

```bash
# Browse everything
skillet search *

# Search by keyword
skillet search rust

# Filter by category, tag, or owner
skillet search * --category development
skillet search * --tag pytest
skillet search * --owner joshrotenberg

# See what categories exist
skillet categories

# Show details about a skill
skillet info joshrotenberg/rust-dev

# Install into your project
skillet install joshrotenberg/rust-dev

# Install for a specific agent
skillet install joshrotenberg/rust-dev --target claude

# Install for all supported agents
skillet install joshrotenberg/rust-dev --target all

# See what's installed
skillet list
```

The default install target is `agents` (writes to `.agents/skills/`),
which is the cross-agent convention. Other targets: `claude`, `cursor`,
`copilot`, `windsurf`, `gemini`, `all`.

### Create and publish skills

```bash
# Scaffold a new skill
skillet init-skill myname/my-skill \
  --description "What this skill does" \
  --category development \
  --tags "rust,testing"

# Edit the prompt
$EDITOR myname/my-skill/SKILL.md

# Validate (includes safety scanning)
skillet validate myname/my-skill

# Pack (validate + generate manifest + update version history)
skillet pack myname/my-skill

# Publish to a registry (pack + open PR via gh CLI)
skillet publish myname/my-skill --repo owner/registry
```

### Serve over MCP

Add skillet to your agent's MCP config to give it runtime access to
skills:

```json
{
  "mcpServers": {
    "skillet": {
      "command": "skillet"
    }
  }
}
```

Or with Docker (zero install):

```json
{
  "mcpServers": {
    "skillet": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "ghcr.io/joshrotenberg/skillet"]
    }
  }
}
```

That's it. Skillet auto-discovers the official registry and any skills
already installed on your machine. For custom registries:

```json
{
  "mcpServers": {
    "skillet": {
      "command": "skillet",
      "args": [
        "--registry", "/path/to/local-skills",
        "--remote", "https://github.com/acme/team-skills.git"
      ]
    }
  }
}
```

Multiple `--registry` and `--remote` flags can be combined. First match
wins on name collisions.

### What it looks like

**CLI:**

```
$ skillet search rust

  joshrotenberg/rust-dev    Rust development standards and conventions
  joshrotenberg/rust-ci     Rust CI/CD pipeline setup

Found 2 skills matching "rust"
```

**MCP (agent integration):**

```
User: Set up a new Rust project with CI

Agent: Let me search for relevant skills.
       [calls search_skills(query: "rust", category: "development")]

       Found joshrotenberg/rust-dev. Let me fetch its instructions.
       [reads skillet://skills/joshrotenberg/rust-dev]

       I'll follow the rust-dev skill conventions for project setup...
```

## Features

### Multi-registry support

Aggregate local directories and remote git repos. Skillet clones and
periodically refreshes remotes in the background. Use `--refresh-interval`
to control how often (default: every 5 minutes).

```bash
# CLI: explicit registries
skillet search rust \
  --registry /path/to/local \
  --remote https://github.com/acme/team-skills.git

# Or configure defaults in ~/.config/skillet/config.toml
```

### Local skill discovery

Skillet auto-discovers skills already installed in agent directories
(`~/.claude/skills/`, `~/.agents/skills/`, `~/.cursor/skills/`, etc.)
and includes them alongside registry skills. No extra config required.

### Safety scanning

Static analysis runs automatically during `validate`, `pack`, and
`publish`. 13 regex-based rules detect dangerous patterns:

- **Danger** (blocks publish): shell injection, hardcoded credentials,
  private keys, token patterns
- **Warning** (informational): exfiltration URLs, safety bypasses,
  obfuscation, over-broad capabilities

```bash
skillet validate myname/my-skill     # includes safety scan
skillet validate myname/my-skill --skip-safety  # bypass if needed
```

Suppress specific rules in config:

```toml
# ~/.config/skillet/config.toml
[safety]
suppress = ["exfiltration-curl"]
```

### Trust and integrity

Content hashing (SHA256) verifies skills haven't been tampered with.
Three trust tiers:

- **Trusted** -- skills from registries you've explicitly marked as
  trusted
- **Reviewed** -- skills whose content hash you've pinned after review
- **Unknown** -- everything else (warns on install)

```bash
# Trust a registry
skillet trust add-registry https://github.com/acme/team-skills.git

# Pin a specific skill's content hash
skillet trust pin joshrotenberg/rust-dev

# Audit installed skills against pinned hashes
skillet audit

# See trust status
skillet trust list
```

Content hashes are auto-pinned on install (configurable via
`[trust].auto_pin` in config).

### BM25 full-text search

Search indexes skill names, descriptions, categories, tags, and SKILL.md
content. Results are ranked by relevance using BM25 scoring with
field-weighted boosting.

### Persistent disk cache

The skill index is cached to disk and refreshed based on TTL (default: 5
minutes). No rebuilding the index from scratch every time. Use
`--no-cache` to bypass.

### Filesystem watching

Use `--watch` with the MCP server to auto-reload when local registry
files change. Useful during skill development.

### Configurable server exposure

Control which MCP tools and resources are exposed:

```bash
# Read-only: hide install_skill
skillet --read-only

# Explicit allowlist
skillet --tools search,categories,info --resources skills,metadata

# Or configure in config.toml:
# [server]
# tools = ["search", "categories", "info"]
# resources = ["skills", "metadata"]
```

## MCP interface

When running as an MCP server, agents discover skills via tools and
fetch content via resource templates.

### Tools

| Tool | Purpose |
|---|---|
| `search_skills` | Full-text search with category, tag, and model filters |
| `list_categories` | Browse all skill categories with counts |
| `list_skills_by_owner` | List all skills by a specific publisher |
| `info_skill` | Detailed information about a specific skill |
| `compare_skills` | Side-by-side comparison of two or more skills |
| `skill_status` | Check install status and trust tier for a skill |
| `install_skill` | Install a skill to the local filesystem |
| `list_installed` | List all skills installed on the local filesystem |
| `audit_skills` | Verify installed skills against pinned content hashes |
| `setup_config` | Generate initial configuration at `~/.config/skillet/config.toml` |
| `validate_skill` | Validate a skillpack directory for correctness and safety |

### Resources

| Resource | Purpose |
|---|---|
| `skillet://skills/{owner}/{name}` | Fetch SKILL.md content (latest version) |
| `skillet://skills/{owner}/{name}/{version}` | Fetch a specific version |
| `skillet://metadata/{owner}/{name}` | Fetch skill.toml metadata |
| `skillet://files/{owner}/{name}/{path}` | Fetch extra files (scripts, references, assets) |

## CLI reference

### Getting started

| Command | Description |
|---|---|
| `skillet setup` | Generate initial config. Supports `--target`, `--remote`, `--force` |

### Use skills

| Command | Description |
|---|---|
| `skillet search <query>` | Search for skills (`*` for all). Supports `--category`, `--tag`, `--owner` |
| `skillet categories` | List all skill categories with counts |
| `skillet info <owner/name>` | Show detailed information about a skill |
| `skillet install <owner/name>` | Install a skill. Supports `--target`, `--global`, `--version` |
| `skillet list` | List installed skills |

### Author skills

| Command | Description |
|---|---|
| `skillet init-skill <path>` | Scaffold a new skillpack. Supports `--description`, `--category`, `--tags` |
| `skillet validate <path>` | Validate a skillpack (includes safety scan). Supports `--skip-safety` |
| `skillet pack <path>` | Validate + generate manifest + update version history |
| `skillet publish <path> --repo <owner/repo>` | Pack + open a PR against the registry. Supports `--dry-run` |

### Manage registries

| Command | Description |
|---|---|
| `skillet init-registry <path>` | Scaffold a new registry git repo. Supports `--name`, `--description` |
| `skillet [serve]` | Run the MCP server (default when no subcommand) |

### Trust and audit

| Command | Description |
|---|---|
| `skillet trust add-registry <url>` | Mark a registry as trusted |
| `skillet trust remove-registry <url>` | Remove a trusted registry |
| `skillet trust pin <owner/name>` | Pin a skill's content hash |
| `skillet trust unpin <owner/name>` | Remove a content hash pin |
| `skillet trust list` | Show trusted registries and pinned skills |
| `skillet audit` | Verify installed skills against pinned hashes |

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
| `--read-only` | Don't expose the install_skill tool |
| `--tools <list>` | Explicit tool allowlist (comma-separated) |
| `--resources <list>` | Explicit resource allowlist (comma-separated) |

## Configuration

Skillet reads `~/.config/skillet/config.toml` for defaults. Run
`skillet setup` to generate one, or create it manually:

```toml
[install]
targets = ["agents"]    # default install target(s)
global = false          # install globally vs project-local

[registries]
remote = ["https://github.com/joshrotenberg/skillet-registry.git"]
local = []

[cache]
enabled = true
ttl = "5m"              # index cache time-to-live

[safety]
suppress = []           # rule IDs to suppress

[trust]
unknown_policy = "warn" # "warn", "prompt", or "block"
auto_pin = true         # pin content hash on install

[server]
tools = []              # empty = expose all
resources = []          # empty = expose all
discover_local = true   # auto-discover installed skills
```

## Skill format

A skill is a directory with two required files:

```
owner/skill-name/
  skill.toml       # Registry metadata
  SKILL.md         # Agent-compatible prompt
  scripts/         # Optional executable scripts
  references/      # Optional reference docs
  assets/          # Optional templates, configs
  versions.toml    # Version history (generated by pack)
  MANIFEST.sha256  # Content hashes (generated by pack)
```

The `skill.toml` carries metadata for indexing and search:

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

The `SKILL.md` is the prompt itself -- fully compatible with the
[Agent Skills specification](https://docs.anthropic.com/en/docs/claude-code/skills).
Drop it into `.claude/skills/` or any agent's skill directory and it
works standalone.

## Status

v0.1.0. The skill format is stable, both the CLI and MCP interface are
functional, and there's an
[official registry](https://github.com/joshrotenberg/skillet-registry)
with skills across development, devops, and security categories.

See [open issues](https://github.com/joshrotenberg/skillet/issues) for
what's next.

## License

MIT OR Apache-2.0
