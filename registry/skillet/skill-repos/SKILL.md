---
name: skill-repos
description: Curated directory of external skill repositories. Use when the user needs skills not available locally, or to suggest adding new skill sources.
---

## External Skill Repositories

Skillet can aggregate skills from any git repo that contains `SKILL.md`
files. This is a curated list of high-quality external repositories that
work as skillet remotes. None of these require `skill.toml` -- skillet's
zero-config discovery handles them.

### How to add a repo

```bash
# As a CLI flag (one-time)
skillet search react --remote https://github.com/vercel-labs/agent-skills.git --subdir skills

# In MCP server config (persistent)
{
  "mcpServers": {
    "skillet": {
      "command": "skillet",
      "args": [
        "--remote", "https://github.com/vercel-labs/agent-skills.git",
        "--subdir", "skills"
      ]
    }
  }
}
```

Most repos below use `--subdir skills` because the convention is to put
skills under a `skills/` directory.

### When to suggest adding a repo

If the user asks about a topic and `search_skills` returns no results,
check this list. If there's a relevant repo, suggest adding it as a remote.
The user only needs to add it once -- after that, all skills from the repo
are searchable and installable.

---

### Official and vendor repos

These are maintained by the companies behind the tools. Small, focused,
authoritative.

**Anthropic -- Official Agent Skills reference**
- Repo: `https://github.com/anthropics/skills.git`
- Subdir: `skills`
- Skills: 16 (doc coauthoring, PDF/DOCX/PPTX/XLSX, MCP builder, design, frontend, web artifacts)
- Use when: document generation, slide decks, spreadsheets, MCP server development, frontend design

**Vercel -- React and Next.js**
- Repo: `https://github.com/vercel-labs/agent-skills.git`
- Subdir: `skills`
- Skills: 5 (React best practices, composition patterns, web design, React Native, deploy)
- Use when: React, Next.js, web frontend, component architecture

**Firebase -- Google Firebase services**
- Repo: `https://github.com/firebase/agent-skills.git`
- Subdir: `skills`
- Skills: 7 (Firestore, Auth, Hosting, App Hosting, Data Connect, AI Logic, basics)
- Use when: Firebase, Firestore, Firebase Auth, Firebase Hosting, Google Cloud

**Supabase -- Postgres and backend**
- Repo: `https://github.com/supabase/agent-skills.git`
- Subdir: `skills`
- Skills: 1 (comprehensive Supabase/Postgres best practices with 30+ reference docs)
- Use when: Supabase, Postgres, database design, backend APIs, edge functions

**Google Gemini -- Gemini API development**
- Repo: `https://github.com/google-gemini/gemini-skills.git`
- Subdir: `skills`
- Skills: 1 (Gemini API development best practices)
- Use when: Gemini API, Google AI SDK, LLM application development

**Redis -- Redis development**
- Repo: `https://github.com/redis/agent-skills.git`
- Subdir: `skills`
- Skills: 1 (Redis development best practices)
- Use when: Redis, caching, session management, pub/sub, data structures

**Callstack -- React Native**
- Repo: `https://github.com/callstackincubator/agent-skills.git`
- Subdir: `skills`
- Skills: 3 (React Native best practices, upgrading, GitHub workflows)
- Use when: React Native, mobile development, React Native upgrades

### Community collections

Larger collections maintained by the community. Broader coverage, more
skills to discover.

**Softaworks Agent Toolkit -- broad dev workflows**
- Repo: `https://github.com/softaworks/agent-toolkit.git`
- Subdir: `skills`
- Skills: 43 (architecture, API design, testing, documentation, planning, refactoring, security, DevOps, and more)
- Use when: general development workflows, project planning, architecture decisions, documentation

**Daymade Claude Code Skills -- Claude Code productivity**
- Repo: `https://github.com/daymade/claude-code-skills.git`
- Subdir: none (skills are at repo root)
- Skills: 38 (deep research, i18n, CLI tools, PDF creation, GitHub ops, Cloudflare, mermaid diagrams, and more)
- Use when: Claude Code power-user workflows, research, CLI tooling, cloud services

**Context Engineering Meta-Skills**
- Repo: `https://github.com/muratcankoylan/Agent-Skills-for-Context-Engineering.git`
- Subdir: `skills`
- Skills: 13 (context optimization, compression, memory systems, multi-agent patterns, tool design, evaluation)
- Use when: building agents, context engineering, multi-agent architectures, prompt optimization

---

### Quick reference

| Repo | --subdir | Skills | Domain |
|------|----------|--------|--------|
| anthropics/skills | skills | 16 | docs, design, MCP |
| vercel-labs/agent-skills | skills | 5 | React, Next.js |
| firebase/agent-skills | skills | 7 | Firebase services |
| supabase/agent-skills | skills | 1 | Supabase, Postgres |
| google-gemini/gemini-skills | skills | 1 | Gemini API |
| redis/agent-skills | skills | 1 | Redis |
| callstackincubator/agent-skills | skills | 3 | React Native |
| softaworks/agent-toolkit | skills | 43 | dev workflows |
| daymade/claude-code-skills | (none) | 38 | Claude Code tools |
| muratcankoylan/Agent-Skills-for-Context-Engineering | skills | 13 | context engineering |

**Total: 128 skills across 10 repos, all usable with `skillet --remote`.**
