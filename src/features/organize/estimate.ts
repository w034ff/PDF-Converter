import type { OrganizePdf } from "../../ipc";
import type { OrganizePage } from "../../state";

const BYTES_PER_MB = 1024n * 1024n;

/**
 * The estimated size of the output made of `pages`, in bytes: for each PDF
 * in it, `ceil(fileBytes × used pages ÷ pageCount)`, added up (design
 * organize §2). It is `estimate_output_bytes` of `src-tauri/src/organize.rs`
 * made from the same `fileBytes`, so the figure on the screen and the refusal
 * of Rust (`OutputTooLarge`) cannot disagree. BigInt, because the product of
 * a file size and a page count can pass `Number.MAX_SAFE_INTEGER`.
 */
export function estimateOutputBytes(
  pages: readonly OrganizePage[],
  pdfs: readonly OrganizePdf[],
): bigint {
  return estimateWith(new Map(pdfs.map((pdf) => [pdf.id, pdf])), pages);
}

/**
 * The estimated sizes of the outputs of the save in parts added up. Each
 * output is estimated on its own (design organize §2), so the sum can differ
 * from the estimate of all the pages as one output: a PDF that has pages in
 * two outputs is counted once in each, rounded up in each.
 */
export function estimatePartsBytes(
  parts: readonly (readonly OrganizePage[])[],
  pdfs: readonly OrganizePdf[],
): bigint {
  const pdfById = new Map(pdfs.map((pdf) => [pdf.id, pdf]));
  return parts.reduce((sum, part) => sum + estimateWith(pdfById, part), 0n);
}

function estimateWith(
  pdfById: ReadonlyMap<number, OrganizePdf>,
  pages: readonly OrganizePage[],
): bigint {
  const used = new Map<number, number>();
  for (const page of pages) {
    used.set(page.id, (used.get(page.id) ?? 0) + 1);
  }
  let total = 0n;
  for (const [id, usedPages] of used) {
    const pdf = pdfById.get(id);
    if (pdf === undefined || pdf.pageCount === 0) {
      continue;
    }
    const divisor = BigInt(pdf.pageCount);
    total +=
      (BigInt(pdf.fileBytes) * BigInt(usedPages) + divisor - 1n) / divisor;
  }
  return total;
}

/** `bytes` in MB of 1,048,576 bytes, rounded up: an output just over a whole MB reads as the next. */
export function bytesToMegabytes(bytes: bigint): number {
  return Number((bytes + BYTES_PER_MB - 1n) / BYTES_PER_MB);
}
