#!/usr/bin/env node

/**
 * Generates type definitions for all SDK languages from Rust schemas.
 * Cross-platform equivalent of generate-types.sh — works on Windows, macOS, and Linux.
 *
 * Usage: node scripts/generate-types.mjs
 */

import { execSync } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(__dirname, "..");

const manifest = resolve(rootDir, "Cargo.toml");

if (!existsSync(manifest)) {
  console.error(`FATAL: Cargo manifest not found at ${manifest}`);
  console.error("The script at scripts/generate-types.mjs could not locate Cargo.toml.");
  console.error("Ensure the script has not been moved relative to the repository root.");
  process.exit(1);
}

const targets = [
  ["python", resolve(rootDir, "python/narrativeengine/types.py")],
  ["typescript", resolve(rootDir, "typescript/src/models.ts")],
  ["go", resolve(rootDir, "generated/go/models.go")],
  ["java", resolve(rootDir, "generated/java/NarrativeModels.java")],
  ["csharp", resolve(rootDir, "generated/csharp/NarrativeModels.cs")],
  ["swift", resolve(rootDir, "generated/swift/NarrativeModels.swift")],
  ["kotlin", resolve(rootDir, "generated/kotlin/NarrativeModels.kt")],
];

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
    execSync(cmd, { stdio: "inherit", cwd: rootDir });
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
