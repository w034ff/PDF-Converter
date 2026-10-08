// Generates files used for manual acceptance testing (docs/work-plan.md T15b, T16):
// 1. too-many-pixels.png exceeding MAX_IMAGE_PIXELS (crates/core/src/probe.rs)
// 2. too-many-pages.pdf exceeding MAX_PDF_PAGES (crates/worker/src/lib.rs)
// 3. many-pages.pdf with 200 pages of vector curves that take long enough to cancel mid-flight at 300 dpi
//
// Usage: npm run manual-test:files -- <output_dir>

import {
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { crc32, deflateSync } from "node:zlib";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const PROBE_RS = join(ROOT, "crates", "core", "src", "probe.rs");
const WORKER_LIB_RS = join(ROOT, "crates", "worker", "src", "lib.rs");

const PNG_WIDTH = 10_000;
const MANY_PAGES_COUNT = 200;
const SHAPES_PER_PAGE = 400;

function readConstant(filePath: string, constantName: string): number {
  if (!existsSync(filePath)) {
    throw new Error(`File not found: ${filePath}`);
  }
  const content = readFileSync(filePath, "utf-8");
  const pattern = new RegExp(
    `pub\\s+const\\s+${constantName}\\s*:\\s*\\w+\\s*=\\s*([0-9_]+)\\s*;`,
  );
  const match = content.match(pattern);
  if (!match || !match[1]) {
    throw new Error(
      `Constant '${constantName}' could not be read from ${filePath}`,
    );
  }
  const parsed = Number.parseInt(match[1].replaceAll("_", ""), 10);
  if (Number.isNaN(parsed) || parsed <= 0) {
    throw new Error(
      `Constant '${constantName}' in ${filePath} parsed to invalid value: ${match[1]}`,
    );
  }
  return parsed;
}

function formatWithCommas(value: number): string {
  return value.toLocaleString("en-US");
}

function createPngChunk(type: string, data: Buffer): Buffer {
  const typeBuf = Buffer.from(type, "ascii");
  const lengthBuf = Buffer.alloc(4);
  lengthBuf.writeUInt32BE(data.length, 0);

  const crcTarget = Buffer.concat([typeBuf, data]);
  const chunkCrc = crc32(crcTarget);
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(chunkCrc, 0);

  return Buffer.concat([lengthBuf, typeBuf, data, crcBuf]);
}

function createTooManyPixelsPng(maxImagePixels: number): Buffer {
  const width = PNG_WIDTH;
  const height = Math.floor(maxImagePixels / width) + 1;

  const signature = Buffer.from([
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
  ]);

  // IHDR: 13 bytes (width, height, bit depth 8, color type 0: grayscale)
  const ihdrData = Buffer.alloc(13);
  ihdrData.writeUInt32BE(width, 0);
  ihdrData.writeUInt32BE(height, 4);
  ihdrData[8] = 8; // 8-bit depth
  ihdrData[9] = 0; // Grayscale
  ihdrData[10] = 0; // Deflate compression
  ihdrData[11] = 0; // Standard filter
  ihdrData[12] = 0; // Non-interlaced
  const ihdrChunk = createPngChunk("IHDR", ihdrData);

  // IDAT: (1 + width) * height bytes of zeroes (filter byte 0 + black pixels)
  const uncompressed = Buffer.alloc((1 + width) * height, 0);
  const deflated = deflateSync(uncompressed);
  const idatChunk = createPngChunk("IDAT", deflated);

  // IEND
  const iendChunk = createPngChunk("IEND", Buffer.alloc(0));

  return Buffer.concat([signature, ihdrChunk, idatChunk, iendChunk]);
}

function createTooManyPagesPdf(pageCount: number): Buffer {
  let pdf = "%PDF-1.4\n%\xe2\xe3\xcf\xd3\n";
  const offsets: number[] = [0];

  // Object 1: Catalog
  offsets.push(pdf.length);
  pdf += "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";

  // Object 2: Pages root
  offsets.push(pdf.length);
  pdf += `2 0 obj\n<< /Type /Pages /Count ${pageCount} /MediaBox [0 0 595.28 841.89] /Kids [\n`;
  for (let i = 0; i < pageCount; i++) {
    pdf += `${3 + i} 0 R `;
    if ((i + 1) % 10 === 0) {
      pdf += "\n";
    }
  }
  pdf += "\n] >>\nendobj\n";

  // Objects 3 .. 3 + pageCount - 1: Page objects
  for (let i = 0; i < pageCount; i++) {
    offsets.push(pdf.length);
    pdf += `${3 + i} 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n`;
  }

  const startxref = pdf.length;
  const totalObjects = pageCount + 3;
  pdf += `xref\n0 ${totalObjects}\n`;
  pdf += "0000000000 65535 f \n";
  for (let i = 1; i < offsets.length; i++) {
    const offset = offsets[i];
    if (offset === undefined) {
      throw new Error(`Missing offset for object ${i}`);
    }
    pdf += `${String(offset).padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${totalObjects} /Root 1 0 R >>\n`;
  pdf += `startxref\n${startxref}\n%%EOF\n`;

  return Buffer.from(pdf, "latin1");
}

function createManyPagesPdf(pageCount: number): Buffer {
  let pdf = "%PDF-1.4\n%\xe2\xe3\xcf\xd3\n";
  const offsets: number[] = [0];

  // Object 1: Catalog
  offsets.push(pdf.length);
  pdf += "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";

  // Generate vector drawing operators (A4: 595.28 x 841.89 pt)
  // Generates complex Bézier curves with stroking and filling to ensure
  // rendering at 300 dpi takes long enough to cancel mid-flight.
  let streamContent = "0.5 w\n";
  for (let s = 0; s < SHAPES_PER_PAGE; s++) {
    const r = (((s * 37) % 255) / 255).toFixed(2);
    const g = (((s * 73) % 255) / 255).toFixed(2);
    const b = (((s * 111) % 255) / 255).toFixed(2);
    const x = (s * 17) % 550;
    const y = (s * 23) % 800;
    const x2 = (x + 30) % 550;
    const y2 = (y + 50) % 800;
    const x3 = (x + 80) % 550;
    const y3 = (y - 30 + 800) % 800;
    streamContent += `${r} ${g} ${b} rg ${r} ${g} ${b} RG\n`;
    streamContent += `${x} ${y} m ${x2} ${y} ${x2} ${y2} ${x3} ${y3} c b\n`;
  }

  // Object 2: Pages root
  offsets.push(pdf.length);
  pdf += `2 0 obj\n<< /Type /Pages /Count ${pageCount} /MediaBox [0 0 595.28 841.89] /Kids [\n`;
  for (let i = 0; i < pageCount; i++) {
    pdf += `${4 + i} 0 R `;
    if ((i + 1) % 10 === 0) {
      pdf += "\n";
    }
  }
  pdf += "\n] >>\nendobj\n";

  // Object 3: Shared content stream with vector shapes
  offsets.push(pdf.length);
  const streamBytes = Buffer.byteLength(streamContent, "utf8");
  pdf += `3 0 obj\n<< /Length ${streamBytes} >>\nstream\n${streamContent}endstream\nendobj\n`;

  // Objects 4 .. 4 + pageCount - 1: Page objects
  for (let i = 0; i < pageCount; i++) {
    offsets.push(pdf.length);
    pdf += `${4 + i} 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 3 0 R >>\nendobj\n`;
  }

  const startxref = pdf.length;
  const totalObjects = pageCount + 4;
  pdf += `xref\n0 ${totalObjects}\n`;
  pdf += "0000000000 65535 f \n";
  for (let i = 1; i < offsets.length; i++) {
    const offset = offsets[i];
    if (offset === undefined) {
      throw new Error(`Missing offset for object ${i}`);
    }
    pdf += `${String(offset).padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${totalObjects} /Root 1 0 R >>\n`;
  pdf += `startxref\n${startxref}\n%%EOF\n`;

  return Buffer.from(pdf, "latin1");
}

function main(): void {
  const targetArg = process.argv[2];
  if (!targetArg) {
    console.error("Usage: npm run manual-test:files -- <output_dir>");
    process.exit(1);
  }

  const outputDir = resolve(process.cwd(), targetArg);
  if (!existsSync(outputDir)) {
    mkdirSync(outputDir, { recursive: true });
  }

  const maxImagePixels = readConstant(PROBE_RS, "MAX_IMAGE_PIXELS");
  const maxPdfPages = readConstant(WORKER_LIB_RS, "MAX_PDF_PAGES");

  // 1. too-many-pixels.png
  const pngPath = join(outputDir, "too-many-pixels.png");
  const pngBuffer = createTooManyPixelsPng(maxImagePixels);
  writeFileSync(pngPath, pngBuffer);
  const pngHeight = Math.floor(maxImagePixels / PNG_WIDTH) + 1;
  const pngPixels = PNG_WIDTH * pngHeight;

  // 2. too-many-pages.pdf
  const tooManyPagesPath = join(outputDir, "too-many-pages.pdf");
  const tooManyPagesCount = maxPdfPages + 1;
  const tooManyPagesBuffer = createTooManyPagesPdf(tooManyPagesCount);
  writeFileSync(tooManyPagesPath, tooManyPagesBuffer);

  // 3. many-pages.pdf
  const manyPagesPath = join(outputDir, "many-pages.pdf");
  const manyPagesBuffer = createManyPagesPdf(MANY_PAGES_COUNT);
  writeFileSync(manyPagesPath, manyPagesBuffer);

  const pngStat = statSync(pngPath);
  const tooManyPagesStat = statSync(tooManyPagesPath);
  const manyPagesStat = statSync(manyPagesPath);

  console.log(
    `too-many-pixels.png: ${formatWithCommas(pngPixels)} pixels (${PNG_WIDTH}x${pngHeight}), ${formatWithCommas(pngStat.size)} bytes`,
  );
  console.log(
    `too-many-pages.pdf: ${formatWithCommas(tooManyPagesCount)} pages, ${formatWithCommas(tooManyPagesStat.size)} bytes`,
  );
  console.log(
    `many-pages.pdf: ${formatWithCommas(MANY_PAGES_COUNT)} pages, ${formatWithCommas(manyPagesStat.size)} bytes`,
  );
}

main();
