---
name: skillet-dev
description: Skillet codebase conventions, architecture, and contribution workflow. Use when working on the skillet codebase.
---

## What is Skillet

Skillet is an MCP-native skill registry for AI agents. It serves skills (structured prompts with metadata) from a git-backed registry via MCP tools and resource templates. Think crates.io or Homebrew, but for agent skills.

Three-layer architecture:

| Layer | Implementation | Purpose |
|-------|---------------|---------|
| Discovery index | Git repo, flat `owner/skill-name` dirs | Find skills, filter, resolve versions |
| Content storage | Same git repo (skills are small) | Store skill.toml + SKILL.md packages |
| MCP server | Rust, tower-mcp | Search, browse, fetch via tools + resource templates |

## Module Map

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI args (clap), server startup, MCP router assembly, background refresh task |
| `src/state.rs` | `AppState` (shared state), all data types: `SkillIndex`, `SkillEntry`, `SkillVersion`, `SkillSummary`, `SkillMetadata` |
| `src/index.rs` | Registry loader -- walks `owner/skill-name/` dirs, parses skill.toml + SKILL.md, loads extra files, handles `versions.toml` multi-version support |
| `src/bm25.rs` | Vendored BM25 search engine (from jpx-engine), simple plural stemmer, no serde |
| `src/search.rs` | `SkillSearch` wrapper -- indexes skill metadata fields into BM25, rebuilt on refresh |
| `src/integrity.rs` | Content hashing (SHA256) and `MANIFEST.sha256` verification |
| `src/git.rs` | Git CLI operations: clone, pull, HEAD inspection for remote registries |
| `src/tools/` | MCP tools: `search_skills`, `list_categories`, `list_skills_by_owner` |
| `src/resources/` | MCP resource templates: `skill_content`, `skill_metadata`, `skill_files` |

## Key Patterns

### AppState with RwLock

`AppState` holds `RwLock<SkillIndex>` and `RwLock<SkillSearch>`. Both are rebuilt together when the remote registry is refreshed. Tools and resources acquire read locks; the refresh task acquires write locks.

### tower-mcp Tool Pattern

Tools use `ToolBuilder` with the extractor pattern:

```rust
pub fn build(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("tool_name")
        .description("...")
        .read_only()
        .idempotent()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<InputType>| async move {
                let index = state.index.read().await;
                // ... use index ...
                Ok(CallToolResult::text(output))
            },
        )
        .build()
}
```

Input types derive `Deserialize` and `JsonSchema`. The `State` extractor provides access to `AppState`. Return `CallToolResult::text(...)`.

### Resource Template Pattern

Resource templates use `ResourceTemplateBuilder` with a URI template and closure handler:

```rust
pub fn build(state: Arc<AppState>) -> ResourceTemplate {
    ResourceTemplateBuilder::new("skillet://skills/{owner}/{name}")
        .name("Skill Content")
        .description("...")
        .mime_type("text/markdown")
        .handler(move |uri: String, vars: HashMap<String, String>| {
            let state = state.clone();
            async move {
                let owner = vars.get("owner").cloned().unwrap_or_default();
                // ... look up skill, return content ...
                Ok(ReadResourceResult { contents: vec![...], meta: None })
            }
        })
}
```

URI variables are extracted from the `vars` HashMap.

## Adding a Tool

1. Create `src/tools/my_tool.rs` with a `build(state: Arc<AppState>) -> Tool` function
2. Add `pub mod my_tool;` to `src/tools/mod.rs`
3. In `main.rs`: call `let my_tool = tools::my_tool::build(state.clone());` and add `.tool(my_tool)` to the router

## Adding a Resource Template

1. Create `src/resources/my_resource.rs` with a `build(state: Arc<AppState>) -> ResourceTemplate` function
2. Add `pub mod my_resource;` to `src/resources/mod.rs`
3. In `main.rs`: call `let my_resource = resources::my_resource::build(state.clone());` and add `.resource_template(my_resource)` to the router

## Test Registry

`test-registry/` is the development registry. The server defaults to this directory when no `--registry` or `--remote` flag is provided. Adding skills there provides integration test coverage -- the existing `test_load_index_from_test_registry` test validates all skills in the directory parse correctly.

Skill directory layout: `test-registry/owner/skill-name/` containing `skill.toml` and `SKILL.md`. Optional subdirectories: `scripts/`, `references/`, `assets/`.

## Search

BM25 index over skill metadata fields (owner, name, description, trigger, categories, tags). Rebuilt on each registry refresh. Structured filters (category, tag, verified_with) are applied post-search. Wildcard query `*` returns all skills with filters only.

The simple stemmer handles plurals but not verb forms -- "testing" and "test" are different tokens.

## Pre-commit Checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib --all-features
cargo test --test '*' --all-features
```

## Git Workflow

- Create feature branches from main: `feat/`, `fix/`, `docs/`, `refactor/`
- Use conventional commits: `feat: add X`, `fix: resolve Y`
- Do not merge PRs -- the maintainer handles merges
- After a PR is merged: checkout main and pull before starting new work

For deeper module-level reference, fetch `skillet://files/joshrotenberg/skillet-dev/references/ARCHITECTURE.md`.
