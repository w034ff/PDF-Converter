import { formatMessage, type Translations } from "../../i18n";
import type { JobFinishedPayload } from "../../ipc";

/**
 * The line shown when a conversion ends, such as
 * "変換が終わりました：成功 3 件 · 失敗 2 件" (mockup `PdfBatch`).
 * Successes are always listed; the other counts only when they are not zero.
 *
 * `job-finished` counts a PDF that failed on some pages as failed, though its
 * other pages were saved. `partial` is how many of those there were (from
 * `job-item`); they are listed on their own instead of as failures.
 */
export function formatJobSummary(
  t: Translations,
  finished: JobFinishedPayload,
  partial = 0,
): string {
  const parts = [
    formatMessage(t.job.counts.succeeded, { count: finished.succeeded }),
  ];
  const optional: [number, string][] = [
    [finished.failed - partial, t.job.counts.failed],
    [partial, t.job.counts.partial],
    [finished.noPages, t.job.counts.noPages],
    [finished.unprocessed, t.job.counts.unprocessed],
  ];
  for (const [count, template] of optional) {
    if (count > 0) {
      parts.push(formatMessage(template, { count }));
    }
  }
  const template = finished.cancelled
    ? t.job.summaryCancelled
    : t.job.summaryFinished;
  return formatMessage(template, {
    counts: parts.join(t.job.summarySeparator),
  });
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
