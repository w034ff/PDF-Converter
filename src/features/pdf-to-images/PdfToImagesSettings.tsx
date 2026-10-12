import { useEffect, useRef, useState, type ChangeEvent } from "react";
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
  type RenderColorChoice,
  type RenderFormatChoice,
} from "../../ipc";
import {
  DPI_CHOICES,
  RENDER_DPI_MAX,
  RENDER_DPI_MIN,
  isJobActive,
  parseCustomDpi,
  useAppDispatch,
  useAppState,
  type PageSelection,
} from "../../state";
import { OutputDirField } from "../output/OutputDirField";
import { calculateRenderDimensions, formatPaperSize } from "./pdfUtils";
import "./pdfToImages.css";

/** How long typing must pause before a field error is shown. */
export const RANGE_ERROR_REVEAL_DELAY_MS = 600;

const DPI_LABEL_KEYS: Record<
  number,
  keyof Translations["pdfToImages"]["settings"]["dpiChoices"]
> = {
  72: "dpi72",
  150: "dpi150",
  300: "dpi300",
};

/**
 * Hook to delay revealing an error until typing has paused or the input has blurred.
 */
function useDelayedReveal(
  value: string,
  hasError: boolean,
  delayMs = RANGE_ERROR_REVEAL_DELAY_MS,
): [boolean, () => void] {
  const [revealedFor, setRevealedFor] = useState<string | null>(null);

  useEffect(() => {
    if (!hasError) {
      return;
    }
    const timer = setTimeout(() => setRevealedFor(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, hasError, delayMs]);

  const isRevealed = hasError && revealedFor === value;
  const reveal = () => setRevealedFor(value);
  return [isRevealed, reveal];
}

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

  // Custom DPI error is delayed in the same manner.
  const isCustomDpiError =
    pdfToImages.isCustomDpi &&
    parseCustomDpi(pdfToImages.customDpiText) === null;
  const [isCustomDpiErrorShown, revealCustomDpiError] = useDelayedReveal(
    pdfToImages.customDpiText,
    isCustomDpiError,
  );

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
    { value: "webp", label: t.pdfToImages.settings.formatWebp },
  ];

  const colorOptions: readonly SegmentedOption<RenderColorChoice>[] = [
    { value: "color", label: t.pdfToImages.settings.colorFullColor },
    { value: "gray", label: t.pdfToImages.settings.colorGrayscale },
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

  function handleDpiSelectChange(e: ChangeEvent<HTMLSelectElement>) {
    if (e.target.value === "custom") {
      dispatch({ type: "SET_IS_CUSTOM_DPI", isCustom: true });
    } else {
      const dpi = Number(e.target.value);
      dispatch({ type: "SET_RENDER_DPI", dpi });
    }
  }

  // When 1 valid PDF is in the list, compute rendered dimensions at selected dpi (design §4.4)
  const singlePdf =
    pdfToImages.items.length === 1 && pdfToImages.items[0].error === null
      ? pdfToImages.items[0]
      : null;

  const isDpiValid =
    !pdfToImages.isCustomDpi ||
    parseCustomDpi(pdfToImages.customDpiText) !== null;

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

  const selectDpiValue = pdfToImages.isCustomDpi
    ? "custom"
    : String(pdfToImages.dpi);

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
              className="pdf-settings-field"
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

      {/* 色 */}
      <div className="pdf-settings-group">
        <span className="label">{t.pdfToImages.settings.colorTitle}</span>
        <SegmentedControl<RenderColorChoice>
          label={t.pdfToImages.settings.colorTitle}
          options={colorOptions}
          value={pdfToImages.colorMode}
          onChange={(colorMode) =>
            dispatch({ type: "SET_RENDER_COLOR_MODE", colorMode })
          }
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
          value={selectDpiValue}
          onChange={handleDpiSelectChange}
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
          <option value="custom">
            {t.pdfToImages.settings.dpiChoices.custom}
          </option>
        </select>
        {pdfToImages.isCustomDpi && (
          <div className="pdf-custom-dpi-row">
            <input
              type="text"
              inputMode="numeric"
              className="pdf-settings-field pdf-custom-dpi-input"
              aria-label={t.pdfToImages.settings.customDpiLabel}
              value={pdfToImages.customDpiText}
              onChange={(e) =>
                dispatch({
                  type: "SET_CUSTOM_DPI_TEXT",
                  text: e.target.value,
                })
              }
              onBlur={revealCustomDpiError}
              disabled={busy}
            />
            <span className="pdf-custom-dpi-unit">
              {t.pdfToImages.settings.customDpiUnit}
            </span>
          </div>
        )}
        {isCustomDpiErrorShown && (
          <span className="error-text" role="alert">
            {formatMessage(t.pdfToImages.settings.customDpiError, {
              min: RENDER_DPI_MIN,
              max: RENDER_DPI_MAX,
            })}
          </span>
        )}
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
