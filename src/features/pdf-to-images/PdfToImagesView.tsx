import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import {
  DropZone,
  ErrorDisplay,
  LockIcon,
  NoticeDisplay,
  PdfFileIcon,
} from "../../components";
import { PasswordDialog } from "./PasswordDialog";
import {
  formatIpcError,
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
  type JobItemPayload,
  type PageSizePt,
  type PdfItem,
  type Skipped,
  unlockPdf,
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
import { OutputDirError } from "../output";
import { useRoom } from "../shared/useRoom";
import {
  LIST_THUMBNAIL_DELAY_MS,
  rowOffsets,
  useVirtualRows,
} from "../shared/virtualRows";
import { formatSkippedSummary } from "../shared/skipped";
import {
  countPagesInIntervals,
  formatFileSize,
  formatPaperSize,
  failedAsAWhole,
  intervalsToRangeText,
  isPageInIntervals,
  selectedIntervals,
  pageOutcomes,
  togglePageRangeInIntervals,
  type PageOutcome,
} from "./pdfUtils";
import { pdfThumbnails } from "./pdfThumbnails";
import { useDisplayedRange } from "./useDisplayedRange";
import "./pdfToImages.css";

/** Columns of the page grid; `.pdf-single-rows` lays them out. */
const PDF_GRID_COLUMNS = 4;

/** Between the cells of the grid, both ways; `.pdf-single-rows` has it too. */
const PDF_GRID_GAP = 16;

/** Between a page and its caption; `.pdf-page-figure` has it too. */
const PDF_CAPTION_GAP = 6;

/** The caption's one line, which `.pdf-page-figure figcaption` fixes. */
const PDF_CAPTION_HEIGHT = 16;

/** The shape of a page whose size is not known. */
const DEFAULT_PAGE_SIZE_PT: PageSizePt = { widthPt: 210, heightPt: 297 };

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
  const { ref, src, failed } = useThumbnail(id, page, {
    delayMs: LIST_THUMBNAIL_DELAY_MS,
    cache: pdfThumbnails,
  });
  const isFailed = outcome === "failed";

  const { widthPt, heightPt } = firstPageSizePt ?? DEFAULT_PAGE_SIZE_PT;
  const aspectRatio = `${widthPt} / ${heightPt}`;

  return (
    <figure
      className={`pdf-page-figure ${!isHighlighted ? "is-dimmed" : ""}`}
      data-testid={`page-thumbnail-${page}`}
      data-virtual-row={Math.floor((page - 1) / PDF_GRID_COLUMNS)}
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

interface PdfPageGridProps {
  containerRef: RefObject<HTMLDivElement | null>;
  item: PdfItem;
  isPageSelected: (page: number) => boolean;
  outcomes: ReadonlyMap<number, PageOutcome> | null;
  disabled: boolean;
  onPageClick: (page: number, shiftKey: boolean) => void;
  translations: Translations;
}

/**
 * The pages of a single PDF. Every page is drawn in the shape of the first,
 * so the rows are all one height and only those near the view are drawn: ten
 * thousand pages cost as much as a hundred.
 */
function PdfPageGrid({
  containerRef,
  item,
  isPageSelected,
  outcomes,
  disabled,
  onPageClick,
  translations,
}: PdfPageGridProps) {
  const room = useRoom(containerRef);
  const { pageCount, firstPageSizePt } = item;
  const offsets = useMemo(() => {
    const { widthPt, heightPt } = firstPageSizePt ?? DEFAULT_PAGE_SIZE_PT;
    const cellWidth =
      (room.width - (PDF_GRID_COLUMNS - 1) * PDF_GRID_GAP) / PDF_GRID_COLUMNS;
    const height =
      Math.max(cellWidth, 0) * (heightPt / widthPt) +
      PDF_CAPTION_GAP +
      PDF_CAPTION_HEIGHT;
    return rowOffsets(
      Math.ceil(pageCount / PDF_GRID_COLUMNS),
      () => height,
      PDF_GRID_GAP,
    );
  }, [pageCount, firstPageSizePt, room.width]);
  const { first, last, before, after, onScroll } = useVirtualRows(
    containerRef,
    room,
    offsets,
  );

  const pages: number[] = [];
  const end = Math.min((last + 1) * PDF_GRID_COLUMNS, pageCount);
  for (let page = first * PDF_GRID_COLUMNS + 1; page <= end; page += 1) {
    pages.push(page);
  }

  return (
    <div
      ref={containerRef}
      className="pdf-single-grid"
      tabIndex={-1}
      onScroll={onScroll}
    >
      <div
        className="pdf-single-rows"
        style={{ paddingTop: before, paddingBottom: after }}
      >
        {pages.map((page) => (
          <PdfPageThumbnail
            key={page}
            id={item.id}
            page={page}
            name={item.name}
            firstPageSizePt={firstPageSizePt}
            isHighlighted={isPageSelected(page)}
            outcome={outcomes?.get(page) ?? null}
            disabled={disabled}
            onClick={onPageClick}
            translations={translations}
          />
        ))}
      </div>
    </div>
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
  if (item.error?.code === "PasswordProtected") {
    // Waiting for input, not an error: it opens once the password is in.
    return {
      text: t.pdfToImages.passwordRequired,
      className: "pdf-status-locked",
      reason: null,
    };
  }
  if (item.error !== null) {
    return {
      text: t.job.loadFailed,
      className: "pdf-status-failed",
      reason: formatIpcError(item.error, language),
    };
  }
  if (job.kind !== "pdfToImages") {
    return null;
  }
  const status = jobRowStatus(job, item.id);
  if (status === null) {
    return null;
  }
  const text = t.job.rowStatus[status];
  const result: JobItemPayload | undefined = job.results[item.id];
  switch (status) {
    case "ok":
      return { text, className: "pdf-status-ok", reason: null };
    case "failed":
      return {
        text,
        className: "pdf-status-failed",
        reason:
          result?.error === undefined
            ? null
            : formatIpcError(result.error, language),
      };
    case "partial":
      return {
        text,
        className: "pdf-status-partial",
        reason:
          result?.failedPages === undefined || result.failedPages.length === 0
            ? null
            : formatMessage(t.pdfToImages.batch.failedPagesReason, {
                pages: result.failedPages.join(", "),
              }),
      };
    case "cancelled":
      return {
        text,
        className: "pdf-status-neutral",
        reason: formatMessage(t.pdfToImages.batch.cancelledSavedReason, {
          count: result?.outputs.length ?? 0,
        }),
      };
    case "noPages":
      return {
        text,
        className: "pdf-status-neutral",
        reason: t.pdfToImages.batch.noPagesReason,
      };
    case "running":
      return { text, className: "pdf-status-running", reason: null };
    case "waiting":
    case "unprocessed":
      return { text, className: "pdf-status-muted", reason: null };
  }
}

/**
 * The table's pages column: how many pages the last conversion saved, or how
 * many the next one will take, out of all the pages of the PDF.
 */
function pagesText(
  item: PdfItem,
  result: JobItemPayload | undefined,
  pageSelection: PageSelection,
  rangeResult: CheckPageRangeResult | null,
): string {
  if (item.error !== null) {
    return "—";
  }
  const ofAll = (count: number | null) => `${count ?? "—"} / ${item.pageCount}`;
  if (result !== undefined) {
    return ofAll(
      result.status === "noPages" || result.status === "failed"
        ? null
        : result.outputs.length,
    );
  }
  if (pageSelection === "all") {
    return ofAll(item.pageCount);
  }
  if (rangeResult === null) {
    return ofAll(null);
  }
  const count = countPagesInIntervals(
    selectedIntervals(rangeResult, item.pageCount),
    item.pageCount,
  );
  return ofAll(count > 0 ? count : null);
}

/** The table's saved-files column: the first file, and "…" when there are more. */
function savedFilesText(result: JobItemPayload | undefined): string {
  const outputs = result?.outputs ?? [];
  if (outputs.length === 0) {
    return "—";
  }
  return outputs.length === 1 ? outputs[0] : `${outputs[0]} …`;
}

interface PdfTableRowProps {
  item: PdfItem;
  job: JobState;
  pageSelection: PageSelection;
  rangeResult: CheckPageRangeResult | null;
  onRemove: () => void;
  onEnterPassword: (item: PdfItem, triggerEl: HTMLElement) => void;
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
  onEnterPassword,
  disabled,
  translations: t,
  language,
}: PdfTableRowProps) {
  const result = job.kind === "pdfToImages" ? job.results[item.id] : undefined;
  const described = describeItemStatus(item, job, t, language);
  const statusText = described?.text ?? "—";
  const statusClass = described?.className ?? "";
  const reasonText = described?.reason ?? null;

  return (
    <tr>
      <td>{item.name}</td>
      <td className="mono pdf-col-pages">
        {pagesText(item, result, pageSelection, rangeResult)}
      </td>
      <td className="mono">{savedFilesText(result)}</td>
      <td className={statusClass}>
        {item.error?.code === "PasswordProtected" && (
          <LockIcon size={13} className="pdf-status-icon" />
        )}
        {statusText}
        {reasonText !== null && (
          <div className="pdf-status-reason">{reasonText}</div>
        )}
        {item.error?.code === "PasswordProtected" && (
          <div className="pdf-status-action">
            <button
              type="button"
              className="pdf-status-password"
              disabled={disabled}
              onClick={(e) => onEnterPassword(item, e.currentTarget)}
            >
              {t.pdfToImages.enterPassword}
            </button>
          </div>
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

  const [passwordTargetItem, setPasswordTargetItem] = useState<PdfItem | null>(
    null,
  );
  const passwordTriggerRef = useRef<HTMLElement | null>(null);
  const listContainerRef = useRef<HTMLDivElement>(null);
  const shouldFocusListRef = useRef(false);

  useEffect(() => {
    if (shouldFocusListRef.current) {
      shouldFocusListRef.current = false;
      listContainerRef.current?.focus();
    }
  });

  function handleOpenPassword(item: PdfItem, triggerEl: HTMLElement) {
    passwordTriggerRef.current = triggerEl;
    setPasswordTargetItem(item);
  }

  function handleUnlockSuccess(items: PdfItem[]) {
    shouldFocusListRef.current = true;
    dispatch({ type: "REPLACE_PDF_ITEMS", items });
    setPasswordTargetItem(null);
  }

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
            message={formatIpcError(error, language.language)}
            onDismiss={() => setError(null)}
            dismissLabel={t.errors.dismiss}
          />
        )}
        <DropZone
          icon={<PdfFileIcon />}
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
      pdfToImages.rangeResult !== null
        ? selectedIntervals(pdfToImages.rangeResult, pageCount)
        : [];
    const isCurrentlySelected = isPageInIntervals(currentIntervals, page);
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
  const singleIntervals =
    singleItem !== null && displayed.rangeResult !== null
      ? selectedIntervals(displayed.rangeResult, singleItem.pageCount)
      : [];
  const isPageSelected = (page: number) =>
    displayed.pageSelection === "all" ||
    isPageInIntervals(singleIntervals, page);
  // Only a result needs every page gone through.
  let singleOutcomes: ReadonlyMap<number, PageOutcome> | null = null;
  if (singleItem !== null && singleResult !== undefined) {
    const selectedPages = Array.from(
      { length: singleItem.pageCount },
      (_, i) => i + 1,
    ).filter(isPageSelected);
    singleOutcomes = pageOutcomes(
      selectedPages,
      singleResult.outputs.length,
      failedAsAWhole(singleResult)
        ? selectedPages
        : (singleResult.failedPages ?? []),
    );
  }

  return (
    <div className="pdf-view">
      {error !== null && (
        <ErrorDisplay
          message={formatIpcError(error, language.language)}
          onDismiss={() => setError(null)}
          dismissLabel={t.errors.dismiss}
        />
      )}
      <OutputDirError tab="pdfToImages" />

      {/* Header above content */}
      <div className="pdf-view-header">
        {isSingle && singleItem !== null ? (
          <>
            <span className="pdf-single-name">{singleItem.name}</span>
            {singleItem.error?.code === "PasswordProtected" ? (
              <span className="pdf-status-locked">
                <LockIcon size={13} className="pdf-status-icon" />
                {t.pdfToImages.passwordRequired}
              </span>
            ) : singleItem.error !== null ? (
              <span className="pdf-status-failed">{t.job.loadFailed}</span>
            ) : (
              <span className="mono">
                {formatMessage(
                  singleItem.pageCount === 1
                    ? t.pdfToImages.single.metaSingle
                    : t.pdfToImages.single.metaMultiple,
                  {
                    pages: singleItem.pageCount,
                    size: formatPaperSize(singleItem.firstPageSizePt),
                    fileSize: formatFileSize(singleItem.bytes),
                  },
                )}
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
          </>
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
          {singleItem !== null ? (
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              onClick={() => void handleRemove(singleItem.id)}
            >
              {t.pdfToImages.remove}
            </button>
          ) : (
            <button
              type="button"
              className="btn btn-ghost"
              disabled={busy}
              onClick={() => void handleClearAll()}
            >
              {t.pdfToImages.clearAll}
            </button>
          )}
        </div>
      </div>

      {/* Body: Single PDF thumbnail grid OR Batch PDFs table */}
      {isSingle && singleItem !== null ? (
        singleItem.error?.code === "PasswordProtected" ? (
          <NoticeDisplay
            message={formatIpcError(singleItem.error, language.language)}
            action={
              <button
                type="button"
                className="btn btn-ghost"
                disabled={busy}
                onClick={(e) => handleOpenPassword(singleItem, e.currentTarget)}
              >
                {t.pdfToImages.enterPassword}
              </button>
            }
          />
        ) : singleItem.error !== null ? (
          // The reason, once; the header says only that it could not be read.
          <ErrorDisplay
            message={formatIpcError(singleItem.error, language.language)}
          />
        ) : (
          <PdfPageGrid
            key={singleItem.id}
            containerRef={listContainerRef}
            item={singleItem}
            isPageSelected={isPageSelected}
            outcomes={singleOutcomes}
            disabled={isThumbnailDisabled}
            onPageClick={handleThumbnailClick}
            translations={t}
          />
        )
      ) : (
        <div
          ref={listContainerRef}
          className="pdf-table-container"
          tabIndex={-1}
        >
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
                  onEnterPassword={handleOpenPassword}
                  disabled={busy}
                  translations={t}
                  language={language.language}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {passwordTargetItem !== null && (
        <PasswordDialog
          name={passwordTargetItem.name}
          unlock={(password) => unlockPdf(passwordTargetItem.id, password)}
          onClose={() => setPasswordTargetItem(null)}
          onSuccess={(result) => handleUnlockSuccess(result.items)}
          triggerRef={passwordTriggerRef}
        />
      )}
    </div>
  );
}
