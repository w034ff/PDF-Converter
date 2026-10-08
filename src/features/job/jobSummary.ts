import { formatMessage, type Translations } from "../../i18n";
import type { JobFinishedPayload, JobItemPayload } from "../../ipc";

/**
 * What the summary counts. Most conversions count items. A conversion of one
 * PDF counts its pages, since "1 item" says little of 200 pages, and
 * "Single PDF" counts the pages of the PDF it wrote.
 */
export type JobSummaryShape =
  | {
      kind: "items";
      /**
       * `job-finished` counts a PDF that failed on some pages as failed,
       * though its other pages were saved. This many of those are listed on
       * their own (from `job-item`).
       */
      partial: number;
    }
  | { kind: "pages"; saved: number; failed: number; unprocessed: number }
  | { kind: "mergedPdf"; pages: number; failedImages: number };

const ITEMS_SHAPE: JobSummaryShape = { kind: "items", partial: 0 };

/** `count` with the template for one or for several. */
function pluralize(count: number, single: string, multiple: string): string {
  return formatMessage(count === 1 ? single : multiple, { count });
}

/** The parts of the summary, none of them for a count of zero. */
function summaryParts(
  t: Translations,
  finished: JobFinishedPayload,
  shape: JobSummaryShape,
): string[] {
  switch (shape.kind) {
    case "items": {
      const counts: [number, string][] = [
        [finished.succeeded, t.job.counts.succeeded],
        [finished.failed - shape.partial, t.job.counts.failed],
        [shape.partial, t.job.counts.partial],
        [finished.noPages, t.job.counts.noPages],
        [finished.unprocessed, t.job.counts.unprocessed],
      ];
      return counts
        .filter(([count]) => count > 0)
        .map(([count, template]) => formatMessage(template, { count }));
    }
    case "pages": {
      const { pageCounts } = t.job;
      const counts: [number, string, string][] = [
        [shape.saved, pageCounts.savedSingle, pageCounts.savedMultiple],
        [shape.failed, pageCounts.failedSingle, pageCounts.failedMultiple],
        [
          shape.unprocessed,
          pageCounts.unprocessedSingle,
          pageCounts.unprocessedMultiple,
        ],
      ];
      return counts
        .filter(([count]) => count > 0)
        .map(([count, single, multiple]) => pluralize(count, single, multiple));
    }
    case "mergedPdf": {
      const parts = [
        formatMessage(t.job.mergedPdf.saved, { count: shape.pages }),
      ];
      if (shape.failedImages > 0) {
        parts.push(
          formatMessage(t.job.mergedPdf.failedImages, {
            count: shape.failedImages,
          }),
        );
      }
      return parts;
    }
  }
}

/**
 * The line shown when a conversion ends, such as
 * "変換が終わりました：成功 3 件 · 失敗 2 件" (mockup `PdfBatch`).
 * A count of zero is not listed.
 */
export function formatJobSummary(
  t: Translations,
  finished: JobFinishedPayload,
  shape: JobSummaryShape = ITEMS_SHAPE,
): string {
  const template = finished.cancelled
    ? t.job.summaryCancelled
    : t.job.summaryFinished;
  return formatMessage(template, {
    counts: summaryParts(t, finished, shape).join(t.job.summarySeparator),
  });
}

/**
 * The page counts of a conversion of one PDF: saved and failed from its
 * `job-item`, and the selected pages that neither reached (design §6.3).
 *
 * @param result the PDF's `job-item`, if it has sent one.
 * @param selectedPages how many pages the range selected in that PDF.
 */
export function singlePdfPageCounts(
  result: JobItemPayload | undefined,
  selectedPages: number,
): Extract<JobSummaryShape, { kind: "pages" }> {
  const saved = result?.outputs.length ?? 0;
  const failed = result?.failedPages?.length ?? 0;
  return {
    kind: "pages",
    saved,
    failed,
    unprocessed: Math.max(0, selectedPages - saved - failed),
  };
}

/**
 * How a finished conversion went, for the colour and mark of its summary:
 * `failure` if any item failed (a partial failure included), `success` if
 * every item converted, and `neutral` for a cancel or a run that converted
 * nothing without failing (no pages in range, items left unprocessed).
 */
export type JobOutcome = "success" | "failure" | "neutral";

export function jobOutcome(finished: JobFinishedPayload): JobOutcome {
  if (finished.failed > 0) {
    return "failure";
  }
  if (finished.cancelled) {
    return "neutral";
  }
  if (finished.noPages > 0 || finished.unprocessed > 0) {
    return "neutral";
  }
  return finished.succeeded > 0 ? "success" : "neutral";
}
