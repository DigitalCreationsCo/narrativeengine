#!/usr/bin/env node

/**
 * Pre-publish validation script.
 *
 * Checks three things before publishing:
 *   1. Version consistency — Cargo.toml, pyproject.toml, and package.json
 *      all declare the same version.
 *   2. Release tag consistency — when running from a GitHub tag like v1.2.3,
 *      the tag version matches the package version being published.
 *   3. Type freshness — generated SDK type definitions match the current
 *      Rust models (runs `generate-types.mjs` and diffs the result).
 *
 * Exits with code 1 if any check fails.
 */

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(__dirname, "..");

let exitCode = 0;

const releaseTag = process.env.GITHUB_REF_NAME;
const tagVersion = releaseTag?.startsWith("v") ? releaseTag.slice(1) : null;

function pass(msg) {
  console.log(`  ✅  ${msg}`);
}

function fail(msg) {
  console.error(`  ❌  ${msg}`);
  exitCode = 1;
}

// =========================================================================
//  1. Version consistency
// =========================================================================

console.log("── Version consistency ─────────────────────────");

const cargoVersion = readFileSync(
  resolve(rootDir, "Cargo.toml"),
  "utf-8",
).match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const pyprojectVersion = readFileSync(
  resolve(rootDir, "python/pyproject.toml"),
  "utf-8",
).match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const packageVersion = JSON.parse(
  readFileSync(resolve(rootDir, "typescript/package.json"), "utf-8"),
).version;

// Guard: all three must be parseable
if (!cargoVersion) fail("Could not extract version from Cargo.toml");
if (!pyprojectVersion)
  fail("Could not extract version from python/pyproject.toml");
if (!packageVersion)
  fail("Could not extract version from typescript/package.json");

// Compare (only meaningful if all three parsed)
if (cargoVersion && pyprojectVersion && packageVersion) {
  if (cargoVersion !== pyprojectVersion)
    fail(`Cargo.toml (${cargoVersion}) ≠ pyproject.toml (${pyprojectVersion})`);
  if (cargoVersion !== packageVersion)
    fail(`Cargo.toml (${cargoVersion}) ≠ package.json (${packageVersion})`);

  if (exitCode === 0) pass(`All three declare version ${cargoVersion}`);
}

// =========================================================================
//  2. Release tag consistency
// =========================================================================

if (tagVersion) {
  console.log("\n── Release tag consistency ────────────────────");

  if (cargoVersion !== tagVersion) {
    fail(`Release tag ${releaseTag} ≠ package version ${cargoVersion}`);
  } else {
    pass(`Release tag ${releaseTag} matches package version ${cargoVersion}`);
  }
}

// =========================================================================
//  3. Type freshness
// =========================================================================

console.log("\n── Type freshness ─────────────────────────────");

try {
  execSync("node scripts/generate-types.mjs", {
    cwd: rootDir,
    stdio: "inherit",
  });
} catch {
  // generate-types.mjs calls process.exit(1) on failure — that kills us
  // too, so we only reach here if execSync itself throws unexpectedly.
  fail("Type generation script crashed; see output above");
  process.exit(1);
}

// The generated files live at these paths.  We check only these so that
// unrelated working-tree changes (e.g. the CI files we just edited) don't
// trigger a false positive.
const generatedPaths = [
  "typescript/src/models.ts",
  "python/narrativeengine/types.py",
  "generated/",
];

try {
  execSync(
    ["git diff --exit-code --", ...generatedPaths.map((p) => `"${p}"`)].join(
      " ",
    ),
    { cwd: rootDir, stdio: "pipe" },
  );
  pass("Generated types are up to date with committed code");
} catch (err) {
  fail(
    "Generated types are stale — run `npm run generate` and commit the " +
      "updated files before tagging.",
  );
  const diff =
    err?.stdout?.toString().trim() ?? err?.message ?? "(no diff available)";
  if (diff) process.stdout.write(diff + "\n");
}

// =========================================================================
//  Summary
// =========================================================================

console.log("");

if (exitCode !== 0) {
  console.error("❌ Pre-publish checks failed.");
  process.exit(1);
} else {
  console.log("✅ All pre-publish checks passed.");
}
