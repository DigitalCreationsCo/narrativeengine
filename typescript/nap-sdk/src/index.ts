import { homedir } from "node:os";
import { join } from "node:path";
import native from "./native.js";

export function parseUri(uri: string): Record<string, unknown> {
  return JSON.parse(native.parseUri(uri)) as Record<string, unknown>;
}

export function parseManifest(yamlStr: string): Record<string, unknown> {
  return JSON.parse(native.parseManifest(yamlStr)) as Record<string, unknown>;
}

export function resolve(uri: string, repoPath?: string): Record<string, unknown> {
  if (!repoPath) {
    repoPath = process.env.NAP_DIR || join(homedir(), ".nap");
  }
  return JSON.parse(native.resolve(uri, repoPath)) as Record<string, unknown>;
}

export function version(): string {
  return native.version();
}
