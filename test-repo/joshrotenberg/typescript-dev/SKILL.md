---
name: typescript-dev
description: TypeScript development standards with strict mode and modern patterns. Use when writing or reviewing TypeScript code.
---

## TypeScript Development Standards

### Tooling

- **Runtime**: Node.js 22+ or Bun
- **Linter**: ESLint with `@typescript-eslint`
- **Formatter**: Prettier or Biome
- **Test runner**: Vitest
- **Build**: `tsc` for type checking, bundler for output

### Strict Mode

Always use strict TypeScript. See `references/TSCONFIG.md` for the
recommended tsconfig.json.

- Enable `strict: true` (includes noImplicitAny, strictNullChecks, etc.)
- Enable `noUncheckedIndexedAccess`
- Enable `exactOptionalPropertyTypes`
- Never use `any` -- use `unknown` and narrow

### Conventions

- Use `type` for object shapes, `interface` only when extending is needed
- Prefer `const` assertions for literal types
- Use discriminated unions over optional fields
- Use `satisfies` operator for type-safe object literals
- Prefer `Map` and `Set` over plain objects for dynamic keys
- Use template literal types for string patterns

### Error Handling

- Use `Result<T, E>` pattern (see `scripts/result.ts`)
- Never throw in library code -- return errors as values
- Use `Error` subclasses with `cause` for error chains
- Type-narrow errors in catch blocks

### Pre-commit

```bash
tsc --noEmit
eslint .
prettier --check .
vitest run
```
