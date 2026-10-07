import { formatMessage, type Translations } from "../../i18n";
import type { PdfToImagesState } from "../../state";

function formatSavePages(
  pages: number,
  count: number,
  format: string,
  t: Translations,
): string {
  if (count >= 2) {
    const template =
      pages === 1
        ? t.pdfToImages.footer.savePagesFromPdfsSingle
        : t.pdfToImages.footer.savePagesFromPdfsMultiple;
    return formatMessage(template, { count, pages, format });
  }
  const template =
    pages === 1
      ? t.pdfToImages.footer.savePagesSingle
      : t.pdfToImages.footer.savePagesMultiple;
  return formatMessage(template, { pages, format });
}

/** The footer's line, and whether it warns rather than describes. */
export interface PdfToImagesStatusLine {
  text: string;
  isWarning: boolean;
}

/**
 * Describes what starting would do, for the footer while idle (design
 * §10.1). Counts only items without errors. `null` when nothing should be
 * shown: the range has an error, which the settings panel shows, or has not
 * been checked yet. A range that matches no page is a warning, since it
 * converts nothing and should be noticed before pressing the button.
 */
export function describePdfToImagesStatus(
  state: PdfToImagesState,
  t: Translations,
): PdfToImagesStatusLine | null {
  const plain = (text: string) => ({ text, isWarning: false });
  const validPdfs = state.items.filter((item) => item.error === null);
  const count = validPdfs.length;
  if (count === 0) {
    return plain(t.footer.noPdfsSelected);
  }

  const format =
    state.format === "jpeg"
      ? t.pdfToImages.settings.formatJpeg
      : t.pdfToImages.settings.formatPng;

  if (state.pageSelection === "all") {
    const pages = validPdfs.reduce((sum, item) => sum + item.pageCount, 0);
    return plain(formatSavePages(pages, count, format, t));
  }

  // "range" selection
  if (state.rangeText.trim().length === 0) {
    return plain(t.pdfToImages.footer.specifyPages);
  }

  if (state.rangeError !== null) {
    return null;
  }

  // While checking, use the previous rangeResult if available; otherwise show nothing.
  if (state.rangeResult === null) {
    return null;
  }

  if (state.rangeResult.totalPages === 0) {
    return { text: t.pdfToImages.footer.noMatchingPages, isWarning: true };
  }

  return plain(formatSavePages(state.rangeResult.totalPages, count, format, t));
}

/** The text of {@link describePdfToImagesStatus} alone. */
export function getPdfToImagesStatusText(
  state: PdfToImagesState,
  t: Translations,
): string | null {
  return describePdfToImagesStatus(state, t)?.text ?? null;
}
