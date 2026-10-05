import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";

/**
 * The bottom bar's button for PDF to images ("変換を開始").
 * Placeholder until T12 starts conversions through `useJobRunner`.
 */
export function PdfToImagesAction() {
  const { language } = useAppState();
  const t = getTranslations(language.language);

  return (
    <button
      type="button"
      className="btn btn-primary app-footer-action"
      disabled
    >
      {t.footer.startConversion}
    </button>
  );
}
