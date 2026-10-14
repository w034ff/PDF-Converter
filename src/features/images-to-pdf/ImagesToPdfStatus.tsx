import { LockIcon } from "../../components";
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
  // Only the line that says what a start would do carries the mark, not "no images selected".
  const isPreview = !status.isWarning && imagesToPdf.items.length > 0;
  if (!imagesToPdf.encrypt || !isPreview) {
    return (
      <span className={status.isWarning ? "error-text" : "hint"}>
        {status.text}
      </span>
    );
  }
  // One element, so that the footer's gap does not come between the line and the mark.
  return (
    <span className="hint">
      {status.text}
      <span className="images-status-protected">
        <span aria-hidden="true">·</span>
        <LockIcon size={12} className="images-status-protected-icon" />
        {t.imagesToPdf.passwordProtected}
      </span>
    </span>
  );
}
