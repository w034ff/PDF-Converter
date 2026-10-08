import { SegmentedControl, type SegmentedOption } from "../../components";
import { OutputDirField } from "../output/OutputDirField";
import { getTranslations } from "../../i18n";
import type { OutputMode, PageSizeChoice } from "../../ipc";
import { isJobActive, useAppDispatch, useAppState } from "../../state";
import "./ImagesToPdf.css";

/**
 * Settings panel for Images to PDF (design §6.1, §6.2, mockups Main.dc.html, ImagesMerge.dc.html, ImagesEach.dc.html).
 * Provides output mode ("merge" | "each"), page size ("fit" | "a4"), and output folder picker for "each" mode.
 */
export function ImagesToPdfSettings() {
  const { imagesToPdf, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const disabled = isJobActive(job);

  const outputOptions: readonly SegmentedOption<OutputMode>[] = [
    { value: "merge", label: t.imagesToPdf.outputMerge },
    { value: "each", label: t.imagesToPdf.outputEach },
  ];

  const pageSizeOptions: readonly SegmentedOption<PageSizeChoice>[] = [
    { value: "fit", label: t.imagesToPdf.pageSizeFit },
    { value: "a4", label: t.imagesToPdf.pageSizeA4 },
  ];

  return (
    <div className="images-settings">
      <div className="images-settings-field">
        <span className="label">{t.imagesToPdf.outputLabel}</span>
        <SegmentedControl
          label={t.imagesToPdf.outputLabel}
          options={outputOptions}
          value={imagesToPdf.output}
          onChange={(value) =>
            dispatch({ type: "SET_IMAGES_OUTPUT_MODE", output: value })
          }
          disabled={disabled}
        />
      </div>

      <div className="images-settings-field">
        <span className="label">{t.imagesToPdf.pageSizeLabel}</span>
        <SegmentedControl
          label={t.imagesToPdf.pageSizeLabel}
          options={pageSizeOptions}
          value={imagesToPdf.pageSize}
          onChange={(value) =>
            dispatch({ type: "SET_IMAGES_PAGE_SIZE", pageSize: value })
          }
          disabled={disabled}
        />
        <span className="hint">
          {imagesToPdf.pageSize === "fit"
            ? t.imagesToPdf.pageSizeFitHint
            : t.imagesToPdf.pageSizeA4Hint}
        </span>
      </div>

      {imagesToPdf.output === "each" && (
        <OutputDirField
          kind="imagesToPdf"
          value={imagesToPdf.outputDir}
          onChange={(dir) =>
            dispatch({ type: "SET_IMAGES_OUTPUT_DIR", outputDir: dir })
          }
          disabled={disabled}
          hint={t.imagesToPdf.outputDirHint}
        />
      )}
    </div>
  );
}
