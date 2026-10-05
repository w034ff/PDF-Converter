import { formatMessage, type Translations } from "../../i18n";
import type { JobFinishedPayload } from "../../ipc";

/**
 * The line shown when a conversion ends, such as
 * "変換が終わりました：成功 3 件 · 失敗 2 件" (mockup `PdfBatch`).
 * Successes are always listed; the other counts only when they are not zero.
 */
export function formatJobSummary(
  t: Translations,
  finished: JobFinishedPayload,
): string {
  const parts = [
    formatMessage(t.job.counts.succeeded, { count: finished.succeeded }),
  ];
  const optional: [number, string][] = [
    [finished.failed, t.job.counts.failed],
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
