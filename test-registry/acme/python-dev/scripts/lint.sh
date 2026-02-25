#!/usr/bin/env bash
# Run all Python linting checks
set -euo pipefail

echo "Running ruff format check..."
ruff format --check .

echo "Running ruff linter..."
ruff check .

echo "Running mypy..."
mypy .

echo "All checks passed."
