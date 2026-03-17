---
name: repo-maintainer
description: Creating and maintaining skill repos. Covers setup, directory layout, PR review, multi-repo, and hosting.
---

## Repo Maintainer Guide

A skill repo is a git repo with a flat directory structure of skillpacks.
Anyone can create one. Skillet is the tool, repos are data.

### Creating a Repo

```bash
skillet init-registry path/to/my-repo
skillet init-registry path/to/my-repo --name "My Repo" --description "Team skills"
```

This creates a git repo with:
- `skillet.toml` -- repo metadata
- `README.md` -- instructions for contributors
- `.gitignore`
- An initial commit

### Directory Layout

```
my-repo/
  skillet.toml          # repo config
  owner1/skill-a/       # flat: owner/name
    skill.toml
    SKILL.md
  owner1/skill-b/
    skill.toml
    SKILL.md
  owner2/tool/
    skill.toml
    SKILL.md
```

Nested paths are also supported (e.g. `acme/lang/java/maven-build/`).
The skill's `owner` and `name` fields in `skill.toml` are authoritative;
the directory path is for organization.

### Repo Configuration

**skillet.toml** (preferred):

```toml
[registry]
name = "my-repo"
version = 1
description = "Team skills for our org"

[registry.maintainer]
name = "Jane Doe"
github = "janedoe"
email = "jane@example.com"

[[suggest]]
url = "https://github.com/joshrotenberg/skillet.git"
description = "Official community skills"

[registry.defaults]
refresh_interval = "10m"
```

### Accepting PRs

When contributors submit skills, review:

1. **skill.toml**: valid fields, appropriate categories/tags, version format
2. **SKILL.md**: clear instructions, no dangerous patterns
3. **Extra files**: scripts should be safe, references should be relevant

### npm-style Repository Bridging

Skillet can read skills from npm-style repos where skills live under a
`skills/` subdirectory. Use `--subdir skills` when serving:

```bash
skillet --repo path/to/npm-repo --subdir skills
```

### Multi-Repo

Users can aggregate multiple repos. First-match-wins on name collision:

```bash
skillet search "*" --repo ./primary --repo ./secondary
skillet --repo ./primary --remote https://github.com/org/skills.git
```

### Hosting

Skill repos are git repos. Host anywhere git is accessible:
- GitHub / GitLab / Bitbucket (public or private)
- Self-hosted git servers
- Any URL that `git clone` can reach

No special server infrastructure needed. Skillet clones the repo and
indexes it locally.
