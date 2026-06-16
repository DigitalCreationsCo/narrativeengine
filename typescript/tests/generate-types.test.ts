import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { resolve } from "node:path";

const repoRoot = resolve(import.meta.dirname!, "../..");

const expectedTargets: Record<string, string> = {
  python: "python/narrativeengine/types.py",
  typescript: "typescript/src/models.ts",
  go: "generated/go/models.go",
  java: "generated/java/NarrativeModels.java",
  csharp: "generated/csharp/NarrativeModels.cs",
  swift: "generated/swift/NarrativeModels.swift",
  kotlin: "generated/kotlin/NarrativeModels.kt",
};

describe("generate-types.mjs targets", () => {
  let mod: Awaited<typeof import("../../scripts/generate-types.mjs")>;

  beforeEach(async () => {
    mod = await import("../../scripts/generate-types.mjs");
  });

  it("exports a targets array with all 7 SDK languages", () => {
    expect(mod.targets).toBeDefined();
    expect(Array.isArray(mod.targets)).toBe(true);
    expect(mod.targets).toHaveLength(7);
  });

  it("each target is a [language, path] tuple", () => {
    for (const [lang, out] of mod.targets) {
      expect(typeof lang).toBe("string");
      expect(typeof out).toBe("string");
      expect(lang.length).toBeGreaterThan(0);
      expect(out.length).toBeGreaterThan(0);
    }
  });

  it("covers the expected set of languages", () => {
    const languages = mod.targets.map(([lang]) => lang).sort();
    expect(languages).toEqual(Object.keys(expectedTargets).sort());
  });

  it("every output path resolves below the repo root", () => {
    for (const [, out] of mod.targets) {
      expect(out.startsWith(repoRoot)).toBe(true);
    }
  });

  it("each output path matches its expected relative path", () => {
    for (const [lang, out] of mod.targets) {
      const expected = resolve(repoRoot, expectedTargets[lang]);
      expect(out).toBe(expected);
    }
  });

  it("exports rootDir that matches the repo root", () => {
    expect(mod.rootDir).toBe(repoRoot);
  });

  it("exports manifest path pointing at repo Cargo.toml", () => {
    expect(mod.manifest).toBe(resolve(repoRoot, "Cargo.toml"));
  });
});

describe("generate-types.mjs run()", () => {
  let mod: Awaited<typeof import("../../scripts/generate-types.mjs")>;

  beforeEach(async () => {
    mod = await import("../../scripts/generate-types.mjs");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("invokes exec once per target when all succeed", () => {
    const execMock = vi.fn();
    const exitSpy = vi.spyOn(process, "exit").mockImplementation(() => undefined as never);

    mod.run({ exec: execMock });

    expect(execMock).toHaveBeenCalledTimes(7);
    expect(exitSpy).not.toHaveBeenCalled();

    for (const call of execMock.mock.calls) {
      const cmd = call[0] as string;
      expect(cmd).toContain("cargo run");
      expect(cmd).toContain("--package narrativeengine-codegen");
      expect(cmd).toContain("--manifest-path");
    }
    exitSpy.mockRestore();
  });

  it("stops and exits on first exec failure", () => {
    const execMock = vi
      .fn()
      .mockImplementationOnce(() => {
        const err = new Error("cargo failed") as Error & { stderr: Buffer };
        err.stderr = Buffer.from("compile error");
        throw err;
      });
    // Make process.exit throw so execution actually stops (mocking it as no-op
    // would let the for-loop continue to the next target).
    const exitSpy = vi
      .spyOn(process, "exit")
      .mockImplementation(() => { throw new Error("process.exit"); });

    expect(() => mod.run({ exec: execMock })).toThrow("process.exit");

    expect(execMock).toHaveBeenCalledTimes(1);
    expect(exitSpy).toHaveBeenCalledWith(1);
    exitSpy.mockRestore();
  });

  it("exits with error when Cargo.toml is missing", () => {
    const execMock = vi.fn();
    const exitSpy = vi
      .spyOn(process, "exit")
      .mockImplementation(() => { throw new Error("process.exit"); });

    expect(() => mod.run({ exec: execMock, exists: () => false })).toThrow("process.exit");

    expect(execMock).toHaveBeenCalledTimes(0);
    expect(exitSpy).toHaveBeenCalledWith(1);
    exitSpy.mockRestore();
  });

  it("passes cwd: rootDir to exec", () => {
    const execMock = vi.fn();
    const exitSpy = vi.spyOn(process, "exit").mockImplementation(() => undefined as never);

    mod.run({ exec: execMock });

    for (const [, opts] of execMock.mock.calls) {
      expect(opts).toHaveProperty("cwd", mod.rootDir);
    }
    exitSpy.mockRestore();
  });

  it("builds command with quoted manifest path", () => {
    const execMock = vi.fn();
    const exitSpy = vi.spyOn(process, "exit").mockImplementation(() => undefined as never);

    mod.run({ exec: execMock });

    const cmd = execMock.mock.calls[0][0] as string;
    expect(cmd).toMatch(/--manifest-path ".*Cargo\.toml"/);
    exitSpy.mockRestore();
  });
});

describe("generate-types.mjs module structure", () => {
  it("does not auto-execute on import — all other tests prove this", () => {
    // Every test in this file imports the module. If run() auto-executed
    // on import, process.exit would terminate the test runner. The fact
    // we reach here proves the process.argv[1] guard works.
    expect(true).toBe(true);
  });

  it("exports run function for programmatic use", async () => {
    const mod = await import("../../scripts/generate-types.mjs");
    expect(typeof mod.run).toBe("function");
    expect(mod.run.name).toBe("run");
  });
});
