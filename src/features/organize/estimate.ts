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
  const used = new Map<number, number>();
  for (const page of pages) {
    used.set(page.id, (used.get(page.id) ?? 0) + 1);
  }
  let total = 0n;
  for (const pdf of pdfs) {
    const usedPages = used.get(pdf.id) ?? 0;
    if (pdf.pageCount === 0 || usedPages === 0) {
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
