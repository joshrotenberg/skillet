---
name: contributor
description: Contributing to skillet itself. Covers architecture, development setup, testing, and PR workflow.
---

## Contributing to Skillet

### Architecture

Skillet is a single Rust binary (`skillet`) that serves as both CLI and
MCP server. Package name is `skillet-mcp`, binary name is `skillet`.

**Module map**:

```
src/
  main.rs        # CLI (clap), MCP server setup
  lib.rs         # Library root, re-exports
  state.rs       # AppState, SkillIndex, SkillEntry, SkillVersion, SkillMetadata
  index.rs       # Directory walking, skill.toml/SKILL.md parsing, config loading
  search.rs      # SkillSearch: BM25 full-text search over skill metadata
  bm25.rs        # Vendored BM25 engine
  cache.rs       # Persistent disk cache for SkillIndex
  config.rs      # Configuration loading (SkilletConfig)
  error.rs       # Error types (thiserror)
  git.rs         # Git operations: clone, pull, head
  prompts.rs     # DynamicPromptRegistry integration, skills as MCP prompts
  project.rs     # skillet.toml unified manifest types and loading
  repo.rs        # Repo management: init, load, merge
  resolve.rs     # Release model resolution (tags/releases/main)
  scaffold.rs    # init-skill, init-registry, init-project scaffolding
  suggest.rs     # [[suggest]] decentralized discovery graph walker
  tools/         # MCP tools: search_skills, list_categories, etc.
```

**Key types**:

- `AppState` -- holds `RwLock<SkillIndex>`, `RwLock<SkillSearch>`, repo paths, config
- `SkillIndex` -- maps `(owner, name)` to `SkillEntry` with `merge()` for multi-repo
- `SkillSearch` -- wraps BM25 index, rebuilt on refresh
- `SkillSource` -- variants: `Registry`, `Embedded`

**Data flow**: repos (local/remote) are loaded into `SkillIndex`, merged
first-match-wins, then `SkillSearch` is built from the merged index.
MCP tools query the search index; skills are served as MCP prompts.

### Development Setup

```bash
git clone https://github.com/joshrotenberg/skillet.git
cd skillet
cargo build
cargo test --lib --all-features
```

### Running Tests

The test suite is organized across multiple files:

```bash
# Unit tests (in src/)
cargo test --lib --all-features

# CLI integration tests
cargo test --test cli --all-features

# Scenario tests (multi-step workflows)
cargo test --test scenarios --all-features

# MCP integration tests
cargo test --test mcp --all-features

# HTTP transport tests
cargo test --test http --all-features

# All tests
cargo test --test '*' --all-features
```

Test fixtures are dynamically generated via `testutil.rs`.

### Pre-commit Checklist

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib --all-features
cargo test --test '*' --all-features
```

### PR Workflow

1. Create a feature branch: `git checkout -b feat/description`
2. Make changes, run the pre-commit checklist
3. Commit with conventional format: `feat: add thing`, `fix: resolve issue`
4. Push and open a PR against `main`
5. Ensure CI passes (fmt, clippy, all tests)

Branch naming conventions:
- `feat/` -- new features
- `fix/` -- bug fixes
- `refactor/` -- code refactoring
- `test/` -- test improvements
- `docs/` -- documentation

### Design Principles

- **Tool-first**: skillet is the tool, repos are data
- **Zero-config-first**: only SKILL.md is truly required
- **Owner/name namespacing**: `owner/skill-name` directories
- **Separate metadata from prompt**: skill.toml + SKILL.md
- **Git-backed**: repos are git repos, auditable by default
