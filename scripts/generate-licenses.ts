// Writes the third-party license list shipped in the app (requirements NFR-05).
// Rust crates come from cargo-about (configured by about.toml), npm packages from
// license-checker-rseidelsohn (production dependencies only, because dev tools are
// not bundled), and the bundled pdfium from the license files of its Linux build
// (design §8.3). Run with `npm run licenses:generate`; CI fails when the
// committed output differs from a fresh run.

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT_PATH = path.join(ROOT, "src/licenses/third-party-licenses.json");
const APP_MANIFEST = path.join(ROOT, "src-tauri/Cargo.toml");
const ABOUT_CONFIG = path.join(ROOT, "about.toml");
const PDFIUM_VERSION_FILE = path.join(ROOT, "scripts/pdfium-version.json");
// The list is generated on Linux only (CI), so it reads the Linux build that
// `npm run pdfium:fetch` unpacks. The Windows build carries the same license
// files for a given release.
const PDFIUM_DIR = path.join(ROOT, "src-tauri/pdfium/linux");
const PDFIUM_PACKAGE_LICENSE_FILE = "LICENSE";
const PDFIUM_COMPONENT_LICENSE_DIR = "licenses";
// Written by scripts/fetch-pdfium.ts: which release is unpacked.
const PDFIUM_RELEASE_MARKER_FILE = ".release";
const PDFIUM_ENTRY_PREFIX = "pdfium";
const PDFIUM_PACKAGE_NAME = "pdfium-binaries";
const LICENSE_CHECKER_BIN = path.join(
  ROOT,
  "node_modules/license-checker-rseidelsohn/bin/license-checker-rseidelsohn.js",
);
// cargo-about's JSON for the current graph is a few MB; the default 1 MB
// buffer of execFileSync would truncate it.
const MAX_TOOL_OUTPUT_BYTES = 64 * 1024 * 1024;
// For a dual-licensed npm package ("Apache-2.0 OR MIT"), the first match in
// this list is the license whose text is shipped.
const PREFERRED_LICENSES = ["MIT", "Apache-2.0", "BSD-3-Clause", "ISC"];
const LICENSE_FILE_PATTERN = /^(licen[cs]e|copying)/i;

type Ecosystem = "cargo" | "npm" | "native";

type LicensedPackage = {
  name: string;
  version: string;
  ecosystem: Ecosystem;
};

type LicenseEntry = {
  id: string;
  name: string;
  text: string;
  packages: LicensedPackage[];
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireString(record: Record<string, unknown>, key: string): string {
  const value = record[key];
  if (typeof value !== "string") {
    throw new Error(`Expected string field "${key}" in tool output`);
  }
  return value;
}

function requireArray(
  record: Record<string, unknown>,
  key: string,
): readonly unknown[] {
  const value = record[key];
  if (!Array.isArray(value)) {
    throw new Error(`Expected array field "${key}" in tool output`);
  }
  return value;
}

function normalizeText(text: string): string {
  return text.replace(/\r\n/g, "\n").trim();
}

function runTool(command: string, args: readonly string[]): unknown {
  const stdout = execFileSync(command, args, {
    cwd: ROOT,
    encoding: "utf8",
    maxBuffer: MAX_TOOL_OUTPUT_BYTES,
  });
  const parsed: unknown = JSON.parse(stdout);
  return parsed;
}

function collectCargoLicenses(names: Map<string, string>): LicenseEntry[] {
  const output = runTool("cargo", [
    "about",
    "generate",
    "--format",
    "json",
    "--config",
    ABOUT_CONFIG,
    "--manifest-path",
    APP_MANIFEST,
  ]);
  if (!isRecord(output)) {
    throw new Error("Unexpected cargo-about output");
  }

  return requireArray(output, "licenses").map((license) => {
    if (!isRecord(license)) {
      throw new Error("Unexpected license entry in cargo-about output");
    }
    const id = requireString(license, "id");
    const name = requireString(license, "name");
    names.set(id, name);
    const packages = requireArray(license, "used_by").map((usage) => {
      if (!isRecord(usage) || !isRecord(usage["crate"])) {
        throw new Error("Unexpected used_by entry in cargo-about output");
      }
      return {
        name: requireString(usage["crate"], "name"),
        version: requireString(usage["crate"], "version"),
        ecosystem: "cargo" as const,
      };
    });
    return { id, name, text: requireString(license, "text"), packages };
  });
}

function chooseLicense(expression: string): string {
  if (/\bAND\b|\bWITH\b/.test(expression)) {
    throw new Error(
      `License expression "${expression}" needs a manual decision on which texts to ship`,
    );
  }
  const options = expression
    .replace(/[()]/g, "")
    .split(/\s+OR\s+/)
    .map((option) => option.trim());
  const preferred = PREFERRED_LICENSES.find((id) => options.includes(id));
  const chosen = preferred ?? options[0];
  if (chosen === undefined || chosen.length === 0) {
    throw new Error(`Empty license expression`);
  }
  return chosen;
}

function readLicenseText(packageDir: string, licenseId: string): string {
  const candidates = readdirSync(packageDir)
    .filter((file) => LICENSE_FILE_PATTERN.test(file))
    .filter((file) => !file.toLowerCase().endsWith(".spdx"))
    .sort();
  const matching = candidates.filter((file) =>
    file.toLowerCase().includes(licenseId.toLowerCase()),
  );
  const file = matching[0] ?? (candidates.length === 1 ? candidates[0] : null);
  if (file === null || file === undefined) {
    throw new Error(
      `Cannot pick the ${licenseId} text in ${packageDir} (found: ${candidates.join(", ") || "none"})`,
    );
  }
  return readFileSync(path.join(packageDir, file), "utf8");
}

function collectNpmLicenses(names: Map<string, string>): LicenseEntry[] {
  const output = runTool(process.execPath, [
    LICENSE_CHECKER_BIN,
    "--production",
    "--excludePrivatePackages",
    "--json",
    "--start",
    ROOT,
  ]);
  if (!isRecord(output)) {
    throw new Error("Unexpected license-checker output");
  }

  return Object.entries(output).map(([key, info]) => {
    if (!isRecord(info)) {
      throw new Error(`Unexpected license-checker entry for ${key}`);
    }
    const separator = key.lastIndexOf("@");
    const name = key.slice(0, separator);
    const version = key.slice(separator + 1);
    const licenses = info["licenses"];
    const expression = Array.isArray(licenses)
      ? licenses.join(" OR ")
      : typeof licenses === "string"
        ? licenses
        : "";
    const id = chooseLicense(expression);
    return {
      id,
      name: names.get(id) ?? id,
      text: readLicenseText(requireString(info, "path"), id),
      packages: [{ name, version, ecosystem: "npm" as const }],
    };
  });
}

function readPdfiumRelease(): string {
  const parsed: unknown = JSON.parse(readFileSync(PDFIUM_VERSION_FILE, "utf8"));
  if (!isRecord(parsed)) {
    throw new Error(`${PDFIUM_VERSION_FILE} does not have the expected shape`);
  }
  return requireString(parsed, "release");
}

// Each license file of the pdfium build is its own entry: the files cover
// different components (FreeType, ICU, libjpeg-turbo, ...) and are not
// merged with the Rust and npm licenses even when a text happens to match.
function collectPdfiumLicenses(): LicenseEntry[] {
  const release = readPdfiumRelease();
  let unpacked: string;
  try {
    unpacked = readFileSync(
      path.join(PDFIUM_DIR, PDFIUM_RELEASE_MARKER_FILE),
      "utf8",
    ).trim();
  } catch {
    throw new Error(
      `pdfium ${release} is not unpacked in ${path.relative(ROOT, PDFIUM_DIR)}; run "npm run pdfium:fetch" on Linux first`,
    );
  }
  if (unpacked !== release) {
    throw new Error(
      `${path.relative(ROOT, PDFIUM_DIR)} holds ${unpacked} but ${path.relative(ROOT, PDFIUM_VERSION_FILE)} pins ${release}; run "npm run pdfium:fetch" again`,
    );
  }

  const components = readdirSync(
    path.join(PDFIUM_DIR, PDFIUM_COMPONENT_LICENSE_DIR),
  ).sort();
  const files = [
    {
      id: PDFIUM_PACKAGE_NAME,
      file: PDFIUM_PACKAGE_LICENSE_FILE,
    },
    ...components.map((component) => ({
      id: `${PDFIUM_ENTRY_PREFIX}/${component}`,
      file: path.join(PDFIUM_COMPONENT_LICENSE_DIR, component),
    })),
  ];
  return files.map(({ id, file }) => ({
    id,
    name: id,
    text: readFileSync(path.join(PDFIUM_DIR, file), "utf8"),
    packages: [{ name: id, version: release, ecosystem: "native" as const }],
  }));
}

function comparePackages(a: LicensedPackage, b: LicensedPackage): number {
  return (
    a.ecosystem.localeCompare(b.ecosystem, "en") ||
    a.name.localeCompare(b.name, "en") ||
    a.version.localeCompare(b.version, "en")
  );
}

function mergeByText(entries: readonly LicenseEntry[]): LicenseEntry[] {
  const merged = new Map<string, LicenseEntry>();
  for (const entry of entries) {
    const text = normalizeText(entry.text);
    const key = `${entry.id}\n${text}`;
    const existing = merged.get(key);
    if (existing) {
      existing.packages.push(...entry.packages);
    } else {
      merged.set(key, { ...entry, text, packages: [...entry.packages] });
    }
  }

  const result = [...merged.values()];
  for (const entry of result) {
    entry.packages.sort(comparePackages);
  }
  return result.sort((a, b) => {
    const firstA = a.packages[0];
    const firstB = b.packages[0];
    const byPackage = firstA && firstB ? comparePackages(firstA, firstB) : 0;
    return a.id.localeCompare(b.id, "en") || byPackage;
  });
}

function main(): void {
  const names = new Map<string, string>();
  const cargo = collectCargoLicenses(names);
  const npm = collectNpmLicenses(names);
  const pdfium = collectPdfiumLicenses();
  const licenses = mergeByText([...cargo, ...npm, ...pdfium]);
  writeFileSync(OUTPUT_PATH, `${JSON.stringify({ licenses }, null, 2)}\n`);
  const packageCount = licenses.reduce(
    (count, entry) => count + entry.packages.length,
    0,
  );
  console.log(
    `Wrote ${licenses.length} license texts for ${packageCount} packages to ${path.relative(ROOT, OUTPUT_PATH)}`,
  );
}

main();
