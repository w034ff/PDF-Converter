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
import { formatJobSummary } from "../job/jobSummary";
import {
  countPagesInIntervals,
  formatFileSize,
  formatPaperSize,
  formatSkippedSummary,
  isPageInIntervals,
} from "./pdfUtils";
import "./pdfToImages.css";

interface PdfPageThumbnailProps {
  id: number;
  page: number;
  name: string;
  firstPageSizePt: PageSizePt | null;
  isHighlighted: boolean;
  translations: Translations;
}

function PdfPageThumbnail({
  id,
  page,
  name,
  firstPageSizePt,
  isHighlighted,
  translations: t,
}: PdfPageThumbnailProps) {
  const { ref, src, failed } = useThumbnail(id, page);

  const aspectRatio = firstPageSizePt
    ? `${firstPageSizePt.widthPt} / ${firstPageSizePt.heightPt}`
    : "210 / 297";

  return (
    <figure
      className={`pdf-page-figure ${!isHighlighted ? "is-dimmed" : ""}`}
      data-testid={`page-thumbnail-${page}`}
    >
      <div
        ref={ref}
        className={`pdf-page-container ${isHighlighted ? "is-highlighted" : ""}`}
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
          <div className="pdf-page-placeholder">
            {failed ? <span className="hint">✕</span> : null}
          </div>
        )}
      </div>
      <figcaption className="mono">
        {isHighlighted
          ? formatMessage(t.pdfToImages.single.convertPage, { page })
          : String(page)}
      </figcaption>
    </figure>
  );
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

  let statusText = "—";
  let statusClass = "";
  let reasonText: string | null = null;

  if (item.error !== null) {
    statusText = t.job.rowStatus.failed;
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

  return (
    <tr>
      <td>{item.name}</td>
      <td className="mono">{pagesText}</td>
      <td className="mono">{savedFilesText}</td>
      <td className={statusClass}>
        {statusText}
        {reasonText !== null && (
          <div className="pdf-status-reason">{reasonText}</div>
        )}
      </td>
      <td>
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

  const isSingle = pdfToImages.items.length === 1;
  const singleItem = isSingle ? pdfToImages.items[0] : null;
  const skippedText = formatSkippedSummary(t, lastSkipped);

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
      {/* Top summary banner when conversion ends (design §6.3, mockup PdfBatch) */}
      {job.kind === "pdfToImages" && job.finished !== null && (
        <div role="status" className="pdf-summary-banner">
          <span className="pdf-summary-title">
            {formatJobSummary(t, job.finished)}
          </span>
          {job.finished.failed > 0 && (
            <span className="hint">{t.pdfToImages.summaryFailuresHint}</span>
          )}
        </div>
      )}

      {/* Header above content */}
      <div className="pdf-view-header">
        {isSingle && singleItem !== null ? (
          <>
            <span className="pdf-single-name">{singleItem.name}</span>
            {singleItem.error !== null ? (
              <span className="hint" role="alert">
                {formatErrorMessage(
                  singleItem.error.code,
                  singleItem.error.detail,
                  language.language,
                )}
              </span>
            ) : (
              <span className="mono">
                {formatMessage(t.pdfToImages.single.meta, {
                  pages: singleItem.pageCount,
                  size: formatPaperSize(singleItem.firstPageSizePt),
                  fileSize: formatFileSize(singleItem.bytes),
                })}
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
          <div className="pdf-summary-banner" role="alert">
            <span className="pdf-status-failed">
              {formatErrorMessage(
                singleItem.error.code,
                singleItem.error.detail,
                language.language,
              )}
            </span>
          </div>
        ) : (
          <div className="pdf-single-grid">
            {Array.from({ length: singleItem.pageCount }, (_, i) => i + 1).map(
              (page) => {
                const isHighlighted =
                  pdfToImages.pageSelection === "all" ||
                  (pdfToImages.rangeResult !== null &&
                    isPageInIntervals(pdfToImages.rangeResult.intervals, page));
                return (
                  <PdfPageThumbnail
                    key={page}
                    id={singleItem.id}
                    page={page}
                    name={singleItem.name}
                    firstPageSizePt={singleItem.firstPageSizePt}
                    isHighlighted={isHighlighted}
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
                  pageSelection={pdfToImages.pageSelection}
                  rangeResult={pdfToImages.rangeResult}
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
