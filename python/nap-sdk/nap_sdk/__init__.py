from __future__ import annotations

import json
import os
from typing import Any, cast

from . import _native

# ── Helpers ──────────────────────────────────────────────────────────

_DEFAULT_NAP_DIR = os.environ.get("NAP_DIR") or os.path.expanduser("~/.nap")


def _resolve_repo_path(repo_path: str | None) -> str:
    if repo_path is None:
        return _DEFAULT_NAP_DIR
    return os.path.expanduser(repo_path)


# ═══════════════════════════════════════════════════════════════════════
# URI Operations
# ═══════════════════════════════════════════════════════════════════════


def parse_uri(uri: str) -> dict[str, Any]:
    """Parse a ``nap://`` URI into its component parts.

    Args:
        uri: A NAP URI, e.g. ``"nap://starwars/character/lukeskywalker"``.

    Returns:
        A dict with keys: ``universe``, ``entity_type``, ``entity_id``,
        and optionally ``fragment``.
    """
    return cast(dict[str, Any], json.loads(_native.parse_uri(uri)))


def uri_new(
    universe: str,
    entity_type: str,
    entity_id: str,
    fragment: str | None = None,
) -> dict[str, Any]:
    """Construct a new NAP URI from components.

    Args:
        universe: Universe name (e.g. ``"starwars"``).
        entity_type: Entity type (e.g. ``"character"``, ``"location"``).
        entity_id: Entity ID slug (e.g. ``"lukeskywalker"``).
        fragment: Optional query fragment (e.g. ``"properties.species"``).

    Returns:
        A dict with the parsed URI components.
    """
    return cast(dict[str, Any], json.loads(_native.uri_new(universe, entity_type, entity_id, fragment)))


def uri_identity(uri: str) -> str:
    """Return the canonical identity URI (without fragment).

    Args:
        uri: A NAP URI, possibly with a fragment.

    Returns:
        The identity URI as a string, e.g. ``"nap://starwars/character/lukeskywalker"``.
    """
    return _native.uri_identity(uri)


def uri_manifest_path(uri: str) -> str:
    """Return the relative filesystem path for an entity's manifest.

    Args:
        uri: A NAP URI.

    Returns:
        A relative path like ``"characters/lukeskywalker.yaml"``.
    """
    return _native.uri_manifest_path(uri)


def uri_format(
    universe: str,
    entity_type: str,
    entity_id: str,
    fragment: str | None = None,
) -> str:
    """Format URI components into a ``nap://`` URI string.

    Args:
        universe: Universe name.
        entity_type: Entity type.
        entity_id: Entity ID.
        fragment: Optional query fragment.

    Returns:
        A full NAP URI string.
    """
    return _native.uri_format(universe, entity_type, entity_id, fragment)


# ═══════════════════════════════════════════════════════════════════════
# EntityType Operations
# ═══════════════════════════════════════════════════════════════════════


def entity_type_parse(s: str) -> str:
    """Parse an entity type string.

    Args:
        s: Type string, e.g. ``"character"``, ``"location"``, ``"scene"``,
            ``"prop"``, ``"world"``.  Also accepts plural forms.

    Returns:
        The normalized entity type string.
    """
    return json.loads(_native.entity_type_parse(s))


def entity_type_directory_name(entity_type: str) -> str:
    """Return the directory name used for this entity type in a repository.

    Args:
        entity_type: Type string, e.g. ``"character"``.

    Returns:
        Directory name, e.g. ``"characters"``.
    """
    return _native.entity_type_directory_name(entity_type)


def entity_type_list() -> list[str]:
    """Return all subdirectory entity types (character, location, scene, prop).

    Returns:
        List of entity type strings excluding ``"world"``.
    """
    return cast(list[str], json.loads(_native.entity_type_list()))


# ═══════════════════════════════════════════════════════════════════════
# Manifest Operations
# ═══════════════════════════════════════════════════════════════════════


def parse_manifest(yaml_str: str) -> dict[str, Any]:
    """Parse a YAML manifest string into a JSON-serializable dict.

    Args:
        yaml_str: A YAML string representing a NAP manifest.

    Returns:
        The parsed manifest as a dict.
    """
    return cast(dict[str, Any], json.loads(_native.parse_manifest(yaml_str)))


def manifest_new(
    universe: str,
    entity_type: str,
    entity_id: str,
    name: str,
) -> dict[str, Any]:
    """Create a new manifest with minimal required fields.

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        entity_id: Entity ID slug.
        name: Human-readable name.

    Returns:
        The new manifest as a dict.
    """
    return cast(dict[str, Any], json.loads(_native.manifest_new(universe, entity_type, entity_id, name)))


def manifest_to_yaml(manifest: dict[str, Any]) -> str:
    """Serialize a manifest dict to YAML.

    Args:
        manifest: A manifest dict.

    Returns:
        YAML string representation.
    """
    return _native.manifest_to_yaml(json.dumps(manifest))


def manifest_from_yaml(yaml_str: str) -> dict[str, Any]:
    """Read a manifest from a YAML string.

    Args:
        yaml_str: YAML string.

    Returns:
        Parsed manifest as a dict.
    """
    return cast(dict[str, Any], json.loads(_native.manifest_from_yaml(yaml_str)))


def manifest_content_hash(manifest: dict[str, Any]) -> str:
    """Compute the SHA-256 content hash of a manifest.

    Args:
        manifest: A manifest dict.

    Returns:
        Content hash string in ``sha256:<hex>`` format.
    """
    return _native.manifest_content_hash(json.dumps(manifest))


def manifest_set_property(manifest: dict[str, Any], key: str, value: str) -> dict[str, Any]:
    """Add or update a property on a manifest.

    Args:
        manifest: A manifest dict.
        key: Property key.
        value: Property value as a string (will be parsed as YAML).

    Returns:
        The updated manifest dict.
    """
    return cast(dict[str, Any], json.loads(_native.manifest_set_property(json.dumps(manifest), key, value)))


def manifest_add_reference(manifest: dict[str, Any], key: str, value: str) -> dict[str, Any]:
    """Add a cross-reference to a manifest.

    Args:
        manifest: A manifest dict.
        key: Reference key (e.g. ``"appears_in"``).
        value: Reference value as a string (parsed as YAML).

    Returns:
        The updated manifest dict.
    """
    return cast(dict[str, Any], json.loads(_native.manifest_add_reference(json.dumps(manifest), key, value)))


def manifest_set_representation(
    manifest: dict[str, Any],
    key: str,
    hash: str,
    format: str,
    uri: str | None = None,
    tier: str | None = None,
) -> dict[str, Any]:
    """Add or update a representation on a manifest.

    Args:
        manifest: A manifest dict.
        key: Representation key (e.g. ``"reference_image"``).
        hash: Content hash string.
        format: File format (e.g. ``"png"``, ``"glb"``).
        uri: Optional storage URI.
        tier: Optional quality tier.

    Returns:
        The updated manifest dict.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.manifest_set_representation(json.dumps(manifest), key, hash, format, uri, tier)
        ),
    )


def manifest_bump_version(manifest: dict[str, Any]) -> dict[str, Any]:
    """Increment the version counter on a manifest.

    Args:
        manifest: A manifest dict.

    Returns:
        The updated manifest dict with version incremented.
    """
    return cast(dict[str, Any], json.loads(_native.manifest_bump_version(json.dumps(manifest))))


# ═══════════════════════════════════════════════════════════════════════
# ContentHash Operations
# ═══════════════════════════════════════════════════════════════════════


def content_hash_from_bytes(data: bytes) -> str:
    """Compute the SHA-256 content hash of raw bytes.

    Args:
        data: Raw byte data.

    Returns:
        Content hash string ``sha256:<hex>``.
    """
    return _native.content_hash_from_bytes(data)


def content_hash_from_string(s: str) -> str:
    """Compute the SHA-256 content hash of a string.

    Args:
        s: Input string.

    Returns:
        Content hash string ``sha256:<hex>``.
    """
    return _native.content_hash_from_string(s)


def content_hash_parse(s: str) -> str:
    """Parse and validate a ``sha256:<hex>`` content hash string.

    Args:
        s: Content hash string.

    Returns:
        The validated content hash string.

    Raises:
        ValueError: If the string is not a valid content hash.
    """
    return _native.content_hash_parse(s)


def content_hash_verify(hash: str, data: bytes) -> bool:
    """Verify that bytes match a content hash.

    Args:
        hash: Content hash string.
        data: Raw byte data to verify.

    Returns:
        ``True`` if the data matches the hash, ``False`` otherwise.
    """
    return _native.content_hash_verify(hash, data)


def content_hash_hex_digest(hash: str) -> str:
    """Extract the hex digest from a content hash string.

    Args:
        hash: Content hash string (``sha256:<hex>``).

    Returns:
        The 64-character hex digest without the ``sha256:`` prefix.
    """
    return _native.content_hash_hex_digest(hash)


# ═══════════════════════════════════════════════════════════════════════
# Commit / Change Operations
# ═══════════════════════════════════════════════════════════════════════


def change_set(path: str, new_value: str, old_value: str | None = None) -> dict[str, Any]:
    """Create a ``Set`` change record.

    Args:
        path: Dot-notation path (e.g. ``"properties.species"``).
        new_value: New value string.
        old_value: Optional previous value string.

    Returns:
        Change dict with ``path``, ``operation``, ``old_value``, ``new_value``.
    """
    return cast(dict[str, Any], json.loads(_native.change_set(path, old_value, new_value)))


def change_delete(path: str, old_value: str) -> dict[str, Any]:
    """Create a ``Delete`` change record.

    Args:
        path: Dot-notation path.
        old_value: Previous value string.

    Returns:
        Change dict.
    """
    return cast(dict[str, Any], json.loads(_native.change_delete(path, old_value)))


def change_append(path: str, new_value: str) -> dict[str, Any]:
    """Create an ``Append`` change record.

    Args:
        path: Dot-notation path.
        new_value: New value to append.

    Returns:
        Change dict.
    """
    return cast(dict[str, Any], json.loads(_native.change_append(path, new_value)))


def commit_new(
    author: str,
    message: str,
    manifest_hash: str,
    changes: list[dict[str, Any]],
    parent: str | None = None,
) -> dict[str, Any]:
    """Create a new NAP commit object.

    Args:
        author: Author identifier.
        message: Human-readable commit message.
        manifest_hash: SHA-256 hash of the resulting manifest.
        changes: List of change dicts.
        parent: Optional parent commit hash.

    Returns:
        Commit dict with all fields including auto-computed ``id``.
    """
    return cast(
        dict[str, Any],
        json.loads(_native.commit_new(parent, author, message, manifest_hash, json.dumps(changes))),
    )


def commit_verify_id(commit: dict[str, Any]) -> bool:
    """Verify a commit's ID by re-computing the hash.

    Args:
        commit: A commit dict.

    Returns:
        ``True`` if the ID is valid, ``False`` otherwise.
    """
    return _native.commit_verify_id(json.dumps(commit))


# ═══════════════════════════════════════════════════════════════════════
# Repository Operations
# ═══════════════════════════════════════════════════════════════════════


def repo_init(universe: str, base_path: str | None = None) -> dict[str, Any]:
    """Initialize a new NAP universe repository.

    Args:
        universe: Universe name.
        base_path: Base directory for universe repos (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``root`` (filesystem path) and ``universe``.
    """
    return cast(dict[str, Any], json.loads(_native.repo_init(_resolve_repo_path(base_path), universe)))


def repo_open(universe: str, base_path: str | None = None) -> dict[str, Any]:
    """Open an existing NAP universe repository.

    Args:
        universe: Universe name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``root`` and ``universe``.
    """
    return cast(dict[str, Any], json.loads(_native.repo_open(_resolve_repo_path(base_path), universe)))


def repo_create_entity(
    universe: str,
    entity_type: str,
    entity_id: str,
    name: str,
    author: str = "nap-sdk",
    base_path: str | None = None,
) -> dict[str, Any]:
    """Create a new entity manifest and commit it.

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        entity_id: Entity ID slug.
        name: Human-readable name.
        author: Author identifier.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``manifest`` and ``commit_hash``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_create_entity(_resolve_repo_path(base_path), universe, entity_type, entity_id, name, author)
        ),
    )


def repo_read_manifest(
    universe: str,
    entity_type: str,
    entity_id: str,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Read a manifest from the repository.

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        entity_id: Entity ID.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        The manifest as a dict.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_read_manifest(_resolve_repo_path(base_path), universe, entity_type, entity_id)
        ),
    )


def repo_read_manifest_at_ref(
    universe: str,
    entity_type: str,
    entity_id: str,
    reference: str,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Read a manifest at a specific VCS reference (commit, branch, tag).

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        entity_id: Entity ID.
        reference: VCS ref (commit hash, branch name, or tag).
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        The manifest as a dict.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_read_manifest_at_ref(
                _resolve_repo_path(base_path), universe, entity_type, entity_id, reference
            )
        ),
    )


def repo_write_manifest(
    universe: str,
    manifest: dict[str, Any],
    base_path: str | None = None,
) -> str:
    """Write a manifest to the repository (does NOT commit).

    Args:
        universe: Universe name.
        manifest: Manifest dict.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        The filesystem path where the manifest was written.
    """
    return _native.repo_write_manifest(_resolve_repo_path(base_path), universe, json.dumps(manifest))


def repo_commit_manifest(
    universe: str,
    entity_type: str,
    entity_id: str,
    message: str,
    author: str = "nap-sdk",
    changes: list[dict[str, Any]] | None = None,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Update an existing manifest and commit the changes.

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        entity_id: Entity ID.
        message: Commit message.
        author: Author identifier.
        changes: List of change dicts.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``commit`` and ``version``.
    """
    changes_json = json.dumps(changes or [])
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_commit_manifest(
                _resolve_repo_path(base_path), universe, entity_type, entity_id,
                message, author, changes_json
            )
        ),
    )


def repo_delete_entity(
    universe: str,
    entity_type: str,
    entity_id: str,
    author: str = "nap-sdk",
    base_path: str | None = None,
) -> str:
    """Delete an entity manifest and commit the deletion.

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        entity_id: Entity ID.
        author: Author identifier.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        The VCS commit hash of the deletion.
    """
    return _native.repo_delete_entity(
        _resolve_repo_path(base_path), universe, entity_type, entity_id, author
    )


def repo_history(
    universe: str,
    entity_type: str,
    entity_id: str,
    limit: int = 20,
    base_path: str | None = None,
) -> list[dict[str, Any]]:
    """Get commit history for an entity.

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        entity_id: Entity ID.
        limit: Maximum number of commits to return (default 20).
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        List of commit info dicts with ``id``, ``author``, ``message``, ``timestamp``.
    """
    return cast(
        list[dict[str, Any]],
        json.loads(
            _native.repo_history(
                _resolve_repo_path(base_path), universe, entity_type, entity_id, limit
            )
        ),
    )


def repo_list_entities(
    universe: str,
    entity_type: str,
    base_path: str | None = None,
) -> list[str]:
    """List all entity IDs of a given type in a universe.

    Args:
        universe: Universe name.
        entity_type: Entity type string.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        List of entity ID strings.
    """
    return cast(
        list[str],
        json.loads(
            _native.repo_list_entities(_resolve_repo_path(base_path), universe, entity_type)
        ),
    )


def repo_create_branch(
    universe: str,
    name: str,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Create a branch in a universe repository.

    Args:
        universe: Universe name.
        name: Branch name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``success`` and ``branch``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_create_branch(_resolve_repo_path(base_path), universe, name)
        ),
    )


def repo_switch_branch(
    universe: str,
    name: str,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Switch to a branch in a universe repository.

    Args:
        universe: Universe name.
        name: Branch name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``success`` and ``branch``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_switch_branch(_resolve_repo_path(base_path), universe, name)
        ),
    )


def repo_list_branches(
    universe: str,
    base_path: str | None = None,
) -> list[str]:
    """List all branches in a universe repository.

    Args:
        universe: Universe name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        List of branch names.
    """
    return cast(
        list[str],
        json.loads(
            _native.repo_list_branches(_resolve_repo_path(base_path), universe)
        ),
    )


def repo_create_tag(
    universe: str,
    name: str,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Create a tag in a universe repository.

    Args:
        universe: Universe name.
        name: Tag name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``success`` and ``tag``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_create_tag(_resolve_repo_path(base_path), universe, name)
        ),
    )


def repo_list_tags(
    universe: str,
    base_path: str | None = None,
) -> list[str]:
    """List all tags in a universe repository.

    Args:
        universe: Universe name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        List of tag names.
    """
    return cast(
        list[str],
        json.loads(
            _native.repo_list_tags(_resolve_repo_path(base_path), universe)
        ),
    )


def repo_head_hash(
    universe: str,
    base_path: str | None = None,
) -> str:
    """Get the current HEAD hash of a universe repository.

    Args:
        universe: Universe name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        The HEAD commit hash string.
    """
    return _native.repo_head_hash(_resolve_repo_path(base_path), universe)


def repo_revert_commit(
    universe: str,
    commit_hash: str,
    author: str = "nap-sdk",
    base_path: str | None = None,
) -> str:
    """Revert a commit across an entire universe.

    Args:
        universe: Universe name.
        commit_hash: Hash of the commit to revert.
        author: Author identifier.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        The new revert commit hash.
    """
    return _native.repo_revert_commit(_resolve_repo_path(base_path), universe, commit_hash, author)


def repo_add_remote(
    universe: str,
    name: str,
    url: str,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Add a remote to a universe repository.

    Args:
        universe: Universe name.
        name: Remote name (e.g. ``"origin"``).
        url: Remote URL.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``success``, ``remote``, and ``url``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_add_remote(_resolve_repo_path(base_path), universe, name, url)
        ),
    )


def repo_remove_remote(
    universe: str,
    name: str,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Remove a remote from a universe repository.

    Args:
        universe: Universe name.
        name: Remote name to remove.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``success`` and ``remote``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_remove_remote(_resolve_repo_path(base_path), universe, name)
        ),
    )


def repo_list_remotes(
    universe: str,
    base_path: str | None = None,
) -> list[tuple[str, str]]:
    """List remotes on a universe repository.

    Args:
        universe: Universe name.
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        List of ``(name, url)`` tuples.
    """
    raw = json.loads(_native.repo_list_remotes(_resolve_repo_path(base_path), universe))
    return [(item[0], item[1]) for item in raw]


def repo_push(
    universe: str,
    remote: str | None = None,
    branch: str | None = None,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Push the current branch to a remote.

    Args:
        universe: Universe name.
        remote: Remote name (defaults to tracking branch's remote).
        branch: Branch to push (defaults to current branch).
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``success``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_push(_resolve_repo_path(base_path), universe, remote, branch)
        ),
    )


def repo_pull(
    universe: str,
    remote: str | None = None,
    branch: str | None = None,
    base_path: str | None = None,
) -> dict[str, Any]:
    """Pull the current branch from a remote.

    Args:
        universe: Universe name.
        remote: Remote name (defaults to tracking branch's remote).
        branch: Branch to pull (defaults to current branch).
        base_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        Dict with ``success``.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.repo_pull(_resolve_repo_path(base_path), universe, remote, branch)
        ),
    )


# ═══════════════════════════════════════════════════════════════════════
# Resolver Operations
# ═══════════════════════════════════════════════════════════════════════


def resolve(
    uri: str,
    repo_path: str | None = None,
    branch: str | None = None,
    commit: str | None = None,
    tag: str | None = None,
    path: str | None = None,
) -> dict[str, Any]:
    """Resolve a NAP URI to a manifest or subtree.

    Args:
        uri: NAP URI (e.g. ``"nap://starwars/character/lukeskywalker"``).
        repo_path: Base directory for universes (defaults to ``$NAP_DIR`` / ``~/.nap``).
        branch: Optional branch selector.
        commit: Optional commit hash selector.
        tag: Optional tag selector.
        path: Optional subtree query path.

    Returns:
        The resolved manifest dict or subtree value.
    """
    repo_path = _resolve_repo_path(repo_path)
    if branch is not None or commit is not None or tag is not None or path is not None:
        result = _native.resolve_with_options(uri, repo_path, branch, commit, tag, path)
    else:
        result = _native.resolve(uri, repo_path)
    return cast(dict[str, Any], json.loads(result))


def resolve_query(uri: str, path: str, repo_path: str | None = None) -> Any:
    """Query a specific subtree path from a manifest.

    This is the most efficient way to read a single property from an entity.

    Args:
        uri: NAP URI.
        path: Dot-notation query path (e.g. ``"properties.species"``).
        repo_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        The value at the given path.
    """
    return json.loads(
        _native.resolve_query(uri, _resolve_repo_path(repo_path), path)
    )


def list_universes(repo_path: str | None = None) -> list[str]:
    """List all universe repositories available.

    Args:
        repo_path: Base directory (defaults to ``$NAP_DIR`` / ``~/.nap``).

    Returns:
        List of universe names.
    """
    return cast(
        list[str],
        json.loads(_native.list_universes(_resolve_repo_path(repo_path))),
    )


# ═══════════════════════════════════════════════════════════════════════
# Schema Operations
# ═══════════════════════════════════════════════════════════════════════


def manifest_schema() -> dict[str, Any]:
    """Get the JSON Schema for a NAP manifest.

    Returns:
        JSON Schema dict for the Manifest type.
    """
    return cast(dict[str, Any], json.loads(_native.manifest_schema_json()))


def commit_schema() -> dict[str, Any]:
    """Get the JSON Schema for a NAP commit.

    Returns:
        JSON Schema dict for the Commit type.
    """
    return cast(dict[str, Any], json.loads(_native.commit_schema_json()))


def validate_manifest(manifest: dict[str, Any]) -> dict[str, Any]:
    """Validate a manifest against the manifest schema.

    Args:
        manifest: A manifest dict.

    Returns:
        Dict with ``valid`` (bool) and optionally ``errors`` (list of strings).
    """
    return cast(dict[str, Any], json.loads(_native.validate_manifest(json.dumps(manifest))))


def validate_commit(commit: dict[str, Any]) -> dict[str, Any]:
    """Validate a commit against the commit schema.

    Args:
        commit: A commit dict.

    Returns:
        Dict with ``valid`` (bool) and optionally ``errors`` (list of strings).
    """
    return cast(dict[str, Any], json.loads(_native.validate_commit(json.dumps(commit))))


# ═══════════════════════════════════════════════════════════════════════
# Merge Operations
# ═══════════════════════════════════════════════════════════════════════


def merge_merge(
    schema: dict[str, Any],
    base: dict[str, Any],
    current: dict[str, Any],
    proposed: dict[str, Any],
) -> dict[str, Any]:
    """Three-way merge of manifest values.

    Args:
        schema: The SDL schema document dict.
        base: The base (common ancestor) value.
        current: The current value (what we have).
        proposed: The proposed new value.

    Returns:
        Merge result dict with either a ``Merged`` value or ``Conflicts`` list.
    """
    return cast(
        dict[str, Any],
        json.loads(
            _native.merge_merge(
                json.dumps(schema),
                json.dumps(base),
                json.dumps(current),
                json.dumps(proposed),
            )
        ),
    )


def merge_diff(
    schema: dict[str, Any],
    base: dict[str, Any],
    candidate: dict[str, Any],
) -> list[dict[str, Any]]:
    """Compute the diff between two manifest values.

    Args:
        schema: The schema definition (SDL document as JSON dict).
        base: The base value.
        candidate: The candidate value to compare against base.

    Returns:
        List of change dicts describing the differences.
    """
    return cast(
        list[dict[str, Any]],
        json.loads(
            _native.merge_diff(
                json.dumps(schema),
                json.dumps(base),
                json.dumps(candidate),
            )
        ),
    )


# ═══════════════════════════════════════════════════════════════════════
# Storage Engine Operations
# ═══════════════════════════════════════════════════════════════════════


def storage_config() -> dict[str, Any]:
    """Get the active storage engine configuration.

    Returns:
        Dict with ``backend``, ``base_dir``, ``assets_prefix``, and ``bucket``.
    """
    return cast(dict[str, Any], json.loads(_native.storage_config()))


def ingest_media(data: bytes, format: str) -> str:
    """Ingest raw media bytes into the content-addressed storage engine.

    The storage backend is determined by the ``NAP_STORAGE_BACKEND``
    environment variable at the Rust layer (``local`` or ``s3``).

    Args:
        data: Raw bytes of the media asset (image, audio, mesh, etc.).
        format: File extension without a leading dot (e.g. ``"png"``,
            ``"jpg"``, ``"wav"``, ``"glb"``).

    Returns:
        The content-addressed hash ``sha256:<hex>``.
    """
    return _native.ingest_media(data, format)


# ═══════════════════════════════════════════════════════════════════════
# VCS / Lore Operations
# ═══════════════════════════════════════════════════════════════════════


def lore_clone(url: str, dest_path: str) -> dict[str, Any]:
    """Clone a Lore repository.

    Args:
        url: Lore remote URL.
        dest_path: Local destination path.

    Returns:
        Dict with ``success``, ``url``, and ``path``.
    """
    return cast(dict[str, Any], json.loads(_native.lore_clone(url, dest_path)))


# ═══════════════════════════════════════════════════════════════════════
# Version
# ═══════════════════════════════════════════════════════════════════════


def version() -> str:
    """Return the nap-sdk version string."""
    return _native.version()


# ═══════════════════════════════════════════════════════════════════════
# Module exports
# ═══════════════════════════════════════════════════════════════════════

__all__ = [
    # URI
    "parse_uri",
    "uri_new",
    "uri_identity",
    "uri_manifest_path",
    "uri_format",
    # EntityType
    "entity_type_parse",
    "entity_type_directory_name",
    "entity_type_list",
    # Manifest
    "parse_manifest",
    "manifest_new",
    "manifest_to_yaml",
    "manifest_from_yaml",
    "manifest_content_hash",
    "manifest_set_property",
    "manifest_add_reference",
    "manifest_set_representation",
    "manifest_bump_version",
    # ContentHash
    "content_hash_from_bytes",
    "content_hash_from_string",
    "content_hash_parse",
    "content_hash_verify",
    "content_hash_hex_digest",
    # Commit / Change
    "change_set",
    "change_delete",
    "change_append",
    "commit_new",
    "commit_verify_id",
    # Repository
    "repo_init",
    "repo_open",
    "repo_create_entity",
    "repo_read_manifest",
    "repo_read_manifest_at_ref",
    "repo_write_manifest",
    "repo_commit_manifest",
    "repo_delete_entity",
    "repo_history",
    "repo_list_entities",
    "repo_create_branch",
    "repo_switch_branch",
    "repo_list_branches",
    "repo_create_tag",
    "repo_list_tags",
    "repo_head_hash",
    "repo_revert_commit",
    "repo_add_remote",
    "repo_remove_remote",
    "repo_list_remotes",
    "repo_push",
    "repo_pull",
    # Resolver
    "resolve",
    "resolve_query",
    "list_universes",
    # Schema
    "manifest_schema",
    "commit_schema",
    "validate_manifest",
    "validate_commit",
    # Merge
    "merge_merge",
    "merge_diff",
    # Storage
    "storage_config",
    "ingest_media",
    # VCS
    "lore_clone",
    # Version
    "version",
]
