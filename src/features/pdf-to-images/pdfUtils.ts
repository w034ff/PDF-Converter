import { formatMessage, type Translations } from "../../i18n";
import type { JobItemPayload, PageSizePt, Skipped } from "../../ipc";

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
 * Whether a PDF failed as a whole: its `job-item` says `failed` and names no
 * page, as when the PDF could not be opened. Every page that was selected
 * then counts as failed, not as left unprocessed (design §6.3).
 */
export function failedAsAWhole(result: JobItemPayload | undefined): boolean {
  return (
    result !== undefined &&
    result.status === "failed" &&
    (result.failedPages ?? []).length === 0
  );
}

/** What a conversion did to a page of the single PDF the screen shows. */
export type PageOutcome = "done" | "failed" | "unprocessed";

/**
 * Tells, for each selected page, what the conversion did to it (design §6.3).
 *
 * Pages are converted from the first selected one on, so the conversion
 * reached the first `savedCount + failedPages.length` of them. Those are
 * `failed` if listed in `failedPages` and `done` otherwise; the rest were
 * never reached, by a cancel or by a stop after a write failure.
 *
 * @param selectedPages the pages that were selected, in ascending order.
 */
export function pageOutcomes(
  selectedPages: readonly number[],
  savedCount: number,
  failedPages: readonly number[],
): Map<number, PageOutcome> {
  const reached = savedCount + failedPages.length;
  const outcomes = new Map<number, PageOutcome>();
  selectedPages.forEach((page, index) => {
    if (index >= reached) {
      outcomes.set(page, "unprocessed");
    } else {
      outcomes.set(page, failedPages.includes(page) ? "failed" : "done");
    }
  });
  return outcomes;
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

/**
 * Clamps sorted intervals to the 1..pageCount range without expanding individual pages (design §4.5).
 */
export function clampIntervals(
  intervals: ReadonlyArray<readonly [number, number]>,
  pageCount: number,
): Array<[number, number]> {
  if (pageCount <= 0) {
    return [];
  }
  const result: Array<[number, number]> = [];
  for (const [start, end] of intervals) {
    if (start > pageCount || end < 1 || start > end) {
      continue;
    }
    const clampedStart = Math.max(1, start);
    const clampedEnd = Math.min(pageCount, end);
    if (clampedStart <= clampedEnd) {
      result.push([clampedStart, clampedEnd]);
    }
  }
  return result;
}

/**
 * Converts sorted non-overlapping intervals to range text (e.g. "1-3, 5, 8-10", design §4.5, §6.3).
 */
export function intervalsToRangeText(
  intervals: ReadonlyArray<readonly [number, number]>,
): string {
  return intervals
    .map(([start, end]) => (start === end ? `${start}` : `${start}-${end}`))
    .join(", ");
}

/**
 * Toggles a range of pages ([fromPage..toPage] inclusive) in intervals to included or excluded,
 * returning the new sorted, non-overlapping intervals within 1..pageCount (design §6.3).
 */
export function togglePageRangeInIntervals(
  intervals: ReadonlyArray<readonly [number, number]>,
  pageCount: number,
  fromPage: number,
  toPage: number,
  include: boolean,
): Array<[number, number]> {
  if (pageCount <= 0) {
    return [];
  }
  const clamped = clampIntervals(intervals, pageCount);
  const selected = new Uint8Array(pageCount + 1);
  for (const [s, e] of clamped) {
    for (let p = s; p <= e; p++) {
      selected[p] = 1;
    }
  }

  const minP = Math.max(1, Math.min(fromPage, toPage));
  const maxP = Math.min(pageCount, Math.max(fromPage, toPage));
  const targetVal = include ? 1 : 0;
  for (let p = minP; p <= maxP; p++) {
    selected[p] = targetVal;
  }

  const result: Array<[number, number]> = [];
  let inInterval = false;
  let start = 0;

  for (let p = 1; p <= pageCount; p++) {
    if (selected[p] === 1) {
      if (!inInterval) {
        inInterval = true;
        start = p;
      }
    } else if (inInterval) {
      result.push([start, p - 1]);
      inInterval = false;
    }
  }
  if (inInterval) {
    result.push([start, pageCount]);
  }

  return result;
}
