# Skillet Architecture Reference

## Data Flow

```
git checkout (or local dir)
    |
    v
load_config() --> RegistryConfig (from config.toml or defaults)
load_index()  --> SkillIndex (walks owner/skill-name dirs)
    |
    v
SkillSearch::build(&index) --> BM25 index over metadata fields
    |
    v
AppState::new(path, index, search, config) --> Arc<AppState>
    |
    +---> tools/resources serve requests via read locks
    +---> spawn_refresh_task pulls, compares HEAD, reloads both index + search
```

## Module Reference

### main.rs

Entry point. Parses CLI args with clap:

- `--registry <path>`: local registry directory
- `--remote <url>`: git URL to clone/pull
- `--refresh-interval <duration>`: pull interval (default "5m")
- `--cache-dir <path>`: where to clone remotes
- `--subdir <path>`: subdirectory within registry containing skills

Key functions:

- `main()`: parses args, resolves registry path, loads index, builds router, starts stdio transport
- `spawn_refresh_task()`: background tokio task that periodically pulls from remote, compares HEAD hash, reloads index + search if changed
- `parse_duration()`: parses "5m", "1h", "30s", "0" into `Duration`
- `cache_dir_for_url()`: derives cache path from remote URL

### state.rs

Shared application state and all data model types.

**AppState**: holds `RwLock<SkillIndex>`, `RwLock<SkillSearch>`, `registry_path`, and `config`. Constructor returns `Arc<Self>`.

**SkillIndex**: `HashMap<(owner, name), SkillEntry>` + `BTreeMap<category, count>`.

**SkillEntry**: owner, name, and `Vec<SkillVersion>`. The `latest()` method returns the last non-yanked version.

**SkillVersion**: version string, parsed `SkillMetadata`, raw `skill_md` and `skill_toml_raw` strings, extra `files` HashMap, `published` timestamp, `has_content` flag (false for historical versions), `content_hash`, and `integrity_ok`.

**SkillSummary**: serializable summary for search results. Built from `SkillEntry` via `SkillSummary::from_entry()`. Includes version count, available versions, integrity status.

**SkillMetadata / SkillInfo**: parsed skill.toml structure with classification (categories, tags) and compatibility (requires_tool_use, verified_with, etc.).

### index.rs

Registry loading from disk.

**load_config()**: reads `config.toml` from registry root. Returns defaults if absent, errors if malformed.

**load_index()**: walks the registry directory two levels deep (owner -> skill-name). Calls `load_skill()` for each, accumulates category counts. Warns and skips invalid skills.

**load_skill()**: reads `skill.toml` and `SKILL.md`, validates owner/name match directory structure, collects extra files from `scripts/`, `references/`, `assets/`. If `versions.toml` exists, builds multi-version entry; otherwise single-version.

**load_versions_manifest()**: parses `versions.toml`. Latest version (last entry) gets full content from disk. Historical versions are placeholders with `has_content = false`. Validates last version matches `skill.toml` version.

**verify_manifest()**: reads `MANIFEST.sha256`, compares computed hashes. Returns `(composite_hash, Option<bool>)`.

**load_extra_files()**: scans `scripts/`, `references/`, `assets/` subdirectories for text files. Determines mime type by extension.

### bm25.rs

Vendored BM25 search engine.

**Bm25Index**: inverted index with term frequencies and document metadata. Built from JSON documents via `Bm25Index::build()`. Search via `Bm25Index::search(query, top_k)` returns `Vec<SearchResult>` sorted by score.

**IndexOptions**: configures fields to index, ID field, stopwords, case normalization, BM25 k1/b parameters.

**stem_simple()**: plural stemmer handling -s, -ies, -xes, -zes, -sses, -shes suffixes. Does not handle verb forms (-ing, -ed).

### search.rs

**SkillSearch**: wraps `Bm25Index` for skill-specific search. Indexes fields: owner, name, description, trigger, categories, tags. Each skill is a JSON document with `id` = "owner/name".

**build()**: constructs from `SkillIndex`, returns `SkillSearch`.

**search()**: delegates to BM25, parses "owner/name" IDs back into tuples. Returns `Vec<(owner, name, score)>`.

### integrity.rs

Content hashing and verification.

**compute_hashes()**: SHA256 of `skill.toml`, `SKILL.md`, and extra files. Produces `ContentHashes` with per-file hashes (BTreeMap) and a composite hash (hash of sorted path+hash pairs).

**parse_manifest()**: reads `MANIFEST.sha256` format: `<hash>  <path>` lines, composite uses `*` as path. Comments (#) and blank lines ignored.

**verify()**: compares computed vs expected hashes. Returns list of mismatch descriptions. Checks: composite mismatch, per-file content mismatch, files in manifest but not on disk, files on disk but not in manifest.

**sha256_hex()**: returns `"sha256:<hex>"` string.

### git.rs

Git CLI wrappers.

- `clone()`: shallow clone (`--depth 1`) to target directory
- `pull()`: git pull in existing clone
- `head()`: `git rev-parse HEAD` to get current commit hash
- `clone_or_pull()`: clone if target doesn't exist, pull if it does

### tools/search_skills.rs

Full-text search tool. Input: query, optional category/tag/verified_with filters. Wildcard `*` returns all skills. BM25 results are looked up in the index to build `SkillSummary` objects. Structured filters applied post-search. Output is formatted markdown.

### tools/list_categories.rs

Returns all categories with skill counts from `SkillIndex.categories` (BTreeMap).

### tools/list_skills_by_owner.rs

Filters index by owner, returns all matching skills as summaries.

### resources/skill_content.rs

Two resource templates:
- `skillet://skills/{owner}/{name}` -- returns `SKILL.md` of latest version
- `skillet://skills/{owner}/{name}/{version}` -- returns `SKILL.md` of specific version (errors if `has_content` is false for historical versions)

### resources/skill_metadata.rs

`skillet://metadata/{owner}/{name}` -- returns raw `skill.toml` content of latest version.

### resources/skill_files.rs

`skillet://files/{owner}/{name}/{path}` -- returns content of extra files (scripts/, references/, assets/). Path is matched against `SkillVersion.files` HashMap keys.

## Refresh Cycle

When running with `--remote`:

1. `spawn_refresh_task` starts a tokio background loop
2. Every `refresh_interval`, it runs `git pull` in a blocking task
3. Compares HEAD before/after pull
4. If HEAD changed: calls `load_index()` and `SkillSearch::build()`, acquires write locks on both, replaces state
5. If HEAD unchanged: no-op (debug log)
6. Errors are logged as warnings; the current index is preserved

## How Tools Use State

All tools and resource handlers receive `Arc<AppState>` at build time. At request time they:

1. Acquire a read lock: `state.index.read().await` (or `state.search.read().await`)
2. Look up data in the index
3. Build response
4. Lock is dropped when the guard goes out of scope

This allows concurrent reads. Write locks are only held briefly during refresh.
