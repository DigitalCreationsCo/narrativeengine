#!/usr/bin/env node

/**
 * Generates type definitions for all SDK languages from Rust schemas.
 * Cross-platform equivalent of generate-types.sh — works on Windows, macOS, and Linux.
 *
 * Usage: node scripts/generate-types.mjs
 *
 * Also exported for testing:
 *   import { targets, rootDir, manifest, run } from "./generate-types.mjs";
 */

import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const __filename = fileURLToPath(import.meta.url);
export const rootDir = resolve(__dirname, "..");
export const manifest = resolve(rootDir, "Cargo.toml");

/** Language-to-output-path mapping for all SDK codegen targets. */
export const targets = [
  ["python", resolve(rootDir, "python/narrativeengine/types.py")],
  ["typescript", resolve(rootDir, "typescript/src/models.ts")],
  ["go", resolve(rootDir, "generated/go/models.go")],
  ["java", resolve(rootDir, "generated/java/NarrativeModels.java")],
  ["csharp", resolve(rootDir, "generated/csharp/NarrativeModels.cs")],
  ["swift", resolve(rootDir, "generated/swift/NarrativeModels.swift")],
  ["kotlin", resolve(rootDir, "generated/kotlin/NarrativeModels.kt")],
];

/**
 * Run codegen for every target.
 * Exits the process on first failure.
 * @param {object} [opts] - Options for testing.
 * @param {(cmd: string, opts: object) => void} [opts.exec] - Exec function (defaults to execSync).
 * @param {(path: string) => boolean} [opts.exists] - File-existence checker (defaults to existsSync).
 */
export function run({ exec = execSync, exists = existsSync } = {}) {
  if (!exists(manifest)) {
    const msg =
      `FATAL: Cargo manifest not found at ${manifest}\n` +
      "The script at scripts/generate-types.mjs could not locate Cargo.toml.\n" +
      "Ensure the script has not been moved relative to the repository root.";
    console.error(msg);
    process.exit(1);
  }

  for (const [lang, out] of targets) {
    const cmd = [
      "cargo",
      "run",
      "--quiet",
      `--manifest-path "${manifest}"`,
      "--package narrativeengine-codegen",
      "--",
      `--language ${lang}`,
      `--out "${out}"`,
    ].join(" ");

    try {
      exec(cmd, { stdio: "inherit", cwd: rootDir });
    } catch (err) {
      console.error(`Failed to generate ${lang} types -> ${out}`);
      if (err instanceof Error && "stderr" in err) {
        const stderr = err.stderr?.toString().trim();
        if (stderr) console.error(stderr);
      } else if (err instanceof Error) {
        console.error(err.message);
      }
      process.exit(1);
    }
  }
}

// Auto-execute when run as CLI, but not when imported for testing.
if (process.argv[1] === __filename) {
  run();
}
