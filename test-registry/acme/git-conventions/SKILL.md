---
name: git-conventions
description: Git workflow conventions with conventional commits and branch naming. Use when making git commits, creating branches, or managing PRs.
---

## Git Conventions

### Branch Naming

- `feat/` -- new features
- `fix/` -- bug fixes
- `docs/` -- documentation changes
- `refactor/` -- code refactoring
- `test/` -- test improvements
- `chore/` -- maintenance tasks

Always create a feature branch before making changes. Never commit directly
to main.

### Conventional Commits

Format: `type(scope): description`

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
- `feat(auth): add OAuth2 login flow`
- `fix(api): handle null response from upstream`
- `docs(readme): update installation instructions`

Breaking changes: use `!` after type -- `feat!: remove legacy API`

### PR Conventions

- Keep PRs focused on a single concern
- Reference related issues in the description
- Include a test plan
- Request review from relevant code owners
