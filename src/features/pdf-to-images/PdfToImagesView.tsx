import { useState } from "react";
import { DropZone, ErrorDisplay } from "../../components";
import {
  formatErrorMessage,
  formatMessage,
  getTranslations,
  type Language,
  type Translations,
} from "../../i18n";
import {
  addPdfs,
  normalizeIpcError,
  removeItems,
  type AddSource,
  type CheckPageRangeResult,
  type IpcError,
  type PageSizePt,
  type PdfItem,
  type Skipped,
} from "../../ipc";
import {
  isJobActive,
  jobRowStatus,
  useAppDispatch,
  useAppState,
  type JobState,
  type PageSelection,
} from "../../state";
import { useThumbnail } from "../items/useThumbnail";
import { JobSummaryBanner } from "../job/JobSummaryBanner";
import { OutputDirError } from "../output";
import {
  countPagesInIntervals,
  formatFileSize,
  formatPaperSize,
  formatSkippedSummary,
  intervalsToRangeText,
  isPageInIntervals,
  pageOutcomes,
  togglePageRangeInIntervals,
  type PageOutcome,
} from "./pdfUtils";
import { useDisplayedRange } from "./useDisplayedRange";
import "./pdfToImages.css";

/** The colour class of a page's caption: only a result colours it. */
function captionClass(outcome: PageOutcome | null): string {
  switch (outcome) {
    case "done":
      return "pdf-status-ok";
    case "failed":
      return "pdf-status-failed";
    case "unprocessed":
      return "pdf-status-muted";
    case null:
      return "";
  }
}

/**
 * The words under a page: its result if the last conversion has one, else
 * what the next one will do. A page that is not selected has its number only.
 */
function captionText(
  t: Translations,
  page: number,
  isHighlighted: boolean,
  outcome: PageOutcome | null,
): string {
  if (!isHighlighted) {
    return String(page);
  }
  switch (outcome) {
    case "done":
      return formatMessage(t.pdfToImages.single.donePage, { page });
    case "failed":
      return formatMessage(t.pdfToImages.single.failedPage, { page });
    case "unprocessed":
      return formatMessage(t.pdfToImages.single.pageWithStatus, {
        page,
        status: t.job.rowStatus.unprocessed,
      });
    case null:
      return formatMessage(t.pdfToImages.single.convertPage, { page });
  }
}

interface PdfPageThumbnailProps {
  id: number;
  page: number;
  name: string;
  firstPageSizePt: PageSizePt | null;
  isHighlighted: boolean;
  /** What the last conversion did to this page; `null` if it has no result. */
  outcome: PageOutcome | null;
  disabled: boolean;
  onClick: (page: number, shiftKey: boolean) => void;
  translations: Translations;
}

function PdfPageThumbnail({
  id,
  page,
  name,
  firstPageSizePt,
  isHighlighted,
  outcome,
  disabled,
  onClick,
  translations: t,
}: PdfPageThumbnailProps) {
  const { ref, src, failed } = useThumbnail(id, page);
  const isFailed = outcome === "failed";

  const aspectRatio = firstPageSizePt
    ? `${firstPageSizePt.widthPt} / ${firstPageSizePt.heightPt}`
    : "210 / 297";

  return (
    <figure
      className={`pdf-page-figure ${!isHighlighted ? "is-dimmed" : ""}`}
      data-testid={`page-thumbnail-${page}`}
    >
      <button
        type="button"
        className="pdf-page-button"
        disabled={disabled}
        aria-pressed={isHighlighted}
        aria-label={formatMessage(t.pdfToImages.single.pageAriaLabel, { page })}
        onClick={(e) => onClick(page, e.shiftKey)}
      >
        <span
          ref={ref}
          className={`pdf-page-container ${isHighlighted ? "is-highlighted" : ""} ${isFailed ? "is-failed" : ""}`}
          style={{ aspectRatio }}
        >
          {src !== null ? (
            <img
              src={src}
              alt={formatMessage(t.pdfToImages.single.thumbnailAlt, {
                name,
                page,
              })}
              className="pdf-page-image"
            />
          ) : (
            <span className="pdf-page-placeholder">
              {failed ? <span className="hint">✕</span> : null}
            </span>
          )}
          {isFailed && (
            // The caption under the page already says it failed in words.
            <span className="pdf-page-failed-badge" aria-hidden="true">
              ✕
            </span>
          )}
        </span>
      </button>
      <figcaption className={`mono ${captionClass(outcome)}`}>
        {captionText(t, page, isHighlighted, outcome)}
      </figcaption>
    </figure>
  );
}

interface ItemStatus {
  text: string;
  className: string;
  reason: string | null;
}

/**
 * How item `item` reads after being added or converted: its own error, or
 * the result of the last conversion of this screen. `null` when there is
 * neither. Shared by the table and the single-PDF view so both say the same.
 */
function describeItemStatus(
  item: PdfItem,
  job: JobState,
  t: Translations,
  language: Language,
): ItemStatus | null {
  let statusText: string | null = null;
  let statusClass = "";
  let reasonText: string | null = null;

  if (item.error !== null) {
    statusText = t.job.loadFailed;
    statusClass = "pdf-status-failed";
    reasonText = formatErrorMessage(
      item.error.code,
      item.error.detail,
      language,
    );
  } else if (job.kind === "pdfToImages") {
    const status = jobRowStatus(job, item.id);
    if (status !== null) {
      statusText = t.job.rowStatus[status];
      switch (status) {
        case "ok":
          statusClass = "pdf-status-ok";
          break;
        case "failed": {
          statusClass = "pdf-status-failed";
          const itemResult = job.results[item.id];
          if (itemResult && itemResult.error) {
            reasonText = formatErrorMessage(
              itemResult.error.code,
              itemResult.error.detail,
              language,
            );
          }
          break;
        }
        case "partial": {
          statusClass = "pdf-status-partial";
          const itemResult = job.results[item.id];
          if (
            itemResult &&
            itemResult.failedPages &&
            itemResult.failedPages.length > 0
          ) {
            reasonText = formatMessage(t.pdfToImages.batch.failedPagesReason, {
              pages: itemResult.failedPages.join(", "),
            });
          }
          break;
        }
        case "cancelled":
          statusClass = "pdf-status-muted";
          reasonText = formatMessage(t.pdfToImages.batch.cancelledSavedReason, {
            count: job.results[item.id]?.outputs.length ?? 0,
          });
          break;
        case "noPages":
          statusClass = "pdf-status-muted";
          reasonText = t.pdfToImages.batch.noPagesReason;
          break;
        case "running":
          statusClass = "pdf-status-running";
          break;
        case "waiting":
        case "unprocessed":
          statusClass = "pdf-status-muted";
          break;
      }
    }
  }

  return statusText === null
    ? null
    : { text: statusText, className: statusClass, reason: reasonText };
}

interface PdfTableRowProps {
  item: PdfItem;
  job: JobState;
  pageSelection: PageSelection;
  rangeResult: CheckPageRangeResult | null;
  onRemove: () => void;
  disabled: boolean;
  translations: Translations;
  language: Language;
}

function PdfTableRow({
  item,
  job,
  pageSelection,
  rangeResult,
  onRemove,
  disabled,
  translations: t,
  language,
}: PdfTableRowProps) {
  let pagesText = "—";
  if (item.error === null) {
    if (job.kind === "pdfToImages" && job.results[item.id] !== undefined) {
      const res = job.results[item.id];
      if (res.status === "noPages") {
        pagesText = `— / ${item.pageCount}`;
      } else if (res.status === "failed") {
        pagesText = `— / ${item.pageCount}`;
      } else {
        pagesText = `${res.outputs.length} / ${item.pageCount}`;
      }
    } else {
      if (pageSelection === "all") {
        pagesText = `${item.pageCount} / ${item.pageCount}`;
      } else if (rangeResult !== null) {
        const count = countPagesInIntervals(
          rangeResult.intervals,
          item.pageCount,
        );
        pagesText =
          count > 0 ? `${count} / ${item.pageCount}` : `— / ${item.pageCount}`;
      } else {
        pagesText = `— / ${item.pageCount}`;
      }
    }
  }

  let savedFilesText = "—";
  if (job.kind === "pdfToImages" && job.results[item.id] !== undefined) {
    const outputs = job.results[item.id].outputs;
    if (outputs.length === 1) {
      savedFilesText = outputs[0];
    } else if (outputs.length > 1) {
      savedFilesText = `${outputs[0]} …`;
    }
  }

  const described = describeItemStatus(item, job, t, language);
  const statusText = described?.text ?? "—";
  const statusClass = described?.className ?? "";
  const reasonText = described?.reason ?? null;

  return (
    <tr>
      <td>{item.name}</td>
      <td className="mono pdf-col-pages">{pagesText}</td>
      <td className="mono">{savedFilesText}</td>
      <td className={statusClass}>
        {statusText}
        {reasonText !== null && (
          <div className="pdf-status-reason">{reasonText}</div>
        )}
      </td>
      <td className="pdf-col-actions">
        <button
          type="button"
          className="btn btn-ghost"
          disabled={disabled}
          onClick={onRemove}
        >
          {t.pdfToImages.remove}
        </button>
      </td>
    </tr>
  );
}

/**
 * Main view for PDF to Images (mockups `PdfSingle` and `PdfBatch`).
 */
export function PdfToImagesView() {
  const { language, pdfToImages, job } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const busy = isJobActive(job);
  const [lastSkipped, setLastSkipped] = useState<Skipped | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const displayed = useDisplayedRange(pdfToImages);

  const isSingle = pdfToImages.items.length === 1;
  const singleItem = isSingle ? pdfToImages.items[0] : null;

  const singleItemId = singleItem?.id ?? null;
  const [lastItemId, setLastItemId] = useState<number | null>(singleItemId);
  const [anchorPage, setAnchorPage] = useState<number | null>(null);

  if (singleItemId !== lastItemId) {
    setLastItemId(singleItemId);
    setAnchorPage(null);
  }

  async function handleAdd(source: AddSource) {
    setError(null);
    try {
      const result = await addPdfs(source);
      if (result !== null) {
        if (result.added.length > 0) {
          dispatch({ type: "ADD_PDF_ITEMS", items: result.added });
        }
        setLastSkipped(result.skipped);
      }
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
  }

  async function handleRemove(id: number) {
    setError(null);
    try {
      await removeItems([id]);
      dispatch({ type: "REMOVE_PDF_ITEM", id });
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
  }

  async function handleClearAll() {
    setError(null);
    const ids = pdfToImages.items.map((i) => i.id);
    try {
      await removeItems(ids);
      dispatch({ type: "CLEAR_PDF_ITEMS" });
      setLastSkipped(null);
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
  }

  if (pdfToImages.items.length === 0) {
    return (
      <div className="pdf-view">
        {error !== null && (
          <ErrorDisplay
            message={formatErrorMessage(
              error.code,
              error.detail,
              language.language,
            )}
            onDismiss={() => setError(null)}
            dismissLabel={t.errors.dismiss}
          />
        )}
        <DropZone
          title={t.dropZone.titlePdfs}
          description={t.dropZone.descriptionPdfs}
          addFilesLabel={t.dropZone.addPdfs}
          addFolderLabel={t.dropZone.addFolder}
          onAddFiles={busy ? undefined : () => void handleAdd("files")}
          onAddFolder={busy ? undefined : () => void handleAdd("folder")}
        />
      </div>
    );
  }

  const isThumbnailDisabled =
    isJobActive(job) ||
    (pdfToImages.pageSelection === "range" && pdfToImages.rangeError !== null);

  function handleThumbnailClick(page: number, shiftKey: boolean) {
    if (
      isThumbnailDisabled ||
      pdfToImages.rangeChecking ||
      singleItem === null
    ) {
      return;
    }
    const pageCount = singleItem.pageCount;
    const anchor = anchorPage;
    setAnchorPage(page);

    const fromPage = shiftKey && anchor !== null ? anchor : page;
    const toPage = page;

    if (pdfToImages.pageSelection === "all") {
      dispatch({ type: "SET_PAGE_SELECTION", selection: "range" });
      const nextIntervals = togglePageRangeInIntervals(
        [[1, pageCount]],
        pageCount,
        fromPage,
        toPage,
        false,
      );
      dispatch({
        type: "SET_RANGE_TEXT",
        rangeText: intervalsToRangeText(nextIntervals),
      });
      return;
    }

    const currentIntervals =
      pdfToImages.rangeResult !== null ? pdfToImages.rangeResult.intervals : [];
    const isCurrentlySelected =
      pdfToImages.rangeResult !== null &&
      isPageInIntervals(pdfToImages.rangeResult.intervals, page);
    const include = !isCurrentlySelected;

    const nextIntervals = togglePageRangeInIntervals(
      currentIntervals,
      pageCount,
      fromPage,
      toPage,
      include,
    );
    dispatch({
      type: "SET_RANGE_TEXT",
      rangeText: intervalsToRangeText(nextIntervals),
    });
  }

  const skippedText = formatSkippedSummary(t, lastSkipped);
  const results = job.kind === "pdfToImages" ? job.results : {};
  const partialCount = Object.values(results).filter(
    (result) => result.status === "partial",
  ).length;
  // The single-PDF view has no table, so its result goes under its name.
  const singleStatus =
    singleItem !== null && singleItem.error === null
      ? describeItemStatus(singleItem, job, t, language.language)
      : null;
  const singleItemJobStatus =
    singleItem !== null && job.kind === "pdfToImages"
      ? jobRowStatus(job, singleItem.id)
      : null;
  const showSingleResult =
    singleStatus !== null && singleItemJobStatus !== "partial";
  const singleResult = singleItem !== null ? results[singleItem.id] : undefined;
  const isPageSelected = (page: number) =>
    displayed.pageSelection === "all" ||
    (displayed.rangeResult !== null &&
      isPageInIntervals(displayed.rangeResult.intervals, page));
  const singleOutcomes =
    singleItem !== null && singleResult !== undefined
      ? pageOutcomes(
          Array.from({ length: singleItem.pageCount }, (_, i) => i + 1).filter(
            isPageSelected,
          ),
          singleResult.outputs.length,
          singleResult.failedPages ?? [],
        )
      : null;

  return (
    <div className="pdf-view">
      {error !== null && (
        <ErrorDisplay
          message={formatErrorMessage(
            error.code,
            error.detail,
            language.language,
          )}
          onDismiss={() => setError(null)}
          dismissLabel={t.errors.dismiss}
        />
      )}
      <OutputDirError tab="pdfToImages" />
      <JobSummaryBanner tab="pdfToImages" partialCount={partialCount} />

      {/* Header above content */}
      <div className="pdf-view-header">
        {isSingle && singleItem !== null ? (
          <>
            <span className="pdf-single-name">{singleItem.name}</span>
            {singleItem.error !== null ? (
              <span className="pdf-status-failed">{t.job.loadFailed}</span>
            ) : (
              <span className="mono">
                {formatMessage(t.pdfToImages.single.meta, {
                  pages: singleItem.pageCount,
                  size: formatPaperSize(singleItem.firstPageSizePt),
                  fileSize: formatFileSize(singleItem.bytes),
                })}
              </span>
            )}
            {showSingleResult && (
              <span className="pdf-single-result" data-testid="single-result">
                <span className={singleStatus.className}>
                  {singleStatus.text}
                </span>
                {singleStatus.reason !== null && (
                  <span className="hint">{singleStatus.reason}</span>
                )}
              </span>
            )}
            <div className="pdf-view-actions">
              <button
                type="button"
                className="btn btn-ghost"
                disabled={busy}
                onClick={() => void handleAdd("files")}
              >
                {t.dropZone.addPdfs}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                disabled={busy}
                onClick={() => void handleAdd("folder")}
              >
                {t.dropZone.addFolder}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                disabled={busy}
                onClick={() => void handleRemove(singleItem.id)}
              >
                {t.pdfToImages.remove}
              </button>
            </div>
          </>
        ) : (
          <>
            <span className="label">
              {formatMessage(t.pdfToImages.batch.pdfCount, {
                count: pdfToImages.items.length,
              })}
            </span>
            {skippedText !== null && (
              <span className="hint">{skippedText}</span>
            )}
            <div className="pdf-view-actions">
              <button
                type="button"
                className="btn btn-ghost"
                disabled={busy}
                onClick={() => void handleAdd("files")}
              >
                {t.dropZone.addPdfs}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                disabled={busy}
                onClick={() => void handleAdd("folder")}
              >
                {t.dropZone.addFolder}
              </button>
              <button
                type="button"
                className="btn btn-ghost"
                disabled={busy}
                onClick={() => void handleClearAll()}
              >
                {t.pdfToImages.clearAll}
              </button>
            </div>
          </>
        )}
      </div>

      {/* Body: Single PDF thumbnail grid OR Batch PDFs table */}
      {isSingle && singleItem !== null ? (
        singleItem.error !== null ? (
          // The reason, once; the header says only that it could not be read.
          <ErrorDisplay
            message={formatErrorMessage(
              singleItem.error.code,
              singleItem.error.detail,
              language.language,
            )}
          />
        ) : (
          <div className="pdf-single-grid">
            {Array.from({ length: singleItem.pageCount }, (_, i) => i + 1).map(
              (page) => {
                const isHighlighted = isPageSelected(page);
                return (
                  <PdfPageThumbnail
                    key={page}
                    id={singleItem.id}
                    page={page}
                    name={singleItem.name}
                    firstPageSizePt={singleItem.firstPageSizePt}
                    isHighlighted={isHighlighted}
                    outcome={singleOutcomes?.get(page) ?? null}
                    disabled={isThumbnailDisabled}
                    onClick={handleThumbnailClick}
                    translations={t}
                  />
                );
              },
            )}
          </div>
        )
      ) : (
        <div className="pdf-table-container">
          <table className="pdf-table">
            <thead>
              <tr>
                <th>{t.pdfToImages.batch.table.filename}</th>
                <th className="pdf-col-pages">
                  {t.pdfToImages.batch.table.pages}
                </th>
                <th>{t.pdfToImages.batch.table.savedFiles}</th>
                <th className="pdf-col-status">
                  {t.pdfToImages.batch.table.status}
                </th>
                <th className="pdf-col-actions">
                  {t.pdfToImages.batch.table.actions}
                </th>
              </tr>
            </thead>
            <tbody>
              {pdfToImages.items.map((item) => (
                <PdfTableRow
                  key={item.id}
                  item={item}
                  job={job}
                  pageSelection={displayed.pageSelection}
                  rangeResult={displayed.rangeResult}
                  onRemove={() => void handleRemove(item.id)}
                  disabled={busy}
                  translations={t}
                  language={language.language}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
