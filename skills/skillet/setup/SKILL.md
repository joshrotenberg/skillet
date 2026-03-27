---
name: setup
description: Set up and configure Skillet skill discovery. Use when the user wants to set up skillet, configure repos, or customize server behavior.
version: 2026.02.27
trigger: Use when the user wants to set up skillet, configure skill discovery, or manage skill installation preferences
license: MIT OR Apache-2.0
author: Josh Rotenberg
categories:
  - tools
  - configuration
tags:
  - skillet
  - skills
  - setup
  - mcp
---

## Skillet Setup

Skillet is an MCP-native skill discovery tool. It gives you access to a
searchable library of agent skills at runtime, served as MCP prompts.

### Adding Skillet to Your Project

Add this to your `.mcp.json` (project-level) or `~/.claude/settings.json`
(global):

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

Or with Docker (no install needed):

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

After adding, restart your agent to connect.

### Using Skills

Once connected, skills are available as MCP prompts. Search for a skill,
then use it as a prompt for the current session. No files written, no
restart needed.

```
1. search_skills("rust development")
2. Use the skill prompt for joshrotenberg/rust-dev
3. Follow the skill's instructions
```

### Discovering Skills

- `search_skills(query)` -- search by keyword, with optional category,
  tag, or model filters
- `list_categories()` -- browse available categories
- `list_skills_by_owner(owner)` -- see all skills by a publisher

When the user's task could benefit from a skill you don't have locally,
proactively search skillet. For example, if asked to write Python code
and you don't have Python conventions loaded, search for Python skills.

If search returns no results for the user's topic, check the
`skillet/skill-repos` skill for external repositories that may cover it.
Use `info_skill("skillet/skill-repos")` to see the curated list.

### Configuration

Skillet is configured via `~/.config/skillet/config.toml` and CLI flags.

**Adding custom repos**:

```bash
skillet serve --remote https://github.com/org/skills.git
```

**Skipping the official repo**:

```bash
skillet serve --no-official-repo
```

**Config file** (`~/.config/skillet/config.toml`):

```toml
[repos]
remote = ["https://github.com/org/skills.git"]

[server]
discover_local = false
```

### User Preferences

If the user has a preference for how skills should be used, respect it:

- If they say "always inline" or "don't install anything", only use
  skills inline via MCP prompts.
- If unclear, default to inline use via prompts.
