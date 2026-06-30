/**
 * NAP SDK — TypeScript bindings for the Narrative Addressing Protocol.
 *
 * Powered by Rust via NAPI-RS native addon.  All functions that return
 * structured data return plain objects (JSON-deserialized from the native
 * layer).
 *
 * @module nap-sdk
 */

import { homedir } from "node:os";
import { join } from "node:path";
import native from "./native.js";

// ── Types ────────────────────────────────────────────────────────────

/** A parsed NAP URI. */
export interface NapUri {
  universe: string;
  entity_type: string;
  entity_id: string;
  fragment?: string;
}

/** A NAP manifest — the canonical representation of a narrative resource. */
export interface Manifest {
  id: string;
  name: string;
  entity_type: string;
  version: number;
  properties: Record<string, unknown>;
  representations: Record<string, Representation>;
  references: Record<string, unknown>;
  provenance?: Provenance;
  head?: string;
  metadata?: Record<string, unknown>;
}

/** A content-addressed representation (image, voice model, mesh, etc.). */
export interface Representation {
  hash: string;
  format: string;
  uri?: string;
  tier?: string;
}

/** AI generation provenance metadata. */
export interface Provenance {
  model?: string;
  prompt_hash?: string;
  seed?: string;
  parameters?: Record<string, string>;
  derived_from?: string;
  created_at?: string;
}

/** A NAP commit object. */
export interface Commit {
  id: string;
  parent?: string;
  timestamp: string;
  author: string;
  signature?: string;
  message: string;
  manifest_hash: string;
  changes: Change[];
}

/** A single change within a commit. */
export interface Change {
  path: string;
  operation: "set" | "delete" | "append" | "remove";
  old_value?: string;
  new_value?: string;
}

/** Commit info from VCS history. */
export interface CommitEntry {
  id: string;
  parent?: string;
  author: string;
  message: string;
  timestamp: string;
}

/** Remote entry (name, URL) pair. */
export type RemoteEntry = [string, string];

/** Repo info result. */
export interface RepoInfo {
  root: string;
  universe: string;
}

/** Operation result with commit. */
export interface CommitResult {
  commit: Commit;
  version: number;
}

/** Entity creation result. */
export interface CreateEntityResult {
  manifest: Manifest;
  commit_hash: string;
}

/** Validation result. */
export interface ValidationResult {
  valid: boolean;
  errors?: string[];
}

/** Storage config. */
export interface StorageConfig {
  backend: string;
  base_dir: string;
  assets_prefix: string;
  bucket: string;
}

/** Generic success response. */
export interface SuccessResponse {
  success: boolean;
  [key: string]: unknown;
}

/** Merge result. */
export type MergeResult =
  | { Merged: Record<string, unknown> }
  | { Conflicts: Array<{ path: string; conflict_type: string; base: unknown; current: unknown; proposed: unknown }> };

/** Merge diff entry. */
export interface DiffEntry {
  path: string;
  operation: string;
  old_value?: unknown;
  new_value?: unknown;
}

// ── Helpers ──────────────────────────────────────────────────────────

/** Resolve the NAP base directory from env or default. */
function resolveRepoPath(repoPath?: string): string {
  return repoPath || process.env.NAP_DIR || join(homedir(), ".nap");
}

// ═══════════════════════════════════════════════════════════════════════
// URI Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Parse a `nap://` URI into its component parts.
 *
 * @param uri - A NAP URI (e.g. `"nap://starwars/character/lukeskywalker"`)
 * @returns Parsed URI components
 */
export function parseUri(uri: string): NapUri {
  return JSON.parse(native.parseUri(uri)) as NapUri;
}

/**
 * Construct a new NAP URI from components.
 *
 * @param universe - Universe name (e.g. `"starwars"`)
 * @param entityType - Entity type (e.g. `"character"`)
 * @param entityId - Entity ID slug (e.g. `"lukeskywalker"`)
 * @param fragment - Optional query fragment
 * @returns Parsed URI components
 */
export function uriNew(
  universe: string,
  entityType: string,
  entityId: string,
  fragment?: string,
): NapUri {
  return JSON.parse(native.uriNew(universe, entityType, entityId, fragment)) as NapUri;
}

/**
 * Return the canonical identity URI (without fragment).
 *
 * @param uri - A NAP URI, possibly with a fragment
 * @returns Identity URI string
 */
export function uriIdentity(uri: string): string {
  return native.uriIdentity(uri);
}

/**
 * Return the relative filesystem path for an entity's manifest.
 *
 * @param uri - A NAP URI
 * @returns Relative path (e.g. `"characters/lukeskywalker.yaml"`)
 */
export function uriManifestPath(uri: string): string {
  return native.uriManifestPath(uri);
}

/**
 * Format URI components into a `nap://` URI string.
 *
 * @param universe - Universe name
 * @param entityType - Entity type
 * @param entityId - Entity ID
 * @param fragment - Optional query fragment
 * @returns Full NAP URI string
 */
export function uriFormat(
  universe: string,
  entityType: string,
  entityId: string,
  fragment?: string,
): string {
  return native.uriFormat(universe, entityType, entityId, fragment);
}

// ═══════════════════════════════════════════════════════════════════════
// EntityType Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Parse an entity type string.
 *
 * @param s - Type string (e.g. `"character"`, `"location"`, `"scene"`, `"prop"`, `"world"`)
 * @returns The normalized entity type string
 */
export function entityTypeParse(s: string): string {
  return JSON.parse(native.entityTypeParse(s)) as string;
}

/**
 * Return the directory name used for this entity type in a repository.
 *
 * @param entityType - Type string
 * @returns Directory name (e.g. `"characters"`)
 */
export function entityTypeDirectoryName(entityType: string): string {
  return native.entityTypeDirectoryName(entityType);
}

/**
 * Return all subdirectory entity types (character, location, scene, prop).
 *
 * @returns Array of entity type strings
 */
export function entityTypeList(): string[] {
  return JSON.parse(native.entityTypeList()) as string[];
}

// ═══════════════════════════════════════════════════════════════════════
// Manifest Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Parse a YAML manifest string into a JSON-serializable object.
 *
 * @param yamlStr - YAML string representing a NAP manifest
 * @returns Parsed manifest
 */
export function parseManifest(yamlStr: string): Manifest {
  return JSON.parse(native.parseManifest(yamlStr)) as Manifest;
}

/**
 * Create a new manifest with minimal required fields.
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param entityId - Entity ID slug
 * @param name - Human-readable name
 * @returns New manifest
 */
export function manifestNew(
  universe: string,
  entityType: string,
  entityId: string,
  name: string,
): Manifest {
  return JSON.parse(native.manifestNew(universe, entityType, entityId, name)) as Manifest;
}

/**
 * Serialize a manifest object to YAML.
 *
 * @param manifest - Manifest object
 * @returns YAML string representation
 */
export function manifestToYaml(manifest: Manifest): string {
  return native.manifestToYaml(JSON.stringify(manifest));
}

/**
 * Read a manifest from a YAML string.
 *
 * @param yamlStr - YAML string
 * @returns Parsed manifest
 */
export function manifestFromYaml(yamlStr: string): Manifest {
  return JSON.parse(native.manifestFromYaml(yamlStr)) as Manifest;
}

/**
 * Compute the SHA-256 content hash of a manifest.
 *
 * @param manifest - Manifest object
 * @returns Content hash in `sha256:<hex>` format
 */
export function manifestContentHash(manifest: Manifest): string {
  return native.manifestContentHash(JSON.stringify(manifest));
}

/**
 * Add or update a property on a manifest.
 *
 * @param manifest - Manifest object
 * @param key - Property key
 * @param value - Property value string (parsed as YAML)
 * @returns Updated manifest
 */
export function manifestSetProperty(manifest: Manifest, key: string, value: string): Manifest {
  return JSON.parse(native.manifestSetProperty(JSON.stringify(manifest), key, value)) as Manifest;
}

/**
 * Add a cross-reference to a manifest.
 *
 * @param manifest - Manifest object
 * @param key - Reference key
 * @param value - Reference value string (parsed as YAML)
 * @returns Updated manifest
 */
export function manifestAddReference(manifest: Manifest, key: string, value: string): Manifest {
  return JSON.parse(native.manifestAddReference(JSON.stringify(manifest), key, value)) as Manifest;
}

/**
 * Add or update a representation on a manifest.
 *
 * @param manifest - Manifest object
 * @param key - Representation key
 * @param hash - Content hash string
 * @param format - File format (e.g. `"png"`, `"glb"`)
 * @param uri - Optional storage URI
 * @param tier - Optional quality tier
 * @returns Updated manifest
 */
export function manifestSetRepresentation(
  manifest: Manifest,
  key: string,
  hash: string,
  format: string,
  uri?: string,
  tier?: string,
): Manifest {
  return JSON.parse(
    native.manifestSetRepresentation(JSON.stringify(manifest), key, hash, format, uri, tier),
  ) as Manifest;
}

/**
 * Increment the version counter on a manifest.
 *
 * @param manifest - Manifest object
 * @returns Updated manifest with version incremented
 */
export function manifestBumpVersion(manifest: Manifest): Manifest {
  return JSON.parse(native.manifestBumpVersion(JSON.stringify(manifest))) as Manifest;
}

// ═══════════════════════════════════════════════════════════════════════
// ContentHash Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Compute the SHA-256 content hash of raw bytes.
 *
 * @param data - Raw byte data
 * @returns Content hash `sha256:<hex>`
 */
export function contentHashFromBytes(data: Buffer): string {
  return native.contentHashFromBytes(data);
}

/**
 * Compute the SHA-256 content hash of a string.
 *
 * @param s - Input string
 * @returns Content hash `sha256:<hex>`
 */
export function contentHashFromString(s: string): string {
  return native.contentHashFromString(s);
}

/**
 * Parse and validate a `sha256:<hex>` content hash string.
 *
 * @param s - Content hash string
 * @returns The validated content hash
 * @throws If the string is not a valid content hash
 */
export function contentHashParse(s: string): string {
  return native.contentHashParse(s);
}

/**
 * Verify that bytes match a content hash.
 *
 * @param hash - Content hash string
 * @param data - Raw byte data to verify
 * @returns `true` if the data matches the hash
 */
export function contentHashVerify(hash: string, data: Buffer): boolean {
  return native.contentHashVerify(hash, data);
}

/**
 * Extract the hex digest from a content hash string.
 *
 * @param hash - Content hash string (`sha256:<hex>`)
 * @returns The 64-character hex digest
 */
export function contentHashHexDigest(hash: string): string {
  return native.contentHashHexDigest(hash);
}

// ═══════════════════════════════════════════════════════════════════════
// Commit / Change Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Create a "Set" change record.
 *
 * @param path - Dot-notation path (e.g. `"properties.species"`)
 * @param newValue - New value string
 * @param oldValue - Optional previous value string
 * @returns Change object
 */
export function changeSet(path: string, newValue: string, oldValue?: string): Change {
  return JSON.parse(native.changeSet(path, oldValue ?? null, newValue)) as Change;
}

/**
 * Create a "Delete" change record.
 *
 * @param path - Dot-notation path
 * @param oldValue - Previous value string
 * @returns Change object
 */
export function changeDelete(path: string, oldValue: string): Change {
  return JSON.parse(native.changeDelete(path, oldValue)) as Change;
}

/**
 * Create an "Append" change record.
 *
 * @param path - Dot-notation path
 * @param newValue - New value to append
 * @returns Change object
 */
export function changeAppend(path: string, newValue: string): Change {
  return JSON.parse(native.changeAppend(path, newValue)) as Change;
}

/**
 * Create a new NAP commit object.
 *
 * @param author - Author identifier
 * @param message - Human-readable commit message
 * @param manifestHash - SHA-256 hash of the resulting manifest
 * @param changes - Array of change objects
 * @param parent - Optional parent commit hash
 * @returns Commit object with auto-computed `id`
 */
export function commitNew(
  author: string,
  message: string,
  manifestHash: string,
  changes: Change[],
  parent?: string,
): Commit {
  return JSON.parse(
    native.commitNew(parent ?? null, author, message, manifestHash, JSON.stringify(changes)),
  ) as Commit;
}

/**
 * Verify a commit's ID by re-computing the hash.
 *
 * @param commit - Commit object
 * @returns `true` if the ID is valid
 */
export function commitVerifyId(commit: Commit): boolean {
  return native.commitVerifyId(JSON.stringify(commit));
}

// ═══════════════════════════════════════════════════════════════════════
// Repository Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Initialize a new NAP universe repository.
 *
 * @param universe - Universe name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Repo info with `root` and `universe`
 */
export function repoInit(universe: string, basePath?: string): RepoInfo {
  return JSON.parse(native.repoInit(resolveRepoPath(basePath), universe)) as RepoInfo;
}

/**
 * Open an existing NAP universe repository.
 *
 * @param universe - Universe name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Repo info
 */
export function repoOpen(universe: string, basePath?: string): RepoInfo {
  return JSON.parse(native.repoOpen(resolveRepoPath(basePath), universe)) as RepoInfo;
}

/**
 * Create a new entity manifest and commit it.
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param entityId - Entity ID slug
 * @param name - Human-readable name
 * @param author - Author identifier (default: `"nap-sdk"`)
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Object with `manifest` and `commit_hash`
 */
export function repoCreateEntity(
  universe: string,
  entityType: string,
  entityId: string,
  name: string,
  author: string = "nap-sdk",
  basePath?: string,
): CreateEntityResult {
  return JSON.parse(
    native.repoCreateEntity(resolveRepoPath(basePath), universe, entityType, entityId, name, author),
  ) as CreateEntityResult;
}

/**
 * Read a manifest from the repository.
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param entityId - Entity ID
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Manifest object
 */
export function repoReadManifest(
  universe: string,
  entityType: string,
  entityId: string,
  basePath?: string,
): Manifest {
  return JSON.parse(
    native.repoReadManifest(resolveRepoPath(basePath), universe, entityType, entityId),
  ) as Manifest;
}

/**
 * Read a manifest at a specific VCS reference (commit, branch, tag).
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param entityId - Entity ID
 * @param reference - VCS ref (commit hash, branch name, or tag)
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Manifest object
 */
export function repoReadManifestAtRef(
  universe: string,
  entityType: string,
  entityId: string,
  reference: string,
  basePath?: string,
): Manifest {
  return JSON.parse(
    native.repoReadManifestAtRef(resolveRepoPath(basePath), universe, entityType, entityId, reference),
  ) as Manifest;
}

/**
 * Write a manifest to the repository (does NOT commit).
 *
 * @param universe - Universe name
 * @param manifest - Manifest object
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns The filesystem path where the manifest was written
 */
export function repoWriteManifest(
  universe: string,
  manifest: Manifest,
  basePath?: string,
): string {
  return native.repoWriteManifest(resolveRepoPath(basePath), universe, JSON.stringify(manifest));
}

/**
 * Commit changes to a manifest.
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param entityId - Entity ID
 * @param message - Commit message
 * @param author - Author identifier (default: `"nap-sdk"`)
 * @param changes - Array of change objects
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Object with `commit` and `version`
 */
export function repoCommitManifest(
  universe: string,
  entityType: string,
  entityId: string,
  message: string,
  author: string = "nap-sdk",
  changes: Change[] = [],
  basePath?: string,
): CommitResult {
  return JSON.parse(
    native.repoCommitManifest(
      resolveRepoPath(basePath), universe, entityType, entityId,
      message, author, JSON.stringify(changes),
    ),
  ) as CommitResult;
}

/**
 * Delete an entity manifest and commit the deletion.
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param entityId - Entity ID
 * @param author - Author identifier (default: `"nap-sdk"`)
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns The VCS commit hash of the deletion
 */
export function repoDeleteEntity(
  universe: string,
  entityType: string,
  entityId: string,
  author: string = "nap-sdk",
  basePath?: string,
): string {
  return native.repoDeleteEntity(resolveRepoPath(basePath), universe, entityType, entityId, author);
}

/**
 * Get commit history for an entity.
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param entityId - Entity ID
 * @param limit - Maximum number of commits (default: 20)
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Array of commit entries
 */
export function repoHistory(
  universe: string,
  entityType: string,
  entityId: string,
  limit: number = 20,
  basePath?: string,
): CommitEntry[] {
  return JSON.parse(
    native.repoHistory(resolveRepoPath(basePath), universe, entityType, entityId, limit),
  ) as CommitEntry[];
}

/**
 * List all entity IDs of a given type in a universe.
 *
 * @param universe - Universe name
 * @param entityType - Entity type string
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Array of entity ID strings
 */
export function repoListEntities(
  universe: string,
  entityType: string,
  basePath?: string,
): string[] {
  return JSON.parse(
    native.repoListEntities(resolveRepoPath(basePath), universe, entityType),
  ) as string[];
}

/**
 * Create a branch in a universe repository.
 *
 * @param universe - Universe name
 * @param name - Branch name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Success response
 */
export function repoCreateBranch(
  universe: string,
  name: string,
  basePath?: string,
): SuccessResponse {
  return JSON.parse(
    native.repoCreateBranch(resolveRepoPath(basePath), universe, name),
  ) as SuccessResponse;
}

/**
 * Switch to a branch in a universe repository.
 *
 * @param universe - Universe name
 * @param name - Branch name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Success response
 */
export function repoSwitchBranch(
  universe: string,
  name: string,
  basePath?: string,
): SuccessResponse {
  return JSON.parse(
    native.repoSwitchBranch(resolveRepoPath(basePath), universe, name),
  ) as SuccessResponse;
}

/**
 * List all branches in a universe repository.
 *
 * @param universe - Universe name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Array of branch names
 */
export function repoListBranches(
  universe: string,
  basePath?: string,
): string[] {
  return JSON.parse(
    native.repoListBranches(resolveRepoPath(basePath), universe),
  ) as string[];
}

/**
 * Create a tag in a universe repository.
 *
 * @param universe - Universe name
 * @param name - Tag name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Success response
 */
export function repoCreateTag(
  universe: string,
  name: string,
  basePath?: string,
): SuccessResponse {
  return JSON.parse(
    native.repoCreateTag(resolveRepoPath(basePath), universe, name),
  ) as SuccessResponse;
}

/**
 * List all tags in a universe repository.
 *
 * @param universe - Universe name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Array of tag names
 */
export function repoListTags(
  universe: string,
  basePath?: string,
): string[] {
  return JSON.parse(
    native.repoListTags(resolveRepoPath(basePath), universe),
  ) as string[];
}

/**
 * Get the current HEAD hash of a universe repository.
 *
 * @param universe - Universe name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns The HEAD commit hash
 */
export function repoHeadHash(
  universe: string,
  basePath?: string,
): string {
  return native.repoHeadHash(resolveRepoPath(basePath), universe);
}

/**
 * Revert a commit across an entire universe.
 *
 * @param universe - Universe name
 * @param commitHash - Hash of the commit to revert
 * @param author - Author identifier (default: `"nap-sdk"`)
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns The new revert commit hash
 */
export function repoRevertCommit(
  universe: string,
  commitHash: string,
  author: string = "nap-sdk",
  basePath?: string,
): string {
  return native.repoRevertCommit(resolveRepoPath(basePath), universe, commitHash, author);
}

/**
 * Add a remote to a universe repository.
 *
 * @param universe - Universe name
 * @param name - Remote name (e.g. `"origin"`)
 * @param url - Remote URL
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Success response
 */
export function repoAddRemote(
  universe: string,
  name: string,
  url: string,
  basePath?: string,
): SuccessResponse {
  return JSON.parse(
    native.repoAddRemote(resolveRepoPath(basePath), universe, name, url),
  ) as SuccessResponse;
}

/**
 * Remove a remote from a universe repository.
 *
 * @param universe - Universe name
 * @param name - Remote name to remove
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Success response
 */
export function repoRemoveRemote(
  universe: string,
  name: string,
  basePath?: string,
): SuccessResponse {
  return JSON.parse(
    native.repoRemoveRemote(resolveRepoPath(basePath), universe, name),
  ) as SuccessResponse;
}

/**
 * List remotes on a universe repository.
 *
 * @param universe - Universe name
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Array of `[name, url]` tuples
 */
export function repoListRemotes(
  universe: string,
  basePath?: string,
): RemoteEntry[] {
  return JSON.parse(
    native.repoListRemotes(resolveRepoPath(basePath), universe),
  ) as RemoteEntry[];
}

/**
 * Push the current branch to a remote.
 *
 * @param universe - Universe name
 * @param remote - Remote name (optional)
 * @param branch - Branch to push (optional)
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Success response
 */
export function repoPush(
  universe: string,
  remote?: string,
  branch?: string,
  basePath?: string,
): SuccessResponse {
  return JSON.parse(
    native.repoPush(resolveRepoPath(basePath), universe, remote, branch),
  ) as SuccessResponse;
}

/**
 * Pull the current branch from a remote.
 *
 * @param universe - Universe name
 * @param remote - Remote name (optional)
 * @param branch - Branch to pull (optional)
 * @param basePath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Success response
 */
export function repoPull(
  universe: string,
  remote?: string,
  branch?: string,
  basePath?: string,
): SuccessResponse {
  return JSON.parse(
    native.repoPull(resolveRepoPath(basePath), universe, remote, branch),
  ) as SuccessResponse;
}

// ═══════════════════════════════════════════════════════════════════════
// Resolver Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Resolve a NAP URI to a manifest or subtree.
 *
 * @param uri - NAP URI (e.g. `"nap://starwars/character/lukeskywalker"`)
 * @param repoPath - Base directory for universes (defaults to `$NAP_DIR` / `~/.nap`)
 * @param branch - Optional branch selector
 * @param commit - Optional commit hash selector
 * @param tag - Optional tag selector
 * @param path - Optional subtree query path
 * @returns Resolved manifest or subtree value
 */
export function resolve(
  uri: string,
  repoPath?: string,
  branch?: string,
  commit?: string,
  tag?: string,
  path?: string,
): Record<string, unknown> {
  const rp = resolveRepoPath(repoPath);
  if (branch !== undefined || commit !== undefined || tag !== undefined || path !== undefined) {
    return JSON.parse(native.resolveWithOptions(uri, rp, branch, commit, tag, path)) as Record<string, unknown>;
  }
  return JSON.parse(native.resolve(uri, rp)) as Record<string, unknown>;
}

/**
 * Query a specific subtree path from a manifest.
 *
 * This is the most efficient way to read a single property from an entity.
 *
 * @param uri - NAP URI
 * @param path - Dot-notation query path (e.g. `"properties.species"`)
 * @param repoPath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns The value at the given path
 */
export function resolveQuery(
  uri: string,
  path: string,
  repoPath?: string,
): unknown {
  return JSON.parse(native.resolveQuery(uri, resolveRepoPath(repoPath), path));
}

/**
 * List all universe repositories available.
 *
 * @param repoPath - Base directory (defaults to `$NAP_DIR` / `~/.nap`)
 * @returns Array of universe names
 */
export function listUniverses(repoPath?: string): string[] {
  return JSON.parse(native.listUniverses(resolveRepoPath(repoPath))) as string[];
}

// ═══════════════════════════════════════════════════════════════════════
// Schema Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Get the JSON Schema for a NAP manifest.
 *
 * @returns JSON Schema object
 */
export function manifestSchema(): Record<string, unknown> {
  return JSON.parse(native.manifestSchema()) as Record<string, unknown>;
}

/**
 * Get the JSON Schema for a NAP commit.
 *
 * @returns JSON Schema object
 */
export function commitSchema(): Record<string, unknown> {
  return JSON.parse(native.commitSchema()) as Record<string, unknown>;
}

/**
 * Validate a manifest against the manifest schema.
 *
 * @param manifest - Manifest object
 * @returns Validation result with `valid` flag and optional `errors`
 */
export function validateManifest(manifest: Manifest): ValidationResult {
  return JSON.parse(native.validateManifest(JSON.stringify(manifest))) as ValidationResult;
}

/**
 * Validate a commit against the commit schema.
 *
 * @param commit - Commit object
 * @returns Validation result with `valid` flag and optional `errors`
 */
export function validateCommit(commit: Commit): ValidationResult {
  return JSON.parse(native.validateCommit(JSON.stringify(commit))) as ValidationResult;
}

// ═══════════════════════════════════════════════════════════════════════
// Merge Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Three-way merge of manifest values.
 *
 * @param schema - The SDL schema document
 * @param base - The base (common ancestor) value
 * @param current - The current value
 * @param proposed - The proposed new value
 * @returns Merge result (either `Merged` or `Conflicts`)
 */
export function mergeMerge(
  schema: Record<string, unknown>,
  base: Record<string, unknown>,
  current: Record<string, unknown>,
  proposed: Record<string, unknown>,
): MergeResult {
  return JSON.parse(
    native.mergeMerge(
      JSON.stringify(schema), JSON.stringify(base), JSON.stringify(current), JSON.stringify(proposed),
    ),
  ) as MergeResult;
}

/**
 * Compute the diff between two manifest values.
 *
 * @param schema - The schema definition (SDL document as JSON)
 * @param base - The base value
 * @param candidate - The candidate value to compare against base
 * @returns Array of change entries describing the differences
 */
export function mergeDiff(
  schema: Record<string, unknown>,
  base: Record<string, unknown>,
  candidate: Record<string, unknown>,
): DiffEntry[] {
  return JSON.parse(
    native.mergeDiff(
      JSON.stringify(schema),
      JSON.stringify(base),
      JSON.stringify(candidate),
    ),
  ) as DiffEntry[];
}

// ═══════════════════════════════════════════════════════════════════════
// Storage Engine Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Get the active storage engine configuration.
 *
 * @returns Storage config object
 */
export function storageConfig(): StorageConfig {
  return JSON.parse(native.storageConfig()) as StorageConfig;
}

/**
 * Ingest raw media bytes into the content-addressed storage engine.
 *
 * The storage backend is determined by the `NAP_STORAGE_BACKEND`
 * environment variable at the Rust layer (`"local"` or `"s3"`).
 *
 * @param data - Raw bytes of the media asset (image, audio, mesh, etc.)
 * @param format - File extension without a leading dot (e.g. `"png"`, `"jpg"`, `"wav"`, `"glb"`)
 * @returns Promise resolving to the content hash `sha256:<hex>`
 */
export async function ingestMedia(data: Buffer, format: string): Promise<string> {
  return native.ingestMedia(data, format);
}

// ═══════════════════════════════════════════════════════════════════════
// VCS / Lore Operations
// ═══════════════════════════════════════════════════════════════════════

/**
 * Clone a Lore repository.
 *
 * @param url - Lore remote URL
 * @param destPath - Local destination path
 * @returns Success response
 */
export function loreClone(url: string, destPath: string): SuccessResponse {
  return JSON.parse(native.loreClone(url, destPath)) as SuccessResponse;
}

// ═══════════════════════════════════════════════════════════════════════
// Version
// ═══════════════════════════════════════════════════════════════════════

/**
 * Return the nap-sdk version string.
 */
export function version(): string {
  return native.version();
}
