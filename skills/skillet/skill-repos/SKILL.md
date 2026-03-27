---
name: skill-repos
description: Curated directory of external skill repositories. Use when the user needs skills not available locally, or to suggest adding new skill sources.
version: 2026.02.27
trigger: Use when the user needs skills skillet doesn't have locally, or when you want to suggest adding external skill sources
license: MIT OR Apache-2.0
author: Josh Rotenberg
categories:
  - tools
  - skills
tags:
  - skillet
  - skills
  - discovery
  - agent-skills
---

## External Skill Repositories

Skillet can aggregate skills from any git repo that contains `SKILL.md`
files. The repos below are included as default `[[suggest]]` entries --
skillet discovers them automatically on startup. No manual configuration
needed.

### Adding additional repos

If you need a repo that isn't in the default set:

```bash
# As a CLI flag (one-time)
skillet search react --remote https://github.com/org/skills.git --subdir skills

# In config (persistent)
```toml
# ~/.config/skillet/config.toml
[repos]
remote = ["https://github.com/org/skills.git"]
```

### When to suggest adding a repo

If the user asks about a topic and `search_skills` returns no results,
check this list for coverage gaps. The default suggest set covers most
common development domains.

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

**Microsoft -- Azure SDK and dev tools**
- Repo: `https://github.com/microsoft/skills.git`
- Subdir: `.github/plugins`
- Skills: 170+ (Azure SDK for Rust/Python/Java/TypeScript/.NET, dev tools)
- Use when: Azure services, Azure SDK, cloud development

**Kepano -- Obsidian and PKM**
- Repo: `https://github.com/kepano/obsidian-skills.git`
- Subdir: `skills`
- Skills: Obsidian note-taking, knowledge management
- Use when: Obsidian, personal knowledge management, note-taking workflows

### Community collections

Larger collections maintained by the community. Broader coverage, more
skills to discover.

**Softaworks Agent Toolkit -- broad dev workflows**
- Repo: `https://github.com/softaworks/agent-toolkit.git`
- Subdir: `skills`
- Skills: 43 (architecture, API design, testing, documentation, planning, refactoring, security, DevOps, and more)
- Use when: general development workflows, project planning, architecture decisions, documentation

**Anthony Fu -- curated dev skills**
- Repo: `https://github.com/antfu/skills.git`
- Subdir: `skills`
- Skills: curated development skills from a prolific open-source maintainer
- Use when: frontend tooling, ESLint, Vite, Vue, TypeScript patterns

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

All repos below are included as default `[[suggest]]` entries.

| Repo | Subdir | Domain |
|------|--------|--------|
| anthropics/skills | skills | docs, design, MCP, frontend |
| vercel-labs/agent-skills | skills | React, Next.js |
| firebase/agent-skills | skills | Firebase services |
| supabase/agent-skills | skills | Supabase, Postgres |
| google-gemini/gemini-skills | skills | Gemini API |
| redis/agent-skills | skills | Redis |
| callstackincubator/agent-skills | skills | React Native |
| microsoft/skills | .github/plugins | Azure SDK, dev tools |
| kepano/obsidian-skills | skills | Obsidian, PKM |
| TerminalSkills/skills | (root) | 800+ tools/frameworks |
| softaworks/agent-toolkit | skills | dev workflows |
| antfu/skills | skills | frontend tooling |
| daymade/claude-code-skills | (root) | Claude Code tools |
| muratcankoylan/Agent-Skills-... | skills | context engineering |
