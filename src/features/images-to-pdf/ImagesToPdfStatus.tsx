import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import { getImagesToPdfStatusText } from "./imagesToPdfStatusText";

/**
 * Bottom bar status text for Images to PDF (design §10.1, mockup `ImagesMerge`).
 */
export function ImagesToPdfStatus() {
  const { language, imagesToPdf } = useAppState();
  const t = getTranslations(language.language);
  const text = getImagesToPdfStatusText(imagesToPdf, t);
  return <span className="hint">{text}</span>;
}
