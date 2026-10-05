import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";

/**
 * The bottom bar's button for Images to PDF ("PDF を保存" or "変換を開始").
 * Placeholder until T11 starts conversions through `useJobRunner`.
 */
export function ImagesToPdfAction() {
  const { language } = useAppState();
  const t = getTranslations(language.language);

  return (
    <button
      type="button"
      className="btn btn-primary app-footer-action"
      disabled
    >
      {t.footer.savePdf}
    </button>
  );
}
