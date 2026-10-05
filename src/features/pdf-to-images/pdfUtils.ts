import { formatMessage, type Translations } from "../../i18n";
import type { PageSizePt, Skipped } from "../../ipc";

const A4_SHORT_PT = 595.28;
const A4_LONG_PT = 841.89;
const LETTER_SHORT_PT = 612;
const LETTER_LONG_PT = 792;
const PT_TOLERANCE = 2.0;

const BYTES_PER_KB = 1024;
const BYTES_PER_MB = 1024 * 1024;
const POINTS_PER_INCH = 72;

/**
 * Returns human-readable paper size name or point dimensions (design §4.4, §6.3).
 */
export function formatPaperSize(sizePt: PageSizePt | null): string {
  if (sizePt === null) {
    return "";
  }
  const shortSide = Math.min(sizePt.widthPt, sizePt.heightPt);
  const longSide = Math.max(sizePt.widthPt, sizePt.heightPt);

  if (
    Math.abs(shortSide - A4_SHORT_PT) < PT_TOLERANCE &&
    Math.abs(longSide - A4_LONG_PT) < PT_TOLERANCE
  ) {
    return "A4";
  }
  if (
    Math.abs(shortSide - LETTER_SHORT_PT) < PT_TOLERANCE &&
    Math.abs(longSide - LETTER_LONG_PT) < PT_TOLERANCE
  ) {
    return "Letter";
  }
  return `${Math.round(sizePt.widthPt)} × ${Math.round(sizePt.heightPt)} pt`;
}

/**
 * Calculates rendered pixel dimensions: round(pt ÷ 72 × dpi) (design §4.4).
 */
export function calculateRenderDimensions(
  sizePt: PageSizePt,
  dpi: number,
): { width: number; height: number } {
  return {
    width: Math.round((sizePt.widthPt / POINTS_PER_INCH) * dpi),
    height: Math.round((sizePt.heightPt / POINTS_PER_INCH) * dpi),
  };
}

/**
 * Formats a file size in bytes to a human-readable string (B, KB, MB).
 */
export function formatFileSize(bytes: number): string {
  if (bytes < BYTES_PER_KB) {
    return `${bytes} B`;
  }
  if (bytes < BYTES_PER_MB) {
    return `${(bytes / BYTES_PER_KB).toFixed(1)} KB`;
  }
  return `${(bytes / BYTES_PER_MB).toFixed(1)} MB`;
}

/**
 * Counts how many pages within 1..pageCount fall into the normalized intervals (design §4.5).
 */
export function countPagesInIntervals(
  intervals: Array<[number, number]>,
  pageCount: number,
): number {
  if (pageCount <= 0) {
    return 0;
  }
  let total = 0;
  for (const [start, end] of intervals) {
    if (start > pageCount) {
      break;
    }
    const cappedEnd = Math.min(end, pageCount);
    if (cappedEnd >= start) {
      total += cappedEnd - start + 1;
    }
  }
  return total;
}

/**
 * Checks whether 1-based page number falls into any of the sorted disjoint intervals (design §4.5, §6.3).
 */
export function isPageInIntervals(
  intervals: Array<[number, number]>,
  page: number,
): boolean {
  return intervals.some(([start, end]) => page >= start && page <= end);
}

/**
 * Formats the summary text for skipped items if any counts are non-zero (design §6.1, FR-01).
 */
export function formatSkippedSummary(
  t: Translations,
  skipped: Skipped | null,
): string | null {
  if (skipped === null) {
    return null;
  }
  const parts: string[] = [];
  if (skipped.unsupported > 0) {
    parts.push(
      formatMessage(t.pdfToImages.batch.skippedUnsupported, {
        count: skipped.unsupported,
      }),
    );
  }
  if (skipped.folders > 0) {
    parts.push(
      formatMessage(t.pdfToImages.batch.skippedFolders, {
        count: skipped.folders,
      }),
    );
  }
  if (skipped.duplicates > 0) {
    parts.push(
      formatMessage(t.pdfToImages.batch.skippedDuplicates, {
        count: skipped.duplicates,
      }),
    );
  }
  if (parts.length === 0) {
    return null;
  }
  return formatMessage(t.pdfToImages.batch.skippedSummary, {
    details: parts.join(t.pdfToImages.batch.skippedSeparator),
  });
}
