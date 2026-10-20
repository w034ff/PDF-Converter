import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";

/**
 * The bottom bar's "書き出す". It cannot be pressed yet: nothing selects an
 * area to write.
 */
export function CropAction() {
  const { language } = useAppState();
  const t = getTranslations(language.language);
  return (
    <button
      type="button"
      className="btn btn-primary app-footer-action"
      disabled
    >
      {t.crop.footer.export}
    </button>
  );
}
