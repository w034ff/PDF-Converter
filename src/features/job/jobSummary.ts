import {
  formatErrorMessage,
  formatMessage,
  type Language,
  type Translations,
} from "../../i18n";
import type { JobFinishedPayload, JobItemPayload } from "../../ipc";
import type { ActiveTab, AppState, JobState } from "../../state";
import {
  countPagesInIntervals,
  failedAsAWhole,
  selectedIntervals,
} from "../pdf-to-images/pdfUtils";

/**
 * What the summary counts. Most conversions count items. A conversion of one
 * PDF counts its pages, since "1 item" says little of 200 pages, and
 * "Single PDF" counts the pages of the PDF it wrote. A cancelled "Single PDF"
 * wrote none, so it counts nothing and says so.
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
      /** The organize tab's save in parts has no rows, so the first output that failed is told here. */
      firstFailure?: PartFailure;
    }
  | { kind: "pages"; saved: number; failed: number; unprocessed: number }
  | { kind: "mergedPdf"; pages: number; failedImages: number }
  | { kind: "mergedPdfNotSaved" }
  /**
   * The organize tab's "PDF を保存": the pages of the PDF it wrote, and the
   * PDF of the list it replaced, if it did.
   */
  | { kind: "organizedPdf"; pages: number; replacedName: string | null }
  /** It wrote none: the PDF it failed on (`null` if it was not one of them) and why. */
  | { kind: "organizedPdfFailed"; name: string | null; message: string }
  /** The organize tab's save in parts, when every output was written. */
  | { kind: "organizedParts"; saved: number }
  /** The crop tab's "全ページの候補": the areas it added, and the pages by outcome. */
  | { kind: "detectAll"; added: number; failed: number; unprocessed: number };

/** The output of the save in parts that failed first: its number from 1, the PDF it failed on if one, and why. */
export interface PartFailure {
  part: number;
  name: string | null;
  message: string;
}

const ITEMS_SHAPE: JobSummaryShape = { kind: "items", partial: 0 };

/** `count` with the template for one or for several. */
export function pluralize(
  count: number,
  single: string,
  multiple: string,
): string {
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
    case "mergedPdfNotSaved":
      return [t.job.mergedPdf.notSaved];
    case "organizedPdf": {
      const { result } = t.organize;
      const saved = pluralize(
        shape.pages,
        result.savedSingle,
        result.savedMultiple,
      );
      return shape.replacedName === null
        ? [saved]
        : [saved, formatMessage(result.replaced, { name: shape.replacedName })];
    }
    case "organizedParts":
      return [
        pluralize(
          shape.saved,
          t.organize.result.savedPartsSingle,
          t.organize.result.savedPartsMultiple,
        ),
      ];
    case "organizedPdfFailed": {
      const { result } = t.organize;
      return [
        shape.name === null
          ? formatMessage(result.failedWithoutName, {
              message: shape.message,
            })
          : formatMessage(result.failed, {
              name: shape.name,
              message: shape.message,
            }),
      ];
    }
    case "detectAll": {
      const { detectAll } = t.crop;
      const { pageCounts } = t.job;
      const parts =
        shape.added > 0
          ? [
              pluralize(
                shape.added,
                detectAll.addedSingle,
                detectAll.addedMultiple,
              ),
            ]
          : [detectAll.noneAdded];
      if (shape.failed > 0) {
        parts.push(
          pluralize(
            shape.failed,
            pageCounts.failedSingle,
            pageCounts.failedMultiple,
          ),
        );
      }
      if (shape.unprocessed > 0) {
        parts.push(
          pluralize(
            shape.unprocessed,
            pageCounts.unprocessedSingle,
            pageCounts.unprocessedMultiple,
          ),
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
  if (shape.kind === "organizedPdf") {
    // The save is one PDF, so there is no count of conversions to lead with.
    return summaryParts(t, finished, shape).join(t.job.summarySeparator);
  }
  if (shape.kind === "organizedPdfFailed" || shape.kind === "organizedParts") {
    return summaryParts(t, finished, shape).join("");
  }
  const finishedTemplate =
    shape.kind === "detectAll"
      ? t.crop.detectAll.summaryFinished
      : t.job.summaryFinished;
  const template = finished.cancelled
    ? t.job.summaryCancelled
    : finishedTemplate;
  const text = formatMessage(template, {
    counts: summaryParts(t, finished, shape).join(t.job.summarySeparator),
  });
  return shape.kind === "items" && shape.firstFailure !== undefined
    ? text + formatPartFailure(t, shape.firstFailure)
    : text;
}

function formatPartFailure(t: Translations, failure: PartFailure): string {
  const { result } = t.organize;
  return failure.name === null
    ? formatMessage(result.failedPartWithoutName, {
        part: failure.part,
        message: failure.message,
      })
    : formatMessage(result.failedPart, {
        part: failure.part,
        name: failure.name,
        message: failure.message,
      });
}

/**
 * The page counts of a conversion of one PDF: saved and failed from its
 * `job-item`, and the selected pages that neither reached (design §6.3).
 * A PDF that failed as a whole (see `failedAsAWhole`) has all of them failed.
 *
 * @param result the PDF's `job-item`, if it has sent one.
 * @param selectedPages how many pages the range selected in that PDF.
 */
export function singlePdfPageCounts(
  result: JobItemPayload | undefined,
  selectedPages: number,
): Extract<JobSummaryShape, { kind: "pages" }> {
  const saved = result?.outputs.length ?? 0;
  const failed = failedAsAWhole(result)
    ? selectedPages
    : (result?.failedPages?.length ?? 0);
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

/** How many pages the range selected in the PDF with `pageCount` pages. */
function selectedPageCount(
  pdfToImages: AppState["pdfToImages"],
  pageCount: number,
): number | null {
  if (pdfToImages.pageSelection === "all") {
    return pageCount;
  }
  return pdfToImages.rangeResult === null
    ? null
    : countPagesInIntervals(
        selectedIntervals(pdfToImages.rangeResult, pageCount),
        pageCount,
      );
}

/**
 * What the summary of the screen's last conversion counts: pages when one
 * PDF was converted, the pages of the PDF written by "Single PDF" (nothing,
 * if that was cancelled, since no PDF exists), items otherwise.
 */
export function summaryShape(
  tab: ActiveTab,
  job: JobState,
  imagesToPdf: AppState["imagesToPdf"],
  pdfToImages: AppState["pdfToImages"],
  crop: AppState["crop"],
  partialCount: number,
): JobSummaryShape {
  if (tab === "crop" && crop.lastJob === "detectAll" && job.finished !== null) {
    return {
      kind: "detectAll",
      added: crop.detectAdded,
      failed: job.finished.failed,
      unprocessed: job.finished.unprocessed,
    };
  }
  if (tab === "pdfToImages" && job.targets.length === 1) {
    const [target] = job.targets;
    const item = pdfToImages.items.find((pdf) => pdf.id === target.id);
    const selected =
      item === undefined
        ? null
        : selectedPageCount(pdfToImages, item.pageCount);
    if (selected !== null) {
      return singlePdfPageCounts(job.results[target.id], selected);
    }
  }
  if (
    tab === "imagesToPdf" &&
    imagesToPdf.output === "merge" &&
    job.finished?.cancelled === true
  ) {
    return { kind: "mergedPdfNotSaved" };
  }
  if (
    tab === "imagesToPdf" &&
    job.savedName !== null &&
    job.finished !== null &&
    job.finished.succeeded > 0
  ) {
    const okItems = imagesToPdf.items.filter(
      (item) => job.results[item.id]?.status === "ok",
    );
    const pages = okItems.reduce((sum, item) => sum + item.pageCount, 0);
    return {
      kind: "mergedPdf",
      pages,
      failedImages: job.finished.failed,
    };
  }
  return { kind: "items", partial: partialCount };
}

/**
 * What the summary of the organize tab's last save tells, by how it started
 * (`JobState.organizeOutput`). A save of one PDF ends in one `job-item`
 * (organize §4): the PDF is written, or it is not and the item says why, with
 * the PDF that failed when it was one of the list. A cancelled save wrote none.
 */
export function organizeSummaryShape(
  job: JobState,
  pdfs: AppState["organize"]["pdfs"],
  language: Language,
): JobSummaryShape {
  if (job.organizeOutput === "split") {
    return organizeSplitSummaryShape(job, pdfs, language);
  }
  const item = job.results[1];
  if (job.finished?.cancelled === true) {
    return { kind: "mergedPdfNotSaved" };
  }
  if (job.finished !== null && job.finished.succeeded > 0) {
    return {
      kind: "organizedPdf",
      pages: job.progress?.total ?? 0,
      replacedName: job.replacedName,
    };
  }
  const error = item?.error;
  return {
    kind: "organizedPdfFailed",
    name: sourceName(item, pdfs),
    message:
      error === undefined
        ? ""
        : formatErrorMessage(error.code, error.detail, language),
  };
}

function sourceName(
  item: JobItemPayload | undefined,
  pdfs: AppState["organize"]["pdfs"],
): string | null {
  return item?.sourceId === undefined
    ? null
    : (pdfs.find((pdf) => pdf.id === item.sourceId)?.name ?? null);
}

/**
 * The save in parts writes one PDF for each output and one `job-item` for
 * each, numbered from 1. A cancelled one keeps the PDFs it wrote, so it counts
 * them as the other conversions do; and the first output that failed is told,
 * as the rows of a list would.
 */
function organizeSplitSummaryShape(
  job: JobState,
  pdfs: AppState["organize"]["pdfs"],
  language: Language,
): JobSummaryShape {
  const { finished } = job;
  if (
    finished !== null &&
    !finished.cancelled &&
    finished.failed === 0 &&
    finished.unprocessed === 0 &&
    finished.succeeded > 0
  ) {
    return { kind: "organizedParts", saved: finished.succeeded };
  }
  const failed = Object.values(job.results)
    .filter((item) => item.status === "failed")
    .sort((a, b) => a.id - b.id)[0];
  if (failed === undefined) {
    return ITEMS_SHAPE;
  }
  return {
    kind: "items",
    partial: 0,
    firstFailure: {
      part: failed.id,
      name: sourceName(failed, pdfs),
      message:
        failed.error === undefined
          ? ""
          : formatErrorMessage(
              failed.error.code,
              failed.error.detail,
              language,
            ),
    },
  };
}
