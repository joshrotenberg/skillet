---
name: user
description: Using skillet as a skill consumer. Covers searching, browsing, and using skills as MCP prompts.
version: 2026.02.27
trigger: Use when the user wants to search for skills, install skills, configure skillet, or manage their skill library
license: MIT OR Apache-2.0
author: Josh Rotenberg
categories:
  - tools
  - configuration
tags:
  - skillet
  - skills
  - mcp
  - install
  - search
---

## Skillet User Guide

Skillet is an MCP-native skill discovery toolkit. This skill covers using
skillet as a consumer: finding and using skills at runtime via MCP prompts.

### Adding Skillet

**MCP server (recommended)** -- add to `.mcp.json` or `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "skillet": {
      "command": "skillet",
      "args": ["serve"]
    }
  }
}
```

**Install the binary**:

```bash
cargo install skillet-mcp
```

**Docker** (no install needed):

```json
{
  "mcpServers": {
    "skillet": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "ghcr.io/joshrotenberg/skillet:latest"]
    }
  }
}
```

### Searching for Skills

**CLI**:

```bash
skillet search "rust development"
skillet search "*" --category development
skillet search "*" --tag pytest
skillet search "*" --owner joshrotenberg
```

**MCP tools** (when running as server):

- `search_skills(query)` -- full-text BM25 search over skill metadata and content
- `list_categories()` -- browse available categories with counts
- `list_skills_by_owner(owner)` -- list all skills by a publisher
- `info_skill(owner_name)` -- detailed information about a specific skill

### Using Skills

Skills are served as MCP prompts. An agent connects to skillet, searches
for a relevant skill, and uses it as a prompt for the current session.
No files written, no restart needed.

```
1. search_skills("rust development")
2. Use the skill prompt for joshrotenberg/rust-dev
3. Follow the skill's instructions
```

### Configuration

Skillet is configured via `~/.config/skillet/config.toml` and CLI flags.

**Adding custom repos**:

```bash
skillet serve --remote https://github.com/org/skills.git
```

**Config file** (`~/.config/skillet/config.toml`):

```toml
[repos]
remote = ["https://github.com/org/skills.git"]

[server]
discover_local = false
```

To skip the official repo, use `--no-official-repo`.

### User Preferences

If you prefer a specific workflow, tell your agent:
- "always inline" -- only use skills via MCP prompts, never write files
- If unclear, default to inline use via prompts
