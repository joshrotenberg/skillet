---
name: rust-dev
description: Rust development standards and conventions. Use when writing or reviewing Rust code.
---

## Rust Development Standards

### Pre-commit Checklist

Run these checks before every commit:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib --all-features
cargo test --test '*' --all-features
```

### Conventions

- Target the latest stable Rust edition
- Use `thiserror` for library errors, `anyhow` for application errors
- All public APIs must have doc comments
- Run `cargo fmt` before committing
- Prefer `impl Trait` over `dyn Trait` where possible
- Use `#[must_use]` on functions that return values that should not be ignored

### Testing

- Unit tests go in `#[cfg(test)] mod tests` within the source file
- Integration tests go in `tests/`
- Use `#[test]` for synchronous tests, `#[tokio::test]` for async
- Prefer `assert_eq!` and `assert_ne!` over `assert!` for better error messages

### Dependencies

- Audit new dependencies before adding them
- Prefer well-maintained crates with recent releases
- Pin major versions in Cargo.toml (`"1"` not `"1.2.3"`)
- Run `cargo deny check` if configured
