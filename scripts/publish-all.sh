#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CURRENT_VERSION=$(grep '^version = ' "$ROOT_DIR/Cargo.toml" | head -1 | sed 's/^version = "\(.*\)"/\1/')
WORKSPACE_PACKAGES=(
  "nap-cli"
  "nap-core"
  "nap-server"
  "narrativeengine-codegen"
  "narrativeengine-core"
  "narrativeengine-py"
  "narrativeengine-ts"
)

usage() {
  cat <<EOF
Usage: $0 <version>

  <version> can be an explicit semver like 0.2.0, or one of:
    patch    bump the patch segment  (${CURRENT_VERSION} → next patch)
    minor    bump the minor segment  (${CURRENT_VERSION} → next minor)
    major    bump the major segment  (${CURRENT_VERSION} → next major)

This script will:
  1. Verify you're on a clean main branch
  2. Bump all release versions (Cargo, Cargo.lock, Python, TypeScript)
  3. Run pre-publish validation
  4. Commit the release
  5. Create an annotated tag (vX.Y.Z)
  6. Push main and tags to origin

After the push, GitHub Actions will start the publish workflow.
If the 'production' environment requires approval, approve it in GitHub.
EOF
}

compute_next_version() {
  local version="$1"
  local segment="$2"
  local major minor patch
  IFS='.' read -r major minor patch <<< "$version"
  case "$segment" in
    patch) echo "$major.$minor.$((patch + 1))" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    major) echo "$((major + 1)).0.0" ;;
    *)
      echo "Internal error: unsupported segment '$segment'" >&2
      exit 1
      ;;
  esac
}

if [ $# -ne 1 ]; then
  usage
  exit 1
fi

case "$1" in
  patch|minor|major)
    NEW_VERSION=$(compute_next_version "$CURRENT_VERSION" "$1")
    ;;
  *)
    NEW_VERSION="$1"
    if ! echo "$NEW_VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
      echo "Error: version must be X.Y.Z or one of: patch, minor, major" >&2
      echo "  Got: '$NEW_VERSION'" >&2
      exit 1
    fi
    ;;
esac

RELEASE_TAG="v$NEW_VERSION"

if [ "$NEW_VERSION" = "$CURRENT_VERSION" ]; then
  echo "Error: new version matches current version ($CURRENT_VERSION)" >&2
  exit 1
fi

if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
  echo "Error: must be on main branch to release (currently on '$(git rev-parse --abbrev-ref HEAD)')" >&2
  exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "Error: working tree has uncommitted changes — commit or stash them first" >&2
  exit 1
fi

if git rev-parse "$RELEASE_TAG" >/dev/null 2>&1; then
  echo "Error: tag $RELEASE_TAG already exists locally" >&2
  exit 1
fi

if git ls-remote --tags origin "refs/tags/$RELEASE_TAG" | grep -q "$RELEASE_TAG"; then
  echo "Error: tag $RELEASE_TAG already exists on origin" >&2
  exit 1
fi

if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
  echo "Error: local main is not aligned with origin/main — pull/rebase first" >&2
  exit 1
fi

echo "Bumping release version: $CURRENT_VERSION → $NEW_VERSION"

python3 <<'PY' "$ROOT_DIR" "$CURRENT_VERSION" "$NEW_VERSION"
from pathlib import Path
import sys

root = Path(sys.argv[1])
current = sys.argv[2]
new = sys.argv[3]
workspace_packages = [
    "nap-cli",
    "nap-core",
    "nap-server",
    "narrativeengine-codegen",
    "narrativeengine-core",
    "narrativeengine-py",
    "narrativeengine-ts",
]

replacements = {
    root / "Cargo.toml": [(f'version = "{current}"', f'version = "{new}"')],
    root / "python/pyproject.toml": [(f'version = "{current}"', f'version = "{new}"')],
    root / "typescript/package.json": [(f'  "version": "{current}",', f'  "version": "{new}",')],
}

for path, ops in replacements.items():
    text = path.read_text()
    for old, replacement in ops:
        if old not in text:
            raise SystemExit(f"Expected text not found in {path}: {old}")
        text = text.replace(old, replacement, 1)
    path.write_text(text)

cargo_lock = root / "Cargo.lock"
text = cargo_lock.read_text()
for package in workspace_packages:
    old = f'name = "{package}"\nversion = "{current}"'
    new_text = f'name = "{package}"\nversion = "{new}"'
    if old not in text:
        raise SystemExit(f"Expected package/version pair not found in Cargo.lock: {package} {current}")
    text = text.replace(old, new_text, 1)
cargo_lock.write_text(text)
PY

echo "✓ Versions updated"

echo ""
echo "Running pre-publish validation..."

npm --prefix typescript ci
env GITHUB_REF_NAME="$RELEASE_TAG" node scripts/pre-publish-check.mjs
npm --prefix typescript run build:types

echo "✓ Release validation passed"

echo ""
echo "Committing and tagging $RELEASE_TAG..."

git add Cargo.toml Cargo.lock python/pyproject.toml typescript/package.json
git commit -m "chore(release): cut $RELEASE_TAG"
git tag -a "$RELEASE_TAG" -m "$RELEASE_TAG"

echo ""
echo "Pushing main and tags..."

git push origin main --tags

echo ""
echo "✓ Pushed $RELEASE_TAG"
echo "→ GitHub Actions publish workflow should now be running"
echo "→ If the 'production' environment requires approval, approve it in GitHub"
