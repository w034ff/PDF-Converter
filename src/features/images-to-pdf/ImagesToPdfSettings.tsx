import type { ChangeEvent } from "react";
import { SegmentedControl, type SegmentedOption } from "../../components";
import { OutputDirField } from "../output/OutputDirField";
import { getTranslations } from "../../i18n";
import type { OutputMode, PageOrientationChoice } from "../../ipc";
import {
  imagesPdfFormat,
  isJobActive,
  isPageSizeChoice,
  PAPER_SIZE_OPTIONS,
  type ImagesPdfFormat,
  useAppDispatch,
  useAppState,
} from "../../state";
import "./ImagesToPdf.css";

/**
 * Settings panel for Images to PDF (design §6.1, §6.2, mockups Main.dc.html, ImagesMerge.dc.html, ImagesEach.dc.html).
 * Provides output mode ("merge" | "each"), page size dropdown, orientation, the PDF format (standard, PDF/A or password), and output folder picker for "each" mode.
 */
export function ImagesToPdfSettings() {
  const { imagesToPdf, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const disabled = isJobActive(job);
  const isFit = imagesToPdf.pageSize === "fit";

  const outputOptions: readonly SegmentedOption<OutputMode>[] = [
    { value: "merge", label: t.imagesToPdf.outputMerge },
    { value: "each", label: t.imagesToPdf.outputEach },
  ];

  const orientationOptions: readonly SegmentedOption<PageOrientationChoice>[] =
    [
      { value: "auto", label: t.imagesToPdf.orientationAuto },
      { value: "portrait", label: t.imagesToPdf.orientationPortrait },
      { value: "landscape", label: t.imagesToPdf.orientationLandscape },
    ];

  const pdfFormatOptions: readonly SegmentedOption<ImagesPdfFormat>[] = [
    { value: "standard", label: t.imagesToPdf.pdfFormatStandard },
    { value: "pdfA", label: t.imagesToPdf.pdfFormatPdfA },
    { value: "encrypt", label: t.imagesToPdf.pdfFormatEncrypt },
  ];
  const pdfFormat = imagesPdfFormat(imagesToPdf);

  const handlePageSizeChange = (event: ChangeEvent<HTMLSelectElement>) => {
    const value = event.target.value;
    if (isPageSizeChoice(value)) {
      dispatch({ type: "SET_IMAGES_PAGE_SIZE", pageSize: value });
    }
  };

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
        <label className="label" htmlFor="pageSize">
          {t.imagesToPdf.pageSizeLabel}
        </label>
        <select
          id="pageSize"
          className="settings-control"
          value={imagesToPdf.pageSize}
          onChange={handlePageSizeChange}
          disabled={disabled}
        >
          <option value="fit">{t.imagesToPdf.pageSizeFit}</option>
          {PAPER_SIZE_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <span className="hint">
          {isFit
            ? t.imagesToPdf.pageSizeFitHint
            : t.imagesToPdf.pageSizePaperHint}
        </span>
      </div>

      {/* Always shown, so choosing a page size never moves the fields below
          it: a field popping in where the dropdown list just closed reads as
          a ghost of the list (design §10.1). */}
      <div className="images-settings-field">
        <span className="label">{t.imagesToPdf.orientationLabel}</span>
        <SegmentedControl
          label={t.imagesToPdf.orientationLabel}
          options={orientationOptions}
          value={imagesToPdf.orientation}
          onChange={(value) =>
            dispatch({
              type: "SET_IMAGES_ORIENTATION",
              orientation: value,
            })
          }
          disabled={disabled || isFit}
        />
        {isFit ? (
          <span className="hint">
            {t.imagesToPdf.orientationUnavailableHint}
          </span>
        ) : (
          imagesToPdf.orientation === "auto" && (
            <span className="hint">{t.imagesToPdf.orientationAutoHint}</span>
          )
        )}
      </div>

      <div className="images-settings-field">
        <span className="label">{t.imagesToPdf.pdfFormatLabel}</span>
        <SegmentedControl
          label={t.imagesToPdf.pdfFormatLabel}
          className="seg images-pdf-format"
          options={pdfFormatOptions}
          value={pdfFormat}
          onChange={(format) =>
            dispatch({ type: "SET_IMAGES_PDF_FORMAT", format })
          }
          disabled={disabled}
        />
        {pdfFormat === "pdfA" && (
          <span className="hint">{t.imagesToPdf.pdfAHint}</span>
        )}
        {pdfFormat === "encrypt" && (
          <span className="hint">{t.imagesToPdf.encryptHint}</span>
        )}
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
