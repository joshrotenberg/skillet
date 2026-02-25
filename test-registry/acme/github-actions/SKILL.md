---
name: github-actions
description: GitHub Actions CI/CD patterns and workflow templates. Use when creating or modifying GitHub Actions workflows, CI/CD pipelines, or automated releases.
---

## GitHub Actions

### Workflow Structure

- One workflow per concern (ci.yml, release.yml, deploy.yml)
- Use reusable workflows for shared logic
- Pin action versions to full SHA, not tags
- Use `concurrency` to cancel redundant runs

### CI Workflow Pattern

See `assets/ci-rust.yml` and `assets/ci-node.yml` for complete templates.

### Security

- Never use `pull_request_target` without careful review
- Use `permissions` to limit GITHUB_TOKEN scope
- Pin actions to SHA: `uses: actions/checkout@abc123`
- Use `environment` protection rules for deployments
- Store secrets in GitHub Secrets, never in workflow files

### Caching

```yaml
- uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry
      ~/.cargo/git
      target
    key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    restore-keys: ${{ runner.os }}-cargo-
```

### Matrix Builds

```yaml
strategy:
  fail-fast: false
  matrix:
    os: [ubuntu-latest, macos-latest]
    rust: [stable, nightly]
```
