---
name: user
description: Using skillet as a skill consumer. Covers searching, installing, and managing skills from registries.
---

## Skillet User Guide

Skillet is an MCP-native skill registry toolkit. This skill covers using
skillet as a consumer: finding, installing, and managing skills.

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

### Using Skills

**Inline (recommended)**: search, read the skill content via the MCP resource,
and follow its instructions for the current session. No files written, no
restart needed.

```
1. search_skills("rust development")
2. Read skillet://skills/joshrotenberg/rust-dev
3. Follow the skill's instructions
```

**Install locally**: write the skill to disk for persistent use.

```bash
skillet install joshrotenberg/rust-dev --target claude
skillet install joshrotenberg/rust-dev --target agents
skillet install joshrotenberg/rust-dev --global
```

Target directories:
- `claude` -- `.claude/skills/<name>/`
- `agents` -- `.agents/skills/<name>/`
- `--global` -- `~/.claude/skills/<name>/` or `~/.agents/skills/<name>/`

### Managing Installed Skills

```bash
skillet list                    # show all installed skills
skillet audit                   # verify integrity of installed skills
skillet trust list              # show trusted registries and pinned skills
skillet trust pin owner/name    # pin a skill's content hash
```

### Configuration

Run `skillet setup` to generate `~/.config/skillet/config.toml`:

```bash
skillet setup                          # default setup
skillet setup --target claude          # set default install target
skillet setup --remote <url>           # add a custom registry
skillet setup --no-official-registry   # skip the official registry
```

The config file controls default install targets, registries, cache settings,
and trust configuration.

### User Preferences

If you prefer a specific workflow, tell your agent:
- "always inline" -- only use skills via MCP resources, never install
- "install it" -- write skills to disk for persistence
- If unclear, default to inline use and offer to install for future sessions
