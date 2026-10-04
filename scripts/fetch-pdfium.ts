// Downloads the pdfium build pinned in pdfium-version.json for this OS,
// checks its SHA-256 and unpacks it into src-tauri/pdfium/<os>/ (design §8.1).
// The app itself never downloads anything; this runs only at build time.
//
// Usage: node scripts/fetch-pdfium.ts

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASES_URL = "https://github.com/bblanchon/pdfium-binaries/releases";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const VERSION_FILE = join(ROOT, "scripts", "pdfium-version.json");
// Records which release is unpacked, so a matching copy is not fetched again.
const MARKER_FILE = ".release";

type Os = "linux" | "windows";

type Asset = { file: string; sha256: string; library: string };

type PdfiumVersion = { release: string; assets: Record<Os, Asset> };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isAsset(value: unknown): value is Asset {
  return (
    isRecord(value) &&
    typeof value.file === "string" &&
    typeof value.sha256 === "string" &&
    typeof value.library === "string"
  );
}

function parseVersion(text: string): PdfiumVersion {
  const value: unknown = JSON.parse(text);
  if (
    isRecord(value) &&
    typeof value.release === "string" &&
    isRecord(value.assets) &&
    isAsset(value.assets.linux) &&
    isAsset(value.assets.windows)
  ) {
    return {
      release: value.release,
      assets: { linux: value.assets.linux, windows: value.assets.windows },
    };
  }
  throw new Error(`${VERSION_FILE} does not have the expected shape`);
}

function currentOs(): Os {
  if (process.platform === "win32") return "windows";
  if (process.platform === "linux") return "linux";
  throw new Error(
    `pdfium is only bundled for Windows and Linux, not ${process.platform}`,
  );
}

async function main(): Promise<void> {
  const version = parseVersion(readFileSync(VERSION_FILE, "utf8"));
  const os = currentOs();
  const asset = version.assets[os];
  const dest = join(ROOT, "src-tauri", "pdfium", os);
  const marker = join(dest, MARKER_FILE);

  if (
    existsSync(marker) &&
    readFileSync(marker, "utf8").trim() === version.release &&
    existsSync(join(dest, asset.library))
  ) {
    console.log(`pdfium ${version.release} is already in ${dest}`);
    return;
  }

  const url = `${RELEASES_URL}/download/${version.release}/${asset.file}`;
  console.log(`Downloading ${url}`);
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Download failed: HTTP ${response.status}`);
  }
  const data = Buffer.from(await response.arrayBuffer());
  const digest = createHash("sha256").update(data).digest("hex");
  if (digest !== asset.sha256) {
    throw new Error(
      `SHA-256 mismatch for ${asset.file}: expected ${asset.sha256}, got ${digest}`,
    );
  }

  const work = mkdtempSync(join(tmpdir(), "pdfium-"));
  try {
    const archive = join(work, asset.file);
    writeFileSync(archive, data);
    rmSync(dest, { recursive: true, force: true });
    mkdirSync(dest, { recursive: true });
    // Both Windows (bsdtar) and Linux ship a tar that reads gzip archives.
    const result = spawnSync("tar", ["-xzf", archive, "-C", dest], {
      stdio: "inherit",
    });
    if (result.status !== 0) {
      throw new Error(`tar exited with ${String(result.status)}`);
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  if (!existsSync(join(dest, asset.library))) {
    throw new Error(`${asset.library} is missing from ${asset.file}`);
  }
  writeFileSync(marker, `${version.release}\n`);
  console.log(`pdfium ${version.release} unpacked into ${dest}`);
}

await main();
