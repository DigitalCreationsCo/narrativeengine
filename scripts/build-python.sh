#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_DIR="${VENV_DIR:-$ROOT_DIR/.venv-python}"

PYTHON_VERSION=$(cat "$ROOT_DIR/.python-version")

if [ ! -d "$VENV_DIR" ]; then
  uv venv "$VENV_DIR" --python "$PYTHON_VERSION"
fi

PYTHON_BIN="$VENV_DIR/bin/python"
echo "Using Python at: $PYTHON_BIN ($("$PYTHON_BIN" --version))"

# Activate the venv so all child processes find the right Python
export VIRTUAL_ENV="$VENV_DIR"
export PATH="$VENV_DIR/bin:$PATH"
export PYO3_PYTHON="$PYTHON_BIN"

# Build narrativeengine Python bindings
cd "$ROOT_DIR/python/narrativeengine"

# Force a clean Rust rebuild so maturin doesn't use a stale cached .so
touch -m src/lib.rs

# --python targets the right venv for pip; --active makes uv run use VIRTUAL_ENV
uv pip install --python "$PYTHON_BIN" -e ".[dev]"
uv run --active maturin develop --extras pydantic
uv run --active pytest
uv run --active ruff check --fix .
uv run --active mypy

# Build nap-sdk Python bindings
cd "$ROOT_DIR/python/nap-sdk"
uv pip install --python "$PYTHON_BIN" -e ".[dev]"
uv run --active maturin develop