import { useEffect, useRef, type ChangeEvent } from "react";
import { SegmentedControl, type SegmentedOption } from "../../components";
import {
  formatErrorMessage,
  formatMessage,
  getTranslations,
  type Translations,
} from "../../i18n";
import {
  checkPageRange,
  normalizeIpcError,
  type RenderFormatChoice,
} from "../../ipc";
import {
  DPI_CHOICES,
  isJobActive,
  useAppDispatch,
  useAppState,
  type PageSelection,
} from "../../state";
import { OutputDirField } from "../output/OutputDirField";
import { calculateRenderDimensions, formatPaperSize } from "./pdfUtils";
import "./pdfToImages.css";

const DPI_LABEL_KEYS: Record<
  number,
  keyof Translations["pdfToImages"]["settings"]["dpiChoices"]
> = {
  72: "dpi72",
  150: "dpi150",
  300: "dpi300",
};

/**
 * Settings panel for PDF to Images (mockups `PdfSingle` and `PdfBatch`).
 */
export function PdfToImagesSettings() {
  const { language, pdfToImages, job } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const busy = isJobActive(job);

  const seqRef = useRef(0);

  useEffect(() => {
    if (pdfToImages.pageSelection !== "range") {
      dispatch({ type: "CHECK_RANGE_RESET" });
      return;
    }

    if (pdfToImages.rangeText.trim().length === 0) {
      dispatch({ type: "CHECK_RANGE_RESET" });
      return;
    }

    const ids = pdfToImages.items
      .filter((item) => item.error === null)
      .map((item) => item.id);

    const seq = ++seqRef.current;
    dispatch({ type: "CHECK_RANGE_STARTED" });

    checkPageRange(pdfToImages.rangeText, ids)
      .then((result) => {
        if (seq === seqRef.current) {
          dispatch({ type: "CHECK_RANGE_SUCCESS", result });
        }
      })
      .catch((error: unknown) => {
        if (seq === seqRef.current) {
          dispatch({
            type: "CHECK_RANGE_FAILURE",
            error: normalizeIpcError(error),
          });
        }
      });
  }, [
    pdfToImages.pageSelection,
    pdfToImages.rangeText,
    pdfToImages.items,
    dispatch,
  ]);

  const pageOptions: readonly SegmentedOption<PageSelection>[] = [
    { value: "all", label: t.pdfToImages.settings.pagesAll },
    { value: "range", label: t.pdfToImages.settings.pagesRange },
  ];

  const formatOptions: readonly SegmentedOption<RenderFormatChoice>[] = [
    { value: "png", label: t.pdfToImages.settings.formatPng },
    { value: "jpeg", label: t.pdfToImages.settings.formatJpeg },
  ];

  function handleDpiChange(e: ChangeEvent<HTMLSelectElement>) {
    const dpi = Number(e.target.value);
    dispatch({ type: "SET_RENDER_DPI", dpi });
  }

  // When 1 valid PDF is in the list, compute rendered dimensions at selected dpi (design §4.4)
  const singlePdf =
    pdfToImages.items.length === 1 && pdfToImages.items[0].error === null
      ? pdfToImages.items[0]
      : null;

  const renderDimensionHint =
    singlePdf !== null && singlePdf.firstPageSizePt !== null
      ? (() => {
          const sizeName = formatPaperSize(singlePdf.firstPageSizePt);
          const dims = calculateRenderDimensions(
            singlePdf.firstPageSizePt,
            pdfToImages.dpi,
          );
          return formatMessage(t.pdfToImages.settings.renderDimension, {
            size: sizeName,
            width: dims.width,
            height: dims.height,
          });
        })()
      : null;

  return (
    <div className="pdf-settings">
      {/* ページ */}
      <div className="pdf-settings-group">
        <span className="label">{t.pdfToImages.settings.pagesTitle}</span>
        <SegmentedControl<PageSelection>
          label={t.pdfToImages.settings.pagesTitle}
          options={pageOptions}
          value={pdfToImages.pageSelection}
          onChange={(selection) =>
            dispatch({ type: "SET_PAGE_SELECTION", selection })
          }
          disabled={busy}
        />
        {pdfToImages.pageSelection === "range" && (
          <>
            <label className="hint" htmlFor="range">
              {t.pdfToImages.settings.rangeLabel}
            </label>
            <input
              id="range"
              className="pdf-settings-field"
              type="text"
              value={pdfToImages.rangeText}
              placeholder={t.pdfToImages.settings.rangePlaceholder}
              onChange={(e) =>
                dispatch({
                  type: "SET_RANGE_TEXT",
                  rangeText: e.target.value,
                })
              }
              disabled={busy}
            />
            {pdfToImages.rangeError !== null ? (
              <span className="hint" role="alert">
                {formatErrorMessage(
                  pdfToImages.rangeError.code,
                  pdfToImages.rangeError.detail,
                  language.language,
                )}
              </span>
            ) : pdfToImages.rangeResult !== null ? (
              <span className="hint">
                {formatMessage(t.pdfToImages.settings.rangeHintSingle, {
                  count: pdfToImages.rangeResult.totalPages,
                })}
              </span>
            ) : null}
            {pdfToImages.items.length > 1 && (
              <span className="hint">
                {t.pdfToImages.settings.rangeHintBatch}
              </span>
            )}
          </>
        )}
      </div>

      {/* 形式 */}
      <div className="pdf-settings-group">
        <span className="label">{t.pdfToImages.settings.formatTitle}</span>
        <SegmentedControl<RenderFormatChoice>
          label={t.pdfToImages.settings.formatTitle}
          options={formatOptions}
          value={pdfToImages.format}
          onChange={(format) => dispatch({ type: "SET_RENDER_FORMAT", format })}
          disabled={busy}
        />
      </div>

      {/* 解像度 */}
      <div className="pdf-settings-group">
        <label className="label" htmlFor="dpi">
          {t.pdfToImages.settings.dpiLabel}
        </label>
        <select
          id="dpi"
          className="pdf-settings-field"
          value={pdfToImages.dpi}
          onChange={handleDpiChange}
          disabled={busy}
        >
          {DPI_CHOICES.map((choice) => {
            const key = DPI_LABEL_KEYS[choice];
            const label = key
              ? t.pdfToImages.settings.dpiChoices[key]
              : `${choice} dpi`;
            return (
              <option key={choice} value={choice}>
                {label}
              </option>
            );
          })}
        </select>
        {renderDimensionHint !== null && (
          <span className="mono">{renderDimensionHint}</span>
        )}
      </div>

      {/* 保存先フォルダ */}
      <OutputDirField
        kind="pdfToImages"
        value={pdfToImages.outputDir}
        onChange={(outputDir) =>
          dispatch({ type: "SET_PDFS_OUTPUT_DIR", outputDir })
        }
        disabled={busy}
      />
    </div>
  );
}
