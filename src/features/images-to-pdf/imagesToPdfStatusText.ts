import { formatMessage, type Translations } from "../../i18n";
import type { ImagesToPdfState } from "../../state";

/**
 * Returns the status text for Images to PDF when idle (design §10.1).
 * Counts only items without errors (error === null).
 */
export function getImagesToPdfStatusText(
  state: ImagesToPdfState,
  t: Translations,
): string {
  const validCount = state.items.filter((item) => item.error === null).length;
  if (validCount === 0) {
    return t.footer.noImagesSelected;
  }
  if (state.output === "merge") {
    return formatMessage(t.imagesToPdf.mergePageCount, { count: validCount });
  }
  const template =
    validCount === 1
      ? t.imagesToPdf.saveEachCountSingle
      : t.imagesToPdf.saveEachCountMultiple;
  return formatMessage(template, { count: validCount });
}
