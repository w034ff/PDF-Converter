import { useEffect, useRef } from "react";
import { SegmentedControl, type SegmentedOption } from "../../components";
import { formatErrorMessage, formatMessage, getTranslations } from "../../i18n";
import { checkPageRange, normalizeIpcError } from "../../ipc";
import {
  hasInvalidCustomDpi,
  isJobActive,
  useAppDispatch,
  useAppState,
  type PageSelection,
} from "../../state";
import { OutputDirField } from "../output/OutputDirField";
import { RenderOptionFields } from "./RenderOptionFields";
import { calculateRenderDimensions, formatPaperSize } from "./pdfUtils";
import { useDelayedReveal } from "./useDelayedReveal";
import "./pdfToImages.css";

export { FIELD_ERROR_REVEAL_DELAY_MS } from "./useDelayedReveal";

/**
 * Settings panel for PDF to Images (mockups `PdfSingle` and `PdfBatch`).
 */
export function PdfToImagesSettings() {
  const { language, pdfToImages, job } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const busy = isJobActive(job);

  const seqRef = useRef(0);

  // A range error is shown once typing pauses or the field loses focus, so
  // a half-typed "1-" is not flagged; the start button is disabled at once.
  const hasRangeError =
    pdfToImages.rangeError !== null && !pdfToImages.rangeChecking;
  const [isRangeErrorShown, revealRangeError] = useDelayedReveal(
    pdfToImages.rangeText,
    hasRangeError,
  );

  const isCustomDpiError = hasInvalidCustomDpi(pdfToImages);

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

  // An empty range selects no page, so switching from "all" to it would
  // start by deselecting everything. Starting from every page lets the user
  // drop the pages they do not want instead (design §6.3).
  function handlePageSelectionChange(selection: PageSelection) {
    dispatch({ type: "SET_PAGE_SELECTION", selection });
    if (selection !== "range" || pdfToImages.rangeText.trim().length > 0) {
      return;
    }
    const maxPages = Math.max(
      0,
      ...pdfToImages.items
        .filter((item) => item.error === null)
        .map((item) => item.pageCount),
    );
    if (maxPages > 0) {
      dispatch({ type: "SET_RANGE_TEXT", rangeText: `1-${maxPages}` });
    }
  }

  // When 1 valid PDF is in the list, compute rendered dimensions at selected dpi (design §4.4)
  const singlePdf =
    pdfToImages.items.length === 1 && pdfToImages.items[0].error === null
      ? pdfToImages.items[0]
      : null;

  const isDpiValid = !isCustomDpiError;

  const renderDimensionHint =
    isDpiValid && singlePdf !== null && singlePdf.firstPageSizePt !== null
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
          onChange={handlePageSelectionChange}
          disabled={busy}
        />
        {singlePdf !== null && (
          <span className="hint">
            {t.pdfToImages.settings.thumbnailPickHint}
          </span>
        )}
        {pdfToImages.pageSelection === "range" && (
          <>
            <label className="hint" htmlFor="range">
              {t.pdfToImages.settings.rangeLabel}
            </label>
            <input
              id="range"
              className="settings-control"
              type="text"
              autoComplete="off"
              value={pdfToImages.rangeText}
              placeholder={t.pdfToImages.settings.rangePlaceholder}
              onChange={(e) =>
                dispatch({
                  type: "SET_RANGE_TEXT",
                  rangeText: e.target.value,
                })
              }
              onBlur={revealRangeError}
              disabled={busy}
            />
            {/* How many pages this converts is in the footer (design §10.1). */}
            {isRangeErrorShown && pdfToImages.rangeError !== null && (
              <span className="error-text" role="alert">
                {formatErrorMessage(
                  pdfToImages.rangeError.code,
                  pdfToImages.rangeError.detail,
                  language.language,
                )}
              </span>
            )}
            {pdfToImages.items.length > 1 && (
              <span className="hint">
                {t.pdfToImages.settings.rangeHintBatch}
              </span>
            )}
          </>
        )}
      </div>

      <RenderOptionFields
        format={pdfToImages.format}
        colorMode={pdfToImages.colorMode}
        dpi={pdfToImages.dpi}
        isCustomDpi={pdfToImages.isCustomDpi}
        customDpiText={pdfToImages.customDpiText}
        onFormatChange={(format) =>
          dispatch({ type: "SET_RENDER_FORMAT", format })
        }
        onColorModeChange={(colorMode) =>
          dispatch({ type: "SET_RENDER_COLOR_MODE", colorMode })
        }
        onDpiChange={(dpi) => dispatch({ type: "SET_RENDER_DPI", dpi })}
        onCustomDpiSelected={() =>
          dispatch({ type: "SET_IS_CUSTOM_DPI", isCustom: true })
        }
        onCustomDpiTextChange={(text) =>
          dispatch({ type: "SET_CUSTOM_DPI_TEXT", text })
        }
        disabled={busy}
        dpiNote={
          renderDimensionHint !== null && (
            <span className="mono">{renderDimensionHint}</span>
          )
        }
      />

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
