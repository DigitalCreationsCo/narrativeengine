import { createRequire } from "node:module";

interface NativeBindings {
  // URI
  parseUri(uri: string): string;
  uriNew(universe: string, entityType: string, entityId: string, fragment?: string): string;
  uriIdentity(uri: string): string;
  uriManifestPath(uri: string): string;
  uriFormat(universe: string, entityType: string, entityId: string, fragment?: string): string;

  // EntityType
  entityTypeParse(s: string): string;
  entityTypeDirectoryName(entityType: string): string;
  entityTypeList(): string;

  // Manifest
  parseManifest(yamlStr: string): string;
  manifestNew(universe: string, entityType: string, entityId: string, name: string): string;
  manifestToYaml(jsonStr: string): string;
  manifestFromYaml(yamlStr: string): string;
  manifestContentHash(jsonStr: string): string;
  manifestSetProperty(jsonStr: string, key: string, value: string): string;
  manifestAddReference(jsonStr: string, key: string, value: string): string;
  manifestSetRepresentation(jsonStr: string, key: string, hash: string, format: string, uri?: string, tier?: string): string;
  manifestBumpVersion(jsonStr: string): string;

  // ContentHash
  contentHashFromBytes(data: Buffer): string;
  contentHashFromString(s: string): string;
  contentHashParse(s: string): string;
  contentHashVerify(hash: string, data: Buffer): boolean;
  contentHashHexDigest(hash: string): string;

  // Commit / Change
  changeSet(path: string, oldValue: string | undefined | null, newValue: string): string;
  changeDelete(path: string, oldValue: string): string;
  changeAppend(path: string, newValue: string): string;
  commitNew(parent: string | undefined | null, author: string, message: string, manifestHash: string, changesJson: string): string;
  commitVerifyId(jsonStr: string): boolean;

  // Repository
  repoInit(basePath: string, universe: string): string;
  repoOpen(basePath: string, universe: string): string;
  repoCreateEntity(basePath: string, universe: string, entityType: string, entityId: string, name: string, author: string): string;
  repoReadManifest(basePath: string, universe: string, entityType: string, entityId: string): string;
  repoReadManifestAtRef(basePath: string, universe: string, entityType: string, entityId: string, reference: string): string;
  repoWriteManifest(basePath: string, universe: string, manifestJson: string): string;
  repoCommitManifest(basePath: string, universe: string, entityType: string, entityId: string, message: string, author: string, changesJson: string): string;
  repoDeleteEntity(basePath: string, universe: string, entityType: string, entityId: string, author: string): string;
  repoHistory(basePath: string, universe: string, entityType: string, entityId: string, limit: number): string;
  repoListEntities(basePath: string, universe: string, entityType: string): string;
  repoCreateBranch(basePath: string, universe: string, name: string): string;
  repoSwitchBranch(basePath: string, universe: string, name: string): string;
  repoListBranches(basePath: string, universe: string): string;
  repoCreateTag(basePath: string, universe: string, name: string): string;
  repoListTags(basePath: string, universe: string): string;
  repoHeadHash(basePath: string, universe: string): string;
  repoRevertCommit(basePath: string, universe: string, commitHash: string, author: string): string;
  repoAddRemote(basePath: string, universe: string, name: string, url: string): string;
  repoRemoveRemote(basePath: string, universe: string, name: string): string;
  repoListRemotes(basePath: string, universe: string): string;
  repoPush(basePath: string, universe: string, remote?: string, branch?: string): string;
  repoPull(basePath: string, universe: string, remote?: string, branch?: string): string;

  // Resolver
  resolve(uri: string, repoPath: string): string;
  resolveWithOptions(uri: string, repoPath: string, branch?: string, commit?: string, tag?: string, path?: string): string;
  resolveQuery(uri: string, repoPath: string, path: string): string;
  listUniverses(repoPath: string): string;

  // Schema
  manifestSchema(): string;
  commitSchema(): string;
  validateManifest(jsonStr: string): string;
  validateCommit(jsonStr: string): string;

  // Merge
  mergeMerge(schemaJson: string, base: string, current: string, proposed: string): string;
  mergeDiff(schemaJson: string, base: string, candidate: string): string;

  // Storage
  storageConfig(): string;
  ingestMedia(data: Buffer, format: string): Promise<string>;

  // VCS
  loreClone(url: string, destPath: string): string;

  // Version
  version(): string;
}

const require = createRequire(import.meta.url);
const native = require("../index.cjs") as NativeBindings;

export default native;
