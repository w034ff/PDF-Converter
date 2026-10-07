import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import { getPdfToImagesStatusText } from "./pdfToImagesStatus";

/**
 * Bottom bar status text for PDF to Images (design §10.1, mockup `PdfSingle`).
 */
export function PdfToImagesStatus() {
  const { language, pdfToImages } = useAppState();
  const t = getTranslations(language.language);
  const text = getPdfToImagesStatusText(pdfToImages, t);
  if (text === null) {
    return null;
  }
  return <span className="hint">{text}</span>;
}
