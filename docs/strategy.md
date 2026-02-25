# Skillet Strategy Notes

Notes from early design sessions. Not public-facing.

## Market Timing

The Agent Skills specification landed Dec 2025. It's been adopted by
Claude Code, OpenAI Codex, Cursor, Gemini CLI, VS Code Copilot, and
others. The format is standardized. Distribution is not.

We're early enough to define the category. Not so early that no one
cares about the problem. The existing projects are all hobby-stage with
no clear winner.

npm is the de facto distribution mechanism right now, but it's a hack.
Skills aren't JavaScript packages. They don't have dependencies, build
steps, or runtime code. Stuffing a markdown file into an npm package to
get versioning and distribution is a solution born of "it's the only
thing we have."

## Competitive Landscape (Feb 2026)

| Project | What it is | Stage | Differentiator |
|---------|-----------|-------|----------------|
| awesome-claude-skills | GitHub list | Curated links | Not a registry |
| skills-npm (antfu) | npm discovery | Finds SKILL.md in node_modules | Tied to Node ecosystem |
| SkillUse | CLI + GitHub backend | Early | Closest to skillet, no MCP |
| SkillDock | Versioned registry | Early, unclear traction | Web-based |
| SkillsMP (skillsmp.com) | Web crawler/directory | 270k+ skills indexed | No MCP, no versioning, manual install |
| Claude Plugins | Auto-crawler | Indexes 63k+ from GitHub | No curation, auto-discovery |
| **Skillet** | **MCP-native registry** | **POC complete** | **Agent discovers skills at runtime via MCP** |

## Competitive Moat

The MCP-native angle is the differentiator. Every other registry requires
a CLI, a package manager, or manual file copying. Skillet is the only
approach where the agent discovers and retrieves skills through the
protocol it already speaks. No installation step, no ecosystem lock-in.

SkillsMP validates the demand (270k+ skills indexed) but solves discovery
with a web UI, not an agent-native interface. Their catalog is broad but
uncurated and requires manual install. Skillet's inline-use model means
the agent fetches and follows skills at runtime with zero friction.

## Launch Strategy

### Phase 1: Curated MVP
- Build the MCP server (tower-mcp, Rust)
- Seed with 10-20 high-quality skills covering common workflows
- Verified publishers only
- Invite a handful of known-good skill authors

### Phase 2: Controlled Opening
- Open to community submissions via PR
- Namespace ownership enforced by CI
- Automated skill scanning on PR
- Quality norms established by the seed set

### Phase 3: Scale (if warranted)
- Migrate from git-only to sparse HTTP index
- Split content storage to CDN if needed
- Consider private registry hosting (enterprise)

## Business Considerations

### Where the money could be

The public registry itself is hard to monetize directly. crates.io and
Homebrew are non-profits. npm monetized through private registries
(enterprise), not the public one.

Potential revenue:
- **Hosted private registries**: "skillet for your org." Companies want
  internal skill libraries (onboarding, code standards, security policies)
  without publishing publicly. That's a SaaS play.
- **Verification/trust services**: skill scanning, security auditing,
  compliance badges. Enterprises will pay for "these skills are safe to
  deploy to our agents."
- **Platform leverage**: if skillet becomes the place agents go for skills,
  that's significant distribution leverage.

### Paths forward

**Path A: Open source, build in public**
- Ship on personal GitHub, write a blog post, get adoption
- Fastest to community traction
- Anyone can fork, no moat beyond network effects

**Path B: Stealth until MVP, then launch**
- Build quietly, seed with good skills, launch with a polished demo
- First impression matters, control the narrative
- Risk of someone else shipping first (low based on landscape)

**Path C: Corporate backing**
- Pitch internally as infrastructure the company could own
- Resources, credibility, brand recognition
- Lose some control, community may be skeptical

**Recommended approach**: Path B first. Build the MVP independently. Get
it working, seed it with skills, have a few trusted people try it. Then
decide between A and C with real data.

### Immediate actions
- Register domain (skillet.dev, skillet.sh, or similar)
- Register GitHub org (skillet-registry or similar)
- Keep the repo private until MVP is functional
- Do not share publicly until there's something to demo
