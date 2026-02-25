# Ruff Configuration Reference

## pyproject.toml

```toml
[tool.ruff]
target-version = "py312"
line-length = 88

[tool.ruff.lint]
select = [
    "E",   # pycodestyle errors
    "W",   # pycodestyle warnings
    "F",   # pyflakes
    "I",   # isort
    "UP",  # pyupgrade
    "B",   # flake8-bugbear
    "SIM", # flake8-simplify
    "TCH", # flake8-type-checking
]
```

## Common Rules

- **E501**: Line too long (disabled by default when using formatter)
- **F401**: Unused import
- **I001**: Import block not sorted
- **UP006**: Use `type` instead of `Type` for type hints (Python 3.9+)
- **B006**: Mutable default argument
