# NarrativeEngine Python SDK

The Python SDK is a thin PyO3 wrapper over the canonical Rust implementation. All domain models are generated from Rust schemas.

## Installation

```sh
pip install narrativeengine
pip install "narrativeengine[pydantic]"
```

## Usage

```python
from narrativeengine import LabConfig, NarrativeLore, create_block, generate_candidate

block = create_block("intro", "A signal appears in the archive.")
lore = NarrativeLore(id="lore-1", title="Archive Signal", blocks=[block])
candidate = generate_candidate(lore, LabConfig(temperature=0.7, max_candidates=4, seed=7))
```

## Development

```sh
python -m pip install -e ".[dev]"
maturin develop --manifest-path ../crates/narrativeengine-py/Cargo.toml --extras pydantic
python -m pytest
ruff check .
mypy narrativeengine
```

## Build

```sh
maturin build --manifest-path ../crates/narrativeengine-py/Cargo.toml --release
```

## Release

Wheels are built through GitHub Actions and published by `scripts/publish-all.sh`.
