import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import {
  describePdfToImagesStatus,
  type PdfToImagesStatusLine,
} from "./pdfToImagesStatusText";
import { useDisplayedRange } from "./useDisplayedRange";

const TONE_CLASS = {
  hint: "hint",
  notice: "pdf-footer-notice",
  warning: "error-text",
} satisfies Record<PdfToImagesStatusLine["tone"], string>;

/**
 * Bottom bar status text for PDF to Images (design §10.1, mockup `PdfSingle`).
 */
export function PdfToImagesStatus() {
  const { language, pdfToImages } = useAppState();
  const displayed = useDisplayedRange(pdfToImages);
  const t = getTranslations(language.language);
  const status = describePdfToImagesStatus({ ...pdfToImages, ...displayed }, t);
  if (status === null) {
    return null;
  }
  return <span className={TONE_CLASS[status.tone]}>{status.text}</span>;
}
