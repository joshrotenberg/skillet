# Changelog

All notable changes to this project will be documented in this file.

## [0.5.1] - 2026-03-18

### Features

- Skill annotations and GitHub repo discovery ([#214](https://github.com/joshrotenberg/skillet/pull/214))

### Miscellaneous Tasks

- Remove [source] prefer = main workaround ([#213](https://github.com/joshrotenberg/skillet/pull/213))



## [0.5.0] - 2026-03-17

### Bug Fixes

- Show quickstart help when running skillet interactively ([#174](https://github.com/joshrotenberg/skillet/pull/174))
- Scope `skillet list` to current project and global by default ([#176](https://github.com/joshrotenberg/skillet/pull/176))
- Soften trust warning on install ([#177](https://github.com/joshrotenberg/skillet/pull/177))
- Implicit serve when --repo/--remote/--http flags are provided ([#201](https://github.com/joshrotenberg/skillet/pull/201))
- Add [source] prefer = main to prevent tag checkout on self ([#209](https://github.com/joshrotenberg/skillet/pull/209))

### Documentation

- Quote glob in README search examples ([#178](https://github.com/joshrotenberg/skillet/pull/178))
- Update all docs and skills for prompt server architecture ([#200](https://github.com/joshrotenberg/skillet/pull/200))

### Features

- Hint at skill-repos on empty search results ([#179](https://github.com/joshrotenberg/skillet/pull/179))
- Rewrite as skill network + MCP prompt server ([#196](https://github.com/joshrotenberg/skillet/pull/196))
- Harden suggest graph with safety limits and trust tiers ([#197](https://github.com/joshrotenberg/skillet/pull/197))
- Release model resolution for skill repos ([#198](https://github.com/joshrotenberg/skillet/pull/198))
- Cache tiering, provenance polish, and public instance groundwork ([#199](https://github.com/joshrotenberg/skillet/pull/199))
- Prompt arguments for section filtering, clean up dead suggest URLs ([#207](https://github.com/joshrotenberg/skillet/pull/207))
- Auto-detect skills/ directory, expand suggest list ([#208](https://github.com/joshrotenberg/skillet/pull/208))

### Refactor

- Rename registry/ to skills/ ([#181](https://github.com/joshrotenberg/skillet/pull/181))
- Replace static test fixtures with dynamic generation ([#183](https://github.com/joshrotenberg/skillet/pull/183))
- Remove registry terminology, add [[suggest]] for discovery ([#184](https://github.com/joshrotenberg/skillet/pull/184))
- Remove dead integrity/content_hash fields and stale URIs ([#202](https://github.com/joshrotenberg/skillet/pull/202))



## [0.4.0] - 2026-03-02

### Bug Fixes

- Scope uninstall to current project directory ([#173](https://github.com/joshrotenberg/skillet/pull/173))

### Refactor

- Design v2 -- remove publishing, simplify trust, rename registry to repo ([#171](https://github.com/joshrotenberg/skillet/pull/171))



## [0.3.0] - 2026-02-28

### Bug Fixes

- Load official registry catalog even with custom remotes ([#167](https://github.com/joshrotenberg/skillet/pull/167))
- Index external repos with flat directory structures ([#169](https://github.com/joshrotenberg/skillet/pull/169))

### Features

- Support nested skill directories in registries ([#126](https://github.com/joshrotenberg/skillet/pull/126)) ([#127](https://github.com/joshrotenberg/skillet/pull/127))
- Implement skillet.toml unified project manifest ([#148](https://github.com/joshrotenberg/skillet/pull/148))
- Support npm-style skill repos as registries ([#152](https://github.com/joshrotenberg/skillet/pull/152))
- Move official registry into main repo ([#159](https://github.com/joshrotenberg/skillet/pull/159))
- Add skill-repos skill with curated external skill sources ([#160](https://github.com/joshrotenberg/skillet/pull/160))
- Add repo catalog and resource templates ([#166](https://github.com/joshrotenberg/skillet/pull/166))

### Miscellaneous Tasks

- Add MIT and Apache-2.0 license files ([#124](https://github.com/joshrotenberg/skillet/pull/124))
- Remove legacy config.toml registry support ([#161](https://github.com/joshrotenberg/skillet/pull/161))

### Refactor

- Remove repo catalog, move curation to SKILL.md ([#170](https://github.com/joshrotenberg/skillet/pull/170))

### Testing

- Add unit tests for state, git, and publish modules ([#153](https://github.com/joshrotenberg/skillet/pull/153))
- Expand coverage for install, pack, and registry modules ([#154](https://github.com/joshrotenberg/skillet/pull/154))
- Add integration tests for CLI hygiene, publish, and audit/trust ([#155](https://github.com/joshrotenberg/skillet/pull/155))
- Add scenario tests for publish, project manifest, config, and error recovery ([#156](https://github.com/joshrotenberg/skillet/pull/156))
- Add MCP integration tests for tools, resources, and local discovery ([#157](https://github.com/joshrotenberg/skillet/pull/157))
- Add HTTP transport integration tests ([#158](https://github.com/joshrotenberg/skillet/pull/158))



## [0.2.0] - 2026-02-27

### Bug Fixes

- Add --version flag and fix commit signing in git commands (#114, #118) ([#119](https://github.com/joshrotenberg/skillet/pull/119))

### Documentation

- Overhaul README for public launch ([#89](https://github.com/joshrotenberg/skillet/pull/89)) ([#101](https://github.com/joshrotenberg/skillet/pull/101))
- Add install options and lead with MCP quick start ([#102](https://github.com/joshrotenberg/skillet/pull/102))
- Add Docker MCP config example to quick start ([#109](https://github.com/joshrotenberg/skillet/pull/109))
- Add RELEASING.md and document HTTP transport security (#111, #116) ([#121](https://github.com/joshrotenberg/skillet/pull/121))

### Features

- Add `skillet setup` command ([#64](https://github.com/joshrotenberg/skillet/pull/64)) ([#100](https://github.com/joshrotenberg/skillet/pull/100))
- Add list_installed, audit_skills, setup_config, validate_skill MCP tools ([#108](https://github.com/joshrotenberg/skillet/pull/108))
- Add --require-trusted flag and improve trust warnings (#112, #117) ([#120](https://github.com/joshrotenberg/skillet/pull/120))

### Refactor

- Consolidate error handling across library modules ([#115](https://github.com/joshrotenberg/skillet/pull/115)) ([#123](https://github.com/joshrotenberg/skillet/pull/123))
- Split main.rs into cli/ module tree ([#113](https://github.com/joshrotenberg/skillet/pull/113)) ([#122](https://github.com/joshrotenberg/skillet/pull/122))


