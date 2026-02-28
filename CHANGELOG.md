# Changelog

All notable changes to this project will be documented in this file.

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


