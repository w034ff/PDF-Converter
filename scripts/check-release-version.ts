// Checks that a release tag names the version the app is built with
// (work-plan T15). The version is written in three files that Tauri, npm and
// cargo each read; an installer built from a mismatch would call itself one
// version in its file name and another in "About".
//
// Usage: node scripts/check-release-version.ts <tag>
// The tag is `v` and the version, optionally with a pre-release suffix
// (`v0.1.0-rc.1` checks 0.1.0).

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TAG_PATTERN = /^v(\d+\.\d+\.\d+)(?:-[0-9A-Za-z.-]+)?$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function jsonVersion(file: string): string {
  const parsed: unknown = JSON.parse(readFileSync(join(ROOT, file), "utf8"));
  if (!isRecord(parsed) || typeof parsed.version !== "string") {
    throw new Error(`${file} has no "version" string`);
  }
  return parsed.version;
}

function cargoPackageVersion(file: string): string {
  const toml = readFileSync(join(ROOT, file), "utf8");
  // The first `version` after `[package]` and before the next table.
  const match = /^\[package\][^[]*?^version\s*=\s*"([^"]+)"/m.exec(toml);
  if (match === null) {
    throw new Error(`${file} has no version in [package]`);
  }
  return match[1];
}

function main(): void {
  const tag = process.argv[2];
  if (tag === undefined) {
    throw new Error("usage: node scripts/check-release-version.ts <tag>");
  }
  const tagMatch = TAG_PATTERN.exec(tag);
  if (tagMatch === null) {
    throw new Error(`tag "${tag}" is not v<major>.<minor>.<patch>[-suffix]`);
  }
  const expected = tagMatch[1];

  const versions: [string, string][] = [
    ["package.json", jsonVersion("package.json")],
    ["src-tauri/tauri.conf.json", jsonVersion("src-tauri/tauri.conf.json")],
    ["src-tauri/Cargo.toml", cargoPackageVersion("src-tauri/Cargo.toml")],
  ];
  const mismatches = versions.filter(([, version]) => version !== expected);
  for (const [file, version] of versions) {
    console.log(`${file}: ${version}`);
  }
  if (mismatches.length > 0) {
    throw new Error(
      `tag ${tag} expects ${expected}; differs in ${mismatches
        .map(([file]) => file)
        .join(", ")}`,
    );
  }
  console.log(`tag ${tag} matches ${expected}`);
}

try {
  main();
} catch (error: unknown) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
