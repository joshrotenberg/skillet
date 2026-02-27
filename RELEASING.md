# Releasing

Releases are fully automated. Merge code to `main` and the pipeline
handles everything from versioning through distribution.

## Pipeline

```
Code merged to main
       |
       v
  release-plz          (runs on every push to main)
       |
       +-- Analyzes commits (conventional commit format)
       +-- Bumps version in Cargo.toml
       +-- Updates CHANGELOG.md (via git-cliff)
       +-- Opens a release PR
       |
       v
  Merge the release PR
       |
       +-- release-plz creates git tag (e.g. v0.1.1)
       +-- release-plz publishes to crates.io
       |
       v
  Tag push triggers two parallel workflows:
       |
       +-- cargo-dist (release.yml)
       |      +-- Builds binaries for 5 platforms
       |      |     aarch64-apple-darwin
       |      |     aarch64-unknown-linux-gnu
       |      |     x86_64-apple-darwin
       |      |     x86_64-unknown-linux-gnu
       |      |     x86_64-pc-windows-msvc
       |      +-- Creates shell + PowerShell installers
       |      +-- Creates GitHub Release with all artifacts
       |      +-- Pushes Homebrew formula to joshrotenberg/homebrew-brew
       |
       +-- Docker (docker.yml)
              +-- Builds linux/amd64 + linux/arm64 images
              +-- Pushes to ghcr.io with semver tags
                    (0.1.1, 0.1, 0, latest)
```

## What to do

1. **Write code.** Use conventional commits (`feat:`, `fix:`, `docs:`, etc.).
2. **Merge to main.** CI runs (fmt, clippy, test, MSRV, docs).
3. **Wait for the release PR.** release-plz opens it automatically.
4. **Review and merge the release PR.** This triggers everything.
5. **Done.** Crate, binaries, Homebrew, Docker all publish automatically.

## What NOT to do

- Do not create tags manually. release-plz owns tagging.
- Do not create GitHub Releases manually. cargo-dist owns that.
- Do not run `cargo publish` manually. release-plz does it.

## Secrets

| Secret | Purpose |
|--------|---------|
| `COMMITTER_TOKEN` | Used by release-plz to push tags and open PRs |
| `CARGO_REGISTRY_TOKEN` | Used by release-plz to publish to crates.io |
| `GITHUB_TOKEN` | Used by cargo-dist and Docker workflows (automatic) |

## Workflows

| File | Trigger | Purpose |
|------|---------|---------|
| `ci.yml` | Push/PR to main | fmt, clippy, test, MSRV, docs |
| `release-plz.yml` | Push to main | Version bump PR + crates.io publish |
| `release.yml` | Tag push (`*[0-9]+.[0-9]+.[0-9]+*`) | Binary builds + GitHub Release + Homebrew |
| `docker.yml` | Tag push (`v[0-9]+.[0-9]+.[0-9]+*`) | Docker images to GHCR |

## Configuration

| File | Purpose |
|------|---------|
| `release-plz.toml` | release-plz behavior (publish, tagging, changelog) |
| `cliff.toml` | git-cliff changelog generation rules |
| `dist-workspace.toml` | cargo-dist targets, installers, Homebrew tap |
