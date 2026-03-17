# Skillet

A skill discovery toolkit for AI agents. CLI and MCP server.

Repo name is `grimoire`, crate/binary name is `skillet`.

## Build and test

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib --all-features
cargo test --bin skillet --all-features
cargo doc --no-deps --all-features
```

Rust 2024 edition, minimum supported version 1.90.

## Architecture

Single crate, library + binary. `src/lib.rs` re-exports modules,
`src/main.rs` has the CLI (clap) and MCP server setup.

Key modules:
- `state.rs` -- `AppState`, `SkillIndex`, `SkillEntry`, `SkillVersion`, data models
- `index.rs` -- directory walking, `skill.toml`/`SKILL.md` parsing
- `search.rs` -- BM25 full-text search over skill metadata and content
- `bm25.rs` -- vendored BM25 engine
- `cache.rs` -- persistent disk cache for `SkillIndex`
- `config.rs` -- `SkilletConfig`, configuration loading
- `error.rs` -- error types (`thiserror`)
- `git.rs` -- git operations: clone, pull, head
- `prompts.rs` -- `DynamicPromptRegistry` integration, skills served as MCP prompts
- `project.rs` -- `skillet.toml` unified manifest types and loading
- `repo.rs` -- multi-repo loading, clone/pull, cache coordination
- `resolve.rs` -- release model resolution (tags/releases/main)
- `scaffold.rs` -- init-skill, init-registry, init-project scaffolding
- `suggest.rs` -- `[[suggest]]` decentralized discovery graph walker
- `tools/` -- MCP tools: `search_skills`, `list_categories`, `list_skills_by_owner`, `info_skill`

Key patterns:
- `AppState` holds `RwLock<SkillIndex>` and `RwLock<SkillSearch>`, both rebuilt on refresh
- `SkillIndex` maps `(owner, name)` to `SkillEntry`, with `merge()` for multi-repo (first match wins)
- MCP tools and prompts are capability-gated via `ServerCapabilities`
- Skills are served as MCP prompts via `DynamicPromptRegistry`
- All MCP tools follow the same pattern: `build(state) -> Tool` using `ToolBuilder` from `tower-mcp`

## Code style

- `anyhow` for application errors, `thiserror` for library errors (`src/error.rs`)
- Public APIs should have doc comments
- No emojis in code, commits, or documentation
- Conventional commits: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`
- Prefer editing existing files over creating new ones
- Don't over-engineer: only add what's needed for the current task

## Testing

- Unit tests live in each module as `#[cfg(test)] mod tests`
- Binary integration tests in `src/main.rs` use `tower-mcp`'s test harness
- Test fixtures are dynamically generated via `testutil.rs`
- `tempfile` crate for temporary directories in tests
- No mocking framework; tests use real data structures

## Skill format

```
owner/skill-name/
  skill.toml       # Metadata for indexing (optional, inferred if absent)
  SKILL.md         # Agent prompt content (required)
  scripts/         # Optional executable scripts
  references/      # Optional reference docs
  assets/          # Optional templates, configs
```

## CLI subcommands

- `skillet search <query>` -- search repos
- `skillet info <owner/name>` -- show skill detail
- `skillet init-skill <path>` -- scaffold a skillpack
- `skillet init-registry <path>` -- scaffold a repo
- `skillet init-project [path]` -- generate a `skillet.toml` project manifest
- `skillet [serve]` -- run MCP server (default subcommand)
