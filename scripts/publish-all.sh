#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# =============================================================================
# Version resolution
# =============================================================================

CURRENT_VERSION=$(grep '^version = ' "$ROOT_DIR/Cargo.toml" | head -1 | sed 's/^version = "\(.*\)"/\1/')

if [ $# -eq 0 ]; then
  echo "Usage: $0 <version>"
  echo ""
  echo "  <version> can be an explicit semver like 0.2.0, or one of:"
  echo "    patch    bump the patch segment  (0.1.0 → 0.1.1)"
  echo "    minor    bump the minor segment  (0.1.0 → 0.2.0)"
  echo "    major    bump the major segment  (0.1.0 → 1.0.0)"
  echo ""
  echo "Current version: $CURRENT_VERSION"
  exit 1
fi

compute_next_version() {
  local version="$1"
  local segment="$2"
  local major minor patch
  IFS='.' read -r major minor patch <<< "$version"
  case "$segment" in
    patch) echo "$major.$minor.$((patch + 1))" ;;
    minor) echo "$major.$((minor + 1)).0" ;;
    major) echo "$((major + 1)).0.0" ;;
  esac
}

case "$1" in
  patch|minor|major)
    NEW_VERSION=$(compute_next_version "$CURRENT_VERSION" "$1")
    ;;
  *)
    NEW_VERSION="$1"
    if ! echo "$NEW_VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
      echo "Error: Version must be a semver (X.Y.Z) or one of: patch, minor, major"
      echo "  Got: '$NEW_VERSION'"
      exit 1
    fi
    ;;
esac

# =============================================================================
# Guards
# =============================================================================

if [ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]; then
  echo "Error: must be on main branch to publish (currently on '$(git rev-parse --abbrev-ref HEAD)')"
  exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "Error: working tree has uncommitted changes — commit or stash them first"
  exit 1
fi

# =============================================================================
# Version bump
# =============================================================================

echo "Bumping version: $CURRENT_VERSION → $NEW_VERSION"

sed -i '' "s/^version = \"[0-9]*\.[0-9]*\.[0-9]*\"/version = \"$NEW_VERSION\"/" "$ROOT_DIR/Cargo.toml"
sed -i '' "s/^version = \"[0-9]*\.[0-9]*\.[0-9]*\"/version = \"$NEW_VERSION\"/" "$ROOT_DIR/python/pyproject.toml"
sed -i '' "s/\"version\": \"[0-9]*\.[0-9]*\.[0-9]*\"/\"version\": \"$NEW_VERSION\"/" "$ROOT_DIR/typescript/package.json"

echo "✓ All versions updated to $NEW_VERSION"

# =============================================================================
# Tests
# =============================================================================

echo ""
echo "Running tests..."
"$ROOT_DIR/scripts/test-all.sh"

# =============================================================================
# Commit, tag, push
# =============================================================================

echo ""
echo "Committing and pushing version bump..."

git add -A
git commit -m "chore: bump version to $NEW_VERSION"
git tag -a "v$NEW_VERSION" -m "v$NEW_VERSION"
git push origin main --tags

echo ""
echo "✓ Pushed v$NEW_VERSION — GitHub Actions will publish to PyPI and npm"
