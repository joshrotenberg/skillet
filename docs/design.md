# Skillet Design Document

A skill registry for AI agents, distributed over MCP.

Working name: skillet. Previous name: grimoire. The logo is a cartoon cast
iron skillet cooking up some markdown.

## Problem Statement

The Agent Skills specification (Dec 2025) standardized the skill format
across Claude Code, Cursor, Copilot, Gemini CLI, and others. Distribution
is the unsolved problem. Skills are scattered across GitHub repos, npm
packages, and copy-paste. There is no Homebrew or crates.io for agent
skills.

| Method | Limitation |
|--------|-----------|
| Org provisioning (Team/Enterprise) | No community sharing |
| GitHub repos (awesome-* lists) | Manual discovery, no versioning |
| npm packages | Wrong abstraction (skills aren't JS packages) |
| Copy-paste into ~/.claude/skills/ | No versioning, no discoverability |
| Nascent registries (SkillUse, SkillDock, etc.) | Fragmented, no MCP interface |

## Core Concept

Connect an MCP server, get runtime access to a searchable, categorized
repository of skills. No local installation, no package manager, no build
step.

```json
{
  "mcpServers": {
    "skillet": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "ghcr.io/skillet/server:latest"],
      "env": {
        "SKILLET_REMOTE": "https://github.com/skillet-registry/index.git"
      }
    }
  }
}
```

Add this, restart your agent, and you have access to the entire registry
with dynamic updates.

## Design Principles

- **Owner/name namespacing**: `joshrotenberg/rust-dev`, `anthropic/review-pr`.
  Avoids squatting, provides built-in trust signal.
- **Separate metadata from prompt**: `skill.toml` (registry metadata) +
  `SKILL.md` (Agent Skills spec-compatible prompt). The SKILL.md works
  standalone if dropped into `.claude/skills/`. The toml is what the
  registry indexes.
- **Strong categorization/tagging**: First-class categories, tags, and
  filtering. Critical for discoverability at scale.
- **Model compatibility**: Hybrid approach. Machine-readable capability
  flags (requires_tool_use, min_context_tokens) for agent filtering.
  Human-readable "verified with" badges for trust signals.
- **Start simple**: Local directory backend, no database, no web UI, no
  auth. Just an MCP server reading a directory tree.

## Architecture

The server reads a registry directory (currently a local path, eventually a
git checkout), loads all skills into an in-memory index, and serves them
over MCP via tools (search/browse) and resource templates (fetch content).

```
registry/              MCP Server              Agent
  owner/              +-------------+
    skill-name/  -->  | Load index  |  <---->  Tools: search, browse
      skill.toml      | Serve MCP   |  <---->  Resources: fetch content
      SKILL.md        +-------------+
      scripts/
      references/
      assets/
```

### Application State

```rust
struct AppState {
    index: RwLock<SkillIndex>,
    registry_path: PathBuf,
}

struct SkillIndex {
    skills: HashMap<(String, String), SkillEntry>,
    categories: BTreeMap<String, usize>,
}
```

The index is behind a `RwLock` to support future background refresh (git
pull on an interval).

### Project Structure

```
src/
  main.rs              # CLI args, tracing, router assembly, stdio transport
  state.rs             # AppState, SkillIndex, SkillEntry, SkillVersion, SkillFile
  index.rs             # Directory walking, skill.toml/SKILL.md parsing, file loading
  tools/
    mod.rs
    search_skills.rs   # Full-text search with category/tag/model filters
    list_categories.rs # Browse category taxonomy with counts
    list_skills_by_owner.rs
  resources/
    mod.rs
    skill_content.rs   # skillet://skills/{owner}/{name}[/{version}]
    skill_metadata.rs  # skillet://metadata/{owner}/{name}
    skill_files.rs     # skillet://files/{owner}/{name}/{path}
```

## Skillpack Format

A skillpack is the unit of distribution: everything needed for a skill.

```
owner/skill-name/
  skill.toml           # Registry metadata (required)
  SKILL.md             # Agent Skills spec-compatible prompt (required)
  scripts/             # Executable scripts referenced by the skill
  references/          # Reference docs (style guides, checklists, etc.)
  assets/              # Templates, configs, other static files
```

The extra directories follow the Agent Skills specification. Files in these
directories are served via the `skillet://files/` resource template and
listed in search results so agents know what's available.

### skill.toml

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
required_mcp_servers = []
verified_with = ["claude-opus-4-6", "claude-sonnet-4-6"]
```

**Capability flags vs model IDs**: The machine-readable fields
(`requires_tool_use`, `required_tools`, `min_context_tokens`) are what
agents filter on. They answer "can I run this skill?" The `verified_with`
field is a human trust signal ("someone tested this with my model"). It's a
badge, not a gate.

### SKILL.md

Minimal frontmatter for standalone compatibility, heavy metadata in toml:

```markdown
---
name: rust-dev
description: Rust development standards and conventions. Use when writing or reviewing Rust code.
---

## Rust Development Standards
...
```

If someone pulls just the SKILL.md and drops it in `.claude/skills/`, it
works standalone. The skill.toml is what the registry cares about.

### Versioning

Creator's choice. The registry stores version strings and enforces:
- Valid string (no whitespace, reasonable length)
- Append-only (can yank but not overwrite)

Convention options: semver (`1.2.0`), calver (`2026.02.24`), monotonic
(`3`). The `latest` version is always the most recently published
non-yanked version.

## MCP Interface

### Tools

| Tool | Parameters | Purpose |
|------|-----------|---------|
| `search_skills` | `query`, `category?`, `tag?`, `verified_with?` | Search skills by keyword with optional filters |
| `list_categories` | (none) | Browse category taxonomy with skill counts |
| `list_skills_by_owner` | `owner` | List all skills by a publisher |

### Resource Templates

| URI | Returns | Content-Type |
|-----|---------|-------------|
| `skillet://skills/{owner}/{name}` | SKILL.md (latest version) | text/markdown |
| `skillet://skills/{owner}/{name}/{version}` | SKILL.md (specific version) | text/markdown |
| `skillet://metadata/{owner}/{name}` | Full skill.toml | application/toml |
| `skillet://files/{owner}/{name}/{path}` | Skillpack file (scripts, references, assets) | varies |

### Agent Workflow

1. **Search** via tools to find relevant skills
2. **Fetch** SKILL.md via resource template
3. **Use** the skill -- three modes:

| Mode | What happens | Restart? |
|------|-------------|----------|
| **Inline** (default) | Agent reads the skill content and follows it for the current session | No |
| **Install** | Agent writes SKILL.md to `.claude/skills/` for persistent use | Yes |
| **Install and use** | Write to disk for persistence AND follow inline immediately | No (for this session) |

Inline use is the default and the key differentiator. The agent doesn't
need to install anything -- it fetches the skill and follows it in context.
This is what makes skillet a live skill library rather than just a package
manager.

### Setup Meta-Skill

The `skillet/setup` skill is a bootstrapping meta-skill that teaches agents
how to configure and use skillet. It covers MCP configuration, searching,
fetching, and the inline/install workflow.

## Trust Model

### Built-in (from git + PR gate)

- All changes auditable via git history
- PRs require review before merge
- Signed commits can be required
- Contributors are GitHub-authenticated

### Additional Layers (planned)

- **Namespace ownership**: once `owner/*` is claimed via first PR, only
  that GitHub user can publish to it. Enforced by CI.
- **Content hashing**: SHA256 of skill content for integrity verification.
- **Skill scanning**: automated PR checks for prompt injection patterns,
  exfiltration attempts, destructive commands.
- **Verified publishers**: badge (not gate) linking namespace to verified
  GitHub org.
- **Community flagging**: report skills via GitHub issue. Flagged skills
  get warning label pending review.

## Deployment

### Local Development

```bash
cargo run -- --registry test-registry
```

The `--registry` flag points to a local directory with the `owner/skill-name/`
structure. The `.mcp.json` at the project root configures this for local
testing.

### Production (planned)

Docker image that either mounts a local registry or clones from a remote
git URL:

```bash
# Local mount
docker run -i --rm -v /path/to/registry:/registry skillet --registry /registry

# Remote clone (planned)
docker run -i --rm -e SKILLET_REMOTE=https://github.com/skillet-registry/index.git skillet
```

### Public vs Self-Hosted

Two deployment models:

- **Public server**: Curated registry, better search (potentially
  Redis-backed), verified publishers, API key for zero-install access.
- **Self-hosted**: Run from Docker/binary against any registry (your org's
  private skills, a fork of the public registry, etc.). Full
  configurability.

## Tech Stack

- **MCP server**: Rust, tower-mcp 0.6
- **Index backend**: local directory (git checkout planned)
- **Serialization**: toml (skill metadata), serde_json (MCP protocol)
- **Distribution**: Docker image (planned)
- **CI**: GitHub Actions (planned)

## Roadmap

See [GitHub issues](https://github.com/joshrotenberg/grimoire/issues) for
detailed tracking. Key next steps:

- **Git backend**: Clone from remote, periodic refresh (#1)
- **Multi-version support**: Store and serve multiple versions per skill (#2)
- **Registry config.toml**: Support alternative/private registries (#3)
- **Content hashing**: SHA256 integrity verification (#4)
- **Search quality**: Move beyond substring matching (#5)
- **Publishing workflow**: `skillet publish` CLI (#6)

## Open Questions

- Skill parameters/template variables: deferred but planned. Skills could
  have `{edition}` style variables populated by the consumer at fetch time.
- Curation at scale: same unsolved problem as every package registry,
  compounded by no compiler to enforce quality.
- Categories taxonomy: who defines the canonical categories? Start with
  a small curated set, expand via PR.
- Naming: skillet is the current favorite. Domain and GitHub org TBD.

## Status

POC complete. Working MCP server with 10 skills across 4 owners. Search,
browse, and fetch all functional. Inline use model validated. Skillpack
file support implemented. Ready for git backend integration and
multi-version support.
