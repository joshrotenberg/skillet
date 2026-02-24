# Skillet Design Document

A skill registry for AI agents, distributed over MCP.

Working name: skillet. Previous name: grimoire. Other candidates: folio,
pantry, quiver. The logo is a cartoon cast iron skillet cooking up some
markdown.

## Problem Statement

The Agent Skills specification (Dec 2025) standardized the skill format
across Claude Code, Cursor, Copilot, Gemini CLI, and others. Distribution
is the unsolved problem. Skills are scattered across GitHub repos, npm
packages, and copy-paste. There is no Homebrew or crates.io for agent
skills.

Current distribution methods and their limitations:

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

Installation is one JSON block:

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
- **Transport-agnostic index**: Same file format whether served from git
  clone, sparse HTTP, or API. Start with git, migrate transport later
  without schema changes.
- **Start simple**: Git repo backend, no database, no web UI, no auth.
  Just an MCP server reading a git checkout.

## Architecture

### Three-Layer Model

Following the crates.io / Homebrew pattern:

| Layer | Implementation | Purpose |
|-------|---------------|---------|
| Discovery index | Git repo, flat `owner/skill-name` dirs | Find skills, filter, resolve versions |
| Content storage | Same git repo (skills are tiny) | Store skill.toml + SKILL.md packages |
| MCP server | tower-mcp Rust server | Search, browse, fetch via tools + resource templates |

Content storage can split out to object storage + CDN later if needed.
Skills are a few KB each so a single git repo scales far before that's
necessary.

### Precedent: crates.io Architecture

The crates.io model that informed this design:

| Layer | crates.io | Skillet equivalent |
|-------|-----------|-------------------|
| Discovery index | Git repo (crates.io-index) with prefix-sharded JSON-lines, now mostly served via sparse HTTP | Git repo with flat owner/name metadata files |
| Content storage | S3 behind CloudFront | Same git repo initially, migrate to object storage if needed |
| Management API | Rust web server + PostgreSQL | MCP server (skillet itself) |

Key insight from both crates.io and Homebrew: the index format must be
transport-agnostic from day one. Same files whether served from git clone,
sparse HTTP, or an API. The migration from git to sparse+CDN is a transport
change, not a schema change.

### Alternative Registries

Like cargo's model: a `config.toml` at the repo root points to content
URLs. For internal/private registries, stand up your own git repo with the
same format and point skillet at it. The protocol is identical; only the
URL differs.

## Skill Package Format

```
owner/skill-name/
  skill.toml       # registry metadata
  SKILL.md         # Agent Skills spec-compatible prompt template
  README.md        # optional human-readable docs
```

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
# Machine-readable gates (agent uses these to filter)
requires_tool_use = true
requires_vision = false
min_context_tokens = 4096
required_tools = ["bash", "read", "write", "edit"]
required_mcp_servers = []

# Human-readable trust signal (not a gate)
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
non-yanked version. Index entries carry a `published` timestamp for
date-based sorting regardless of versioning scheme.

### Future: Skill Parameters

Deferred but planned. Skills could have template variables (`{edition}`)
populated by the consumer at fetch time. The `[skill.parameters]` table in
skill.toml would declare available parameters with types, defaults, and
descriptions. Not in v1.

## Index Format

Flat directory structure: `owner/skill-name` files containing JSON-lines,
one line per published version.

```
# File: joshrotenberg/rust-dev
{"owner":"joshrotenberg","name":"rust-dev","vers":"2026.02.24","cksum":"sha256:abc123","categories":["development","rust"],"tags":["rust","cargo"],"requires_tool_use":true,"verified_with":["claude-opus-4-6"],"published":"2026-02-24T12:00:00Z","yanked":false}
```

No prefix-sharding initially. Flat `owner/skill-name` is simple and maps
directly to resource template URIs. Add sharding if/when the index grows
to warrant it.

### config.toml (Registry Root)

```toml
[registry]
name = "skillet"
version = 1

[registry.urls]
download = "https://skills.example.com/packages/{owner}/{name}/{version}.tar.gz"
api = "https://skills.example.com/api/v1"

[registry.auth]
required = false
```

For pure git-backed registries, `download` is optional (content lives in
the same repo). Having the field from day one supports future splits.

## MCP Interface

### Tools (discovery/search)

- `search_skills(query, categories?, tags?, verified_with?)` -- full-text
  search over index entries
- `list_categories()` -- browse the category taxonomy
- `list_skills_by_owner(owner)` -- everything by one publisher

### Resource Templates (direct access)

- `skillet://skills/{owner}/{name}` -- returns SKILL.md content (latest)
- `skillet://skills/{owner}/{name}/{version}` -- specific version
- `skillet://metadata/{owner}/{name}` -- returns full skill.toml

### Tools (management, future)

- `publish_skill` -- submit a new version
- `yank_version` -- mark a version as yanked

Agent workflow: search via tools, fetch via resource template, use inline.
No installation step.

## Trust Model

### Built-in (from git + PR gate)

- All changes auditable via git history
- PRs require review before merge
- Signed commits can be required
- Contributors are GitHub-authenticated

### Additional Layers

- **Namespace ownership**: once `owner/*` is claimed via first PR, only
  that GitHub user can publish to it. Enforced by CI.
- **Content hashing**: index stores SHA256 of skill content. Consumers
  verify integrity.
- **Skill scanning**: automated PR checks for prompt injection patterns,
  exfiltration attempts, destructive commands. Raises the floor.
- **Verified publishers**: badge (not gate) linking namespace to verified
  GitHub org.
- **Community flagging**: report skills via MCP tool or GitHub issue.
  Flagged skills get warning label pending review.

### Launch Strategy: Curated First

Start with verified publishers only. Seed with 10-20 high-quality skills
covering common workflows, then invite known-good skill authors. Open the
gates once quality norms are established.

Rationale: 41.7% of skills in the wild contain serious security
vulnerabilities. 50 high-quality skills that all work is more valuable than
5,000 untested ones. Homebrew, crates.io, and npm all started small and
curated. You can always open up later; you can never un-open.

## Tech Stack

- **MCP server**: Rust, tower-mcp
- **Index backend**: local git checkout, refreshed on interval or webhook
- **Distribution**: Docker image
- **CI**: GitHub Actions for PR validation, skill scanning, index integrity

## Server Architecture

### Application State

```rust
struct AppState {
    index: RwLock<SkillIndex>,
    repo_path: PathBuf,
    content_path: PathBuf,
}

struct SkillIndex {
    skills: HashMap<(String, String), Vec<IndexEntry>>,  // (owner, name) -> versions
    categories: BTreeSet<String>,
}
```

### Project Structure

```
src/
  main.rs              # CLI args, transport setup, router assembly
  state.rs             # AppState, SkillIndex, IndexEntry
  index.rs             # Git repo loading, index parsing
  tools/
    mod.rs
    search_skills.rs
    list_categories.rs
    list_skills_by_owner.rs
  resources/
    mod.rs
    skill_content.rs   # SKILL.md resource templates
    skill_metadata.rs  # skill.toml resource template
```

### Configuration

```
skillet-server --repo /path/to/index        # local git checkout
skillet-server --remote https://github.com/skillet-registry/index.git
```

The `--remote` variant clones on startup and refreshes on an interval.
For Docker, the image either mounts a local checkout or clones from a
configured remote.

## Competitive Landscape (Feb 2026)

| Project | What it is | Stage | Differentiator |
|---------|-----------|-------|----------------|
| awesome-claude-skills | GitHub list | Curated links | Not a registry |
| skills-npm (antfu) | npm discovery | Finds SKILL.md in node_modules | Tied to Node ecosystem |
| SkillUse | CLI + GitHub backend | Early | Closest to skillet, no MCP |
| SkillDock | Versioned registry | Early, unclear traction | Web-based |
| Claude Plugins | Auto-crawler | Indexes 63k+ from GitHub | No curation, auto-discovery |
| **Skillet** | **MCP-native registry** | **Design phase** | **Agent discovers skills at runtime via MCP** |

Nobody has built the MCP-native registry. Every other approach requires a
CLI, a package manager, or manual file copying. Skillet is the only one
where the agent discovers and fetches skills at runtime through the
protocol it already speaks.

## Open Questions

- Skill parameters/template variables: deferred but planned.
- Curation at scale: same unsolved problem as every package registry,
  compounded by no compiler to enforce quality.
- How consuming agents use fetched skills at runtime: inline expansion
  into context? Dynamic skill registration? Depends on agent capabilities.
- Categories taxonomy: who defines the canonical categories? Start with
  a small curated set, expand via PR.
- Business model: public registry is hard to monetize directly (crates.io
  and Homebrew are non-profits). Potential in hosted private registries
  for enterprise, verification/trust services, or platform leverage.
- Naming: skillet is the current favorite. Domain and GitHub org TBD.

## Status

Design phase. Schema and architecture defined. Next: scaffold the
tower-mcp server.
