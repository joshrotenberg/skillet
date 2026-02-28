---
name: setup
description: Set up and configure the Skillet skill registry. Use when the user wants to set up skillet, configure skill discovery, or manage skill installation preferences.
---

## Skillet Setup

Skillet is an MCP-native skill registry. It gives you access to a
searchable library of agent skills at runtime -- no installation required.

### Adding Skillet to Your Project

Add this to your `.mcp.json` (project-level) or `~/.claude/settings.json`
(global):

```json
{
  "mcpServers": {
    "skillet": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "ghcr.io/skillet/server:latest"],
      "env": {
        "SKILLET_REMOTE": "https://github.com/joshrotenberg/skillet.git"
      }
    }
  }
}
```

After adding, restart your agent to connect.

### Using Skills

Once connected, you have three ways to use skills from the registry:

**Inline (recommended for most cases)**:
Search for a skill, read it via the resource template, and follow its
instructions for the current session. No restart needed, no files written.

```
1. search_skills("rust development")
2. Read skillet://skills/joshrotenberg/rust-dev
3. Follow the skill's instructions
```

**Install locally**:
Write the skill to disk for persistent use across sessions.

- Project-level: `.claude/skills/<skill-name>.md`
- Global: `~/.claude/skills/<skill-name>.md`

Requires a restart to take effect.

**Install and use**:
Write the file for future sessions AND follow the instructions inline
for immediate use. Best of both worlds.

### User Preferences

If the user has a preference for how skills should be used, respect it:

- If they say "always inline" or "don't install anything", only use
  skills inline via the resource template.
- If they say "install it", write the SKILL.md to the appropriate
  skills directory.
- If unclear, default to inline use and ask if they want to install
  for future sessions.

### Discovering Skills

- `search_skills(query)` -- search by keyword, with optional category,
  tag, or model filters
- `list_categories()` -- browse available categories
- `list_skills_by_owner(owner)` -- see all skills by a publisher

When the user's task could benefit from a skill you don't have locally,
proactively search skillet. For example, if asked to write Python code
and you don't have Python conventions loaded, search for Python skills.
