import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import { describeImagesToPdfStatus } from "./imagesToPdfStatusText";

/**
 * Bottom bar status text for Images to PDF (design §10.1, mockup `ImagesMerge`).
 */
export function ImagesToPdfStatus() {
  const { language, imagesToPdf } = useAppState();
  const t = getTranslations(language.language);
  const status = describeImagesToPdfStatus(imagesToPdf, t);
  return (
    <span className={status.isWarning ? "error-text" : "hint"}>
      {status.text}
    </span>
  );
}
