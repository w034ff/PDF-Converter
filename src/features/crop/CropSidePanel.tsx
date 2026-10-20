import { getTranslations } from "../../i18n";
import { useAppState } from "../../state";
import { CropExportSettings } from "./CropExportSettings";
import "./crop.css";

/**
 * The column on the right of the crop tab (mockup `CropA`): the list of
 * areas, and the export settings under it. There are no areas yet, so the
 * list only says so.
 */
export function CropSidePanel() {
  const { language } = useAppState();
  const t = getTranslations(language.language);

  return (
    <section className="crop-side" aria-label={t.crop.ranges.sectionAria}>
      <div className="crop-ranges-header">
        <span className="label">{t.crop.ranges.title}</span>
      </div>
      <div className="crop-ranges">
        <p className="hint">{t.crop.ranges.empty}</p>
      </div>
      <CropExportSettings />
    </section>
  );
}
