import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";

/** What the bottom bar says while nothing is converting (design §10.1). */
export function CropStatus() {
  const { language, crop } = useAppState();
  const t = getTranslations(language.language);
  return (
    <span className="hint">
      {crop.pdf === null ? t.crop.footer.noPdf : t.crop.footer.noRanges}
    </span>
  );
}
