---
name: python-dev
description: Python development standards with type hints and modern tooling. Use when writing or reviewing Python code.
---

## Python Development Standards

### Tooling

- **Formatter**: `ruff format`
- **Linter**: `ruff check --fix`
- **Type checker**: `mypy --strict`
- **Tests**: `pytest`

### Conventions

- Use type hints on all function signatures
- Use `pathlib.Path` instead of string paths
- Prefer `dataclasses` or `pydantic` over plain dicts for structured data
- Use `logging` module, not `print()` for diagnostic output
- Follow PEP 8 naming: `snake_case` for functions/variables, `PascalCase` for classes

### Pre-commit Checklist

```bash
ruff format --check .
ruff check .
mypy .
pytest
```

### Testing

- Use `pytest` with fixtures
- One test file per module: `test_module.py`
- Use `pytest.raises` for exception testing
- Use `tmp_path` fixture for filesystem tests
