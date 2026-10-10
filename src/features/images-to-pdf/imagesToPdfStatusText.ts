import { formatMessage, type Translations } from "../../i18n";
import type { ImagesToPdfState } from "../../state";

/** The footer's line, and whether it warns rather than describes. */
export interface ImagesToPdfStatusLine {
  text: string;
  isWarning: boolean;
}

/**
 * Describes what starting would do, for the footer while idle (design
 * §10.1). Counts only items without errors. A list holding only items that
 * could not be loaded is a warning: the start button is disabled, and "no
 * images selected" would contradict the rows on screen.
 */
export function describeImagesToPdfStatus(
  state: ImagesToPdfState,
  t: Translations,
): ImagesToPdfStatusLine {
  const plain = (text: string) => ({ text, isWarning: false });
  if (state.items.length === 0) {
    return plain(t.footer.noImagesSelected);
  }
  const validCount = state.items.filter((item) => item.error === null).length;
  if (validCount === 0) {
    return { text: t.footer.noReadableImages, isWarning: true };
  }
  if (state.output === "merge") {
    return plain(
      formatMessage(t.imagesToPdf.mergePageCount, { count: validCount }),
    );
  }
  const template =
    validCount === 1
      ? t.imagesToPdf.saveEachCountSingle
      : t.imagesToPdf.saveEachCountMultiple;
  return plain(formatMessage(template, { count: validCount }));
}

/** The text of {@link describeImagesToPdfStatus} alone. */
export function getImagesToPdfStatusText(
  state: ImagesToPdfState,
  t: Translations,
): string {
  return describeImagesToPdfStatus(state, t).text;
}
