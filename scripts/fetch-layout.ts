// Downloads the onnxruntime build and the layout-analysis model pinned in
// layout-version.json, checks their SHA-256 and places them in
// src-tauri/onnxruntime/<os>/ and src-tauri/layout-model/ (design §8.5).
// The app itself never downloads anything; this runs only at build time.
//
// Usage: node scripts/fetch-layout.ts

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ONNXRUNTIME_RELEASES_URL =
  "https://github.com/microsoft/onnxruntime/releases";
const ONNXRUNTIME_TAG_PREFIX = "v";
const MODEL_RELEASES_URL = "https://github.com/w034ff/PDF-Converter/releases";
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const VERSION_FILE = join(ROOT, "scripts", "layout-version.json");
// Records which version or release is in place, so a matching copy is not
// fetched again. onnxruntime and the model each have their own.
const MARKER_FILE = ".release";

type Os = "linux" | "windows";

type OnnxruntimeAsset = {
  file: string;
  sha256: string;
  // Paths inside the archive.
  library: string;
  license: string;
  notices: string;
  // The name the library gets in the destination; it carries no version.
  libraryName: string;
};

type Onnxruntime = { version: string; assets: Record<Os, OnnxruntimeAsset> };

type Model = { release: string; file: string; sha256: string };

type LayoutVersion = { onnxruntime: Onnxruntime; model: Model };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isOnnxruntimeAsset(value: unknown): value is OnnxruntimeAsset {
  return (
    isRecord(value) &&
    typeof value.file === "string" &&
    typeof value.sha256 === "string" &&
    typeof value.library === "string" &&
    typeof value.license === "string" &&
    typeof value.notices === "string" &&
    typeof value.libraryName === "string"
  );
}

function isModel(value: unknown): value is Model {
  return (
    isRecord(value) &&
    typeof value.release === "string" &&
    typeof value.file === "string" &&
    typeof value.sha256 === "string"
  );
}

function parseVersion(text: string): LayoutVersion {
  const value: unknown = JSON.parse(text);
  if (
    isRecord(value) &&
    isRecord(value.onnxruntime) &&
    typeof value.onnxruntime.version === "string" &&
    isRecord(value.onnxruntime.assets) &&
    isOnnxruntimeAsset(value.onnxruntime.assets.linux) &&
    isOnnxruntimeAsset(value.onnxruntime.assets.windows) &&
    isModel(value.model)
  ) {
    return {
      onnxruntime: {
        version: value.onnxruntime.version,
        assets: {
          linux: value.onnxruntime.assets.linux,
          windows: value.onnxruntime.assets.windows,
        },
      },
      model: value.model,
    };
  }
  throw new Error(`${VERSION_FILE} does not have the expected shape`);
}

function currentOs(): Os {
  if (process.platform === "win32") return "windows";
  if (process.platform === "linux") return "linux";
  throw new Error(
    `onnxruntime is only bundled for Windows and Linux, not ${process.platform}`,
  );
}

function isInPlace(dir: string, release: string, files: string[]): boolean {
  const marker = join(dir, MARKER_FILE);
  return (
    existsSync(marker) &&
    readFileSync(marker, "utf8").trim() === release &&
    files.every((file) => existsSync(join(dir, file)))
  );
}

async function download(
  url: string,
  file: string,
  sha256: string,
): Promise<Buffer> {
  console.log(`Downloading ${url}`);
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Download failed: HTTP ${response.status}`);
  }
  const data = Buffer.from(await response.arrayBuffer());
  const digest = createHash("sha256").update(data).digest("hex");
  if (digest !== sha256) {
    throw new Error(
      `SHA-256 mismatch for ${file}: expected ${sha256}, got ${digest}`,
    );
  }
  return data;
}

async function fetchOnnxruntime(os: Os, version: Onnxruntime): Promise<void> {
  const asset = version.assets[os];
  const dest = join(ROOT, "src-tauri", "onnxruntime", os);
  const files = [
    asset.libraryName,
    basename(asset.license),
    basename(asset.notices),
  ];
  if (isInPlace(dest, version.version, files)) {
    console.log(`onnxruntime ${version.version} is already in ${dest}`);
    return;
  }

  const url = `${ONNXRUNTIME_RELEASES_URL}/download/${ONNXRUNTIME_TAG_PREFIX}${version.version}/${asset.file}`;
  const data = await download(url, asset.file, asset.sha256);

  const work = mkdtempSync(join(tmpdir(), "onnxruntime-"));
  try {
    const archive = join(work, asset.file);
    writeFileSync(archive, data);
    const unpacked = join(work, "unpacked");
    mkdirSync(unpacked);
    // Only the files the app ships are extracted (the GPU provider libraries
    // in the archive are not used). tar detects tgz and zip by itself, on
    // Linux (GNU tar) and on Windows (bsdtar).
    const result = spawnSync(
      "tar",
      [
        "-xf",
        archive,
        "-C",
        unpacked,
        asset.library,
        asset.license,
        asset.notices,
      ],
      { stdio: "inherit" },
    );
    if (result.status !== 0) {
      throw new Error(`tar exited with ${String(result.status)}`);
    }
    for (const member of [asset.library, asset.license, asset.notices]) {
      if (!existsSync(join(unpacked, member))) {
        throw new Error(`${member} is missing from ${asset.file}`);
      }
    }
    rmSync(dest, { recursive: true, force: true });
    mkdirSync(dest, { recursive: true });
    copyFileSync(join(unpacked, asset.library), join(dest, asset.libraryName));
    copyFileSync(
      join(unpacked, asset.license),
      join(dest, basename(asset.license)),
    );
    copyFileSync(
      join(unpacked, asset.notices),
      join(dest, basename(asset.notices)),
    );
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  writeFileSync(join(dest, MARKER_FILE), `${version.version}\n`);
  console.log(`onnxruntime ${version.version} placed in ${dest}`);
}

async function fetchModel(model: Model): Promise<void> {
  const dest = join(ROOT, "src-tauri", "layout-model");
  if (isInPlace(dest, model.release, [model.file])) {
    console.log(`Layout model ${model.release} is already in ${dest}`);
    return;
  }

  const url = `${MODEL_RELEASES_URL}/download/${model.release}/${model.file}`;
  const data = await download(url, model.file, model.sha256);
  rmSync(dest, { recursive: true, force: true });
  mkdirSync(dest, { recursive: true });
  writeFileSync(join(dest, model.file), data);
  writeFileSync(join(dest, MARKER_FILE), `${model.release}\n`);
  console.log(`Layout model ${model.release} placed in ${dest}`);
}

async function main(): Promise<void> {
  const version = parseVersion(readFileSync(VERSION_FILE, "utf8"));
  await fetchOnnxruntime(currentOs(), version.onnxruntime);
  await fetchModel(version.model);
}

await main();
