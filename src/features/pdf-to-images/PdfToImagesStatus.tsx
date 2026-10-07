import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import { describePdfToImagesStatus } from "./pdfToImagesStatusText";
import { useDisplayedRange } from "./useDisplayedRange";

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
  return (
    <span className={status.isWarning ? "hint pdf-hint-warning" : "hint"}>
      {status.text}
    </span>
  );
}
