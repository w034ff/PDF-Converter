import { formatMessage, pluralize, type Translations } from "../../i18n";
import type { Skipped } from "../../ipc";

/**
 * What was skipped, by reason, or `null` if nothing was; for a sentence to
 * hold (design §6.1, FR-01).
 */
export function formatSkippedDetails(
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
      pluralize(
        skipped.folders,
        t.pdfToImages.batch.skippedFolderSingle,
        t.pdfToImages.batch.skippedFolders,
      ),
    );
  }
  if (skipped.duplicates > 0) {
    parts.push(
      pluralize(
        skipped.duplicates,
        t.pdfToImages.batch.skippedDuplicateSingle,
        t.pdfToImages.batch.skippedDuplicates,
      ),
    );
  }
  return parts.length === 0
    ? null
    : parts.join(t.pdfToImages.batch.skippedSeparator);
}

/** The skipped items as an aside to the line of the batch (design §6.1, FR-01). */
export function formatSkippedSummary(
  t: Translations,
  skipped: Skipped | null,
): string | null {
  const details = formatSkippedDetails(t, skipped);
  return details === null
    ? null
    : formatMessage(t.pdfToImages.batch.skippedSummary, { details });
}
