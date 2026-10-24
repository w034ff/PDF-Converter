import { formatMessage, getTranslations } from "../../i18n";
import type { CropPdf } from "../../ipc";
import { isJobActive, useAppState } from "../../state";
import { JobSummaryBanner } from "../job/JobSummaryBanner";
import { useJobRunner } from "../job/useJobRunner";
import { CropAreaList } from "./CropAreaList";
import { CropExportSettings } from "./CropExportSettings";
import "./crop.css";

/**
 * The column on the right of the crop tab (mockup `CropA`): the result of the
 * last conversion, the list of areas with how many there are and how many are
 * to be exported, and the export settings under it.
 */
export function CropSidePanel({ pdf }: { pdf: CropPdf }) {
  const { language, crop, job } = useAppState();
  const runner = useJobRunner();
  const t = getTranslations(language.language);
  const count = crop.areas.length;
  const checked = crop.areas.filter((area) => area.checked).length;

  return (
    <section className="crop-side" aria-label={t.crop.ranges.sectionAria}>
      <JobSummaryBanner tab="crop" />
      <div className="crop-ranges-header">
        <span className="label">{t.crop.ranges.title}</span>
        {count > 0 && (
          <span className="hint">
            {formatMessage(
              count === 1
                ? t.crop.ranges.summarySingle
                : t.crop.ranges.summaryMultiple,
              { count, checked },
            )}
          </span>
        )}
      </div>
      <div className="crop-ranges">
        {count === 0 ? (
          // The way in to the main use: let the analysis find the areas.
          <div className="crop-ranges-empty">
            <p className="hint">{t.crop.ranges.empty}</p>
            <p className="hint">{t.crop.ranges.emptyHint}</p>
            <button
              type="button"
              className="btn btn-primary"
              disabled={
                crop.layoutUnavailable || pdf.error !== null || isJobActive(job)
              }
              onClick={() => void runner.startDetectAll(pdf.id)}
            >
              {t.crop.ranges.detectAll}
            </button>
          </div>
        ) : (
          <CropAreaList pdf={pdf} />
        )}
      </div>
      <CropExportSettings />
    </section>
  );
}
