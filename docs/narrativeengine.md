# NarrativeEngine

NarrativeEngine is a SDK platform for interactive enterinment. Domain models, validation, deterministic algorithms, schema generation, and service behavior are defined in Rust. Python and TypeScript SDKs are generated from the Rust definitions through PyO3 and NAPI-RS.

## Repository Tree

```text
.
├── Cargo.toml
├── crates
│   ├── narrativeengine-core
│   ├── narrativeengine-py
│   ├── narrativeengine-ts
│   ├── narrativeengine-codegen
│   ├── nap-core
│   ├── nap-cli
│   └── nap-server
├── generated
│   ├── csharp
│   ├── go
│   ├── java
│   ├── kotlin
│   └── swift
├── integration-tests
│   └── parity
├── python
├── scripts
├── typescript
└── .github
    └── workflows
```

## Installation

Rust:

```sh
cargo add narrativeengine-core
```

Python:

```sh
pip install narrativeengine
pip install "narrativeengine[pydantic]"
```

TypeScript:

```sh
npm install narrativeengine
```

## Usage

Rust:

```rust
use narrativeengine_core::{create_block, LabConfig, NarrativeLore};

let block = create_block("intro", "A signal appears in the archive.")?;
let lore = NarrativeLore {
    id: "lore-1".to_string(),
    title: "Archive Signal".to_string(),
    blocks: vec![block],
};
let candidate = narrativeengine_core::generate_candidate(&lore, &LabConfig::default())?;
# Ok::<(), narrativeengine_core::NarrativeError>(())
```

Python:

```python
from narrativeengine import LabConfig, NarrativeLore, create_block, generate_candidate

block = create_block("intro", "A signal appears in the archive.")
lore = NarrativeLore(id="lore-1", title="Archive Signal", blocks=[block])
candidate = generate_candidate(lore, LabConfig(temperature=0.7, max_candidates=4, seed=7))
```

TypeScript:

```ts
import { createBlock, generateCandidate } from "narrativeengine";

const block = createBlock("intro", "A signal appears in the archive.");
const candidate = generateCandidate(
  { id: "lore-1", title: "Archive Signal", blocks: [block] },
  { temperature: 0.7, max_candidates: 4, seed: 7 },
);
```

## Development

### Available scripts

| Script | What it does |
|--------|-------------|
| `scripts/generate-types.sh` | Generates type definitions for all SDK languages from Rust schemas |
| `scripts/build-rust.sh` | Lints, builds, and tests all Rust crates (`cargo fmt` → `clippy` → `build` → `test`) |
| `scripts/build-python.sh` | Builds Python bindings via `maturin develop`, runs tests, lints with ruff/mypy |
| `scripts/build-typescript.sh` | Installs npm deps, builds native `.node` addon via `napi build`, compiles TS with `tsc`, lints, runs vitest |
| `scripts/build-all.sh` | Runs all four above in order: generate → build-rust → build-python → build-typescript |
| `scripts/test-all.sh` | Runs `build-all.sh` then cross-runtime parity tests |
| `scripts/publish-all.sh` | Publishes all packages (cargo, maturin, npm) |

### Quick start

```sh
# Build + test everything in one command
./scripts/build-all.sh
```

### Layer by layer

```sh
# 1. Generate type definitions (optional if schemas haven't changed)
./scripts/generate-types.sh

# 2. Rust core + codegen (cargo build + test, excluding narrativeengine-py from tests)
cargo build --workspace && cargo test -p narrativeengine-core -p narrativeengine-codegen

# 3. Python bindings (maturin develop, pytest, lint)
./scripts/build-python.sh

# 4. TypeScript bindings (napi build, tsc, vitest, lint)
./scripts/build-typescript.sh
```

### After upgrading Rust or napi-build

Whenever the Rust toolchain or napi-build/pyo3 dependency versions change, **all native artifacts must be rebuilt**:

```sh
# 1. Wipe stale build artifacts
cargo clean

# 2. Point PyO3 at the project venv (not your system Python)
export PYO3_PYTHON="/absolute/path/to/.venv-python/bin/python3"

# 3. Rebuild everything
./scripts/build-all.sh
```

> **Why `PYO3_PYTHON` matters**: The `narrativeengine-py` crate uses PyO3 which probes `python3` on your `$PATH` at build time. If your system Python (e.g. miniforge3's 3.13) differs from the project venv (3.12), the test binary links against the wrong `libpython`. Setting `PYO3_PYTHON` to the venv's interpreter ensures correct linking.

> **Why `cargo test --workspace` fails on `narrativeengine-py`**: The `narrativeengine-py` crate is a `cdylib` that loads *into* a Python process. `cargo test` builds a standalone test **binary** that embeds Python, requiring `libpython.dylib` at runtime — but the rpath is not set up correctly outside of `maturin`. Always use `maturin develop` (via `build-python.sh`) to build and test the Python bindings. For CI, pass `--workspace --exclude narrativeengine-py` to `cargo test`.

### ES module + CJS native loader

The TypeScript package uses `"type": "module"` (ESM). The napi-rs generated loader (`index.js`) uses CommonJS (`require`/`module.exports`). The `build:native` script renames it to `index.cjs` so Node.js treats it as CJS even in an ESM package. The `src/native.ts` file loads it via `createRequire`:

```ts
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const native = require("../index.cjs") as NativeBindings;
```

## Build

Rust uses `cargo`, Python uses `maturin`, and TypeScript uses `@napi-rs/cli`. The Python and TypeScript packages are thin wrappers over native Rust bindings.

## Release

Publishing is handled by `.github/workflows/publish.yml` and `./scripts/publish-all.sh`. The publish script requires package registry credentials to be configured through environment variables.
