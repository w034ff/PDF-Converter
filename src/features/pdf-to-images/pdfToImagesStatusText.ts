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

/**
 * Returns the status text for PDF to Images when idle (design §10.1).
 * Counts only items without errors (error === null).
 * Returns null when nothing should be displayed (e.g. range error, or checking without previous result).
 */
export function getPdfToImagesStatusText(
  state: PdfToImagesState,
  t: Translations,
): string | null {
  const validPdfs = state.items.filter((item) => item.error === null);
  const count = validPdfs.length;
  if (count === 0) {
    return t.footer.noPdfsSelected;
  }

  const format =
    state.format === "jpeg"
      ? t.pdfToImages.settings.formatJpeg
      : t.pdfToImages.settings.formatPng;

  if (state.pageSelection === "all") {
    const pages = validPdfs.reduce((sum, item) => sum + item.pageCount, 0);
    return formatSavePages(pages, count, format, t);
  }

  // "range" selection
  if (state.rangeText.trim().length === 0) {
    return t.pdfToImages.footer.specifyPages;
  }

  if (state.rangeError !== null) {
    return null;
  }

  // While checking, use the previous rangeResult if available; otherwise show nothing.
  if (state.rangeResult === null) {
    return null;
  }

  if (state.rangeResult.totalPages === 0) {
    return t.pdfToImages.footer.noMatchingPages;
  }

  return formatSavePages(state.rangeResult.totalPages, count, format, t);
}
