import { useCallback, useEffect, useId, useRef } from "react";
import {
  DropZone,
  ErrorDisplay,
  ListRow,
  type ListRowStatus,
} from "../../components";
import { OutputDirError } from "../output";
import {
  formatIpcError,
  formatMessage,
  pluralize,
  getTranslations,
  type Language,
  type Translations,
} from "../../i18n";
import { addImages, type ImageItem, type JobItemPayload } from "../../ipc";
import {
  isJobActive,
  jobRowStatus,
  useAppDispatch,
  useAppState,
  type JobRowStatus,
  type JobState,
} from "../../state";
import { useItemList } from "../items/useItemList";
import { useThumbnail } from "../items/useThumbnail";
import { rowDropClass, rowIndexOf, useRowDrag } from "./useRowDrag";
import { formatSkippedSummary } from "../shared/skipped";
import "./ImagesToPdf.css";

function formatBytes(bytes: number): string {
  const oneKb = 1024;
  const oneMb = 1024 * 1024;
  if (bytes >= oneMb) {
    return `${(bytes / oneMb).toFixed(1)} MB`;
  }
  if (bytes >= 100 * oneKb) {
    return `${Math.round(bytes / oneKb)} KB`;
  }
  if (bytes >= oneKb) {
    const kb = bytes / oneKb;
    return `${kb >= 10 ? Math.round(kb) : kb.toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

function formatMeta(item: ImageItem, t: Translations): string | undefined {
  if (item.error !== null || item.format === null) {
    return undefined;
  }
  const formatMap: Record<string, string> = {
    png: "PNG",
    jpeg: "JPEG",
    webp: "WebP",
    bmp: "BMP",
    tiff: "TIFF",
  };
  const formatName = formatMap[item.format] ?? item.format.toUpperCase();
  const parts = [`${item.width} × ${item.height}`, formatName];
  if (item.format === "tiff" && item.pageCount >= 2) {
    parts.push(
      formatMessage(t.imagesToPdf.pageCount, { count: item.pageCount }),
    );
  }
  parts.push(formatBytes(item.bytes));
  return parts.join(" · ");
}

function toListRowStatus(
  status: JobRowStatus | null,
  t: Translations,
): ListRowStatus | null {
  if (status === null) {
    return null;
  }
  const text = t.job.rowStatus[status];
  switch (status) {
    case "ok":
      return { text, kind: "ok" };
    case "failed":
    case "partial":
      return { text, kind: "failed" };
    case "running":
      return { text, kind: "running" };
    default:
      return { text, kind: "waiting" };
  }
}

/**
 * Why item `item` failed: why it could not be read when it was added, or why
 * the last conversion failed on it. `null` when it did not fail.
 */
function failureReason(
  item: ImageItem,
  result: JobItemPayload | undefined,
  language: Language,
): string | null {
  const error = item.error ?? result?.error ?? null;
  return error === null ? null : formatIpcError(error, language);
}

/**
 * The row status of item `item`: the conversion's, or that it could not be
 * read. A merged PDF is written only once every page is in, so after a
 * cancel the pages already added were not saved and read as cancelled.
 */
function imageRowStatus(
  item: ImageItem,
  job: JobState,
  isMerge: boolean,
  t: Translations,
): ListRowStatus | null {
  if (job.kind === "imagesToPdf") {
    let status = jobRowStatus(job, item.id);
    if (isMerge && job.finished?.cancelled === true && status === "ok") {
      status = "cancelled";
    }
    const listStatus = toListRowStatus(status, t);
    if (listStatus !== null) {
      return listStatus;
    }
  }
  return item.error !== null
    ? { text: t.job.loadFailed, kind: "failed" }
    : null;
}

function ImageThumbnail({ id, name }: { id: number; name: string }) {
  const { language } = useAppState();
  const t = getTranslations(language.language);
  const { ref: attachRef, src: imageSrc, failed } = useThumbnail(id);
  const altText = formatMessage(t.listRow.thumbnailAlt, { name });

  // While loading the tile stays blank: an icon there would flash before
  // the image replaces it, every time the list or the table is shown again.
  // The icon marks only a thumbnail that could not be made.
  return (
    <div ref={attachRef} className="image-thumb-container">
      {imageSrc ? (
        <img src={imageSrc} alt={altText} />
      ) : !failed ? (
        <div role="img" aria-label={altText} />
      ) : (
        <div role="img" aria-label={altText}>
          <svg
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <circle cx="8.5" cy="8.5" r="1.5" />
            <path d="M21 15l-5-5L5 21" />
          </svg>
        </div>
      )}
    </div>
  );
}

/**
 * Main view area for Images to PDF (design §6.1, §6.2, mockups Main, ImagesMerge, ImagesEach).
 * Shows DropZone when empty, ordered ListRow list when output is "merge",
 * and conversion table when output is "each".
 */
export function ImagesToPdfView() {
  const { imagesToPdf, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language);
  const disabled = isJobActive(job);
  const headingId = useId();

  const list = useItemList({
    items: imagesToPdf.items,
    add: addImages,
    onAdded: (items) => dispatch({ type: "ADD_IMAGE_ITEMS", items }),
    onRemoved: (id) => dispatch({ type: "REMOVE_IMAGE_ITEM", id }),
    onCleared: () => dispatch({ type: "CLEAR_IMAGE_ITEMS" }),
  });

  const listRef = useRef<HTMLOListElement | null>(null);
  const focusedItemIdRef = useRef<number | null>(null);
  const rowRefs = useRef<Map<number, HTMLLIElement>>(new Map());
  const moveItem = useCallback(
    (fromIndex: number, toIndex: number) =>
      dispatch({ type: "MOVE_IMAGE_ITEM", fromIndex, toIndex }),
    [dispatch],
  );
  const { dragIndex, overIndex, handlers } = useRowDrag(
    listRef,
    disabled,
    moveItem,
  );

  const handleKeyDown = (e: React.KeyboardEvent<HTMLOListElement>) => {
    if (
      disabled ||
      !e.altKey ||
      (e.key !== "ArrowUp" && e.key !== "ArrowDown")
    ) {
      return;
    }
    const i = rowIndexOf(listRef.current, e.target);
    if (i === null) {
      return;
    }

    if (e.key === "ArrowUp" && i > 0) {
      e.preventDefault();
      focusedItemIdRef.current = imagesToPdf.items[i].id;
      moveItem(i, i - 1);
    } else if (e.key === "ArrowDown" && i < imagesToPdf.items.length - 1) {
      e.preventDefault();
      focusedItemIdRef.current = imagesToPdf.items[i].id;
      moveItem(i, i + 1);
    }
  };

  // Restore focus to moved row using refs after keyboard reordering
  useEffect(() => {
    if (focusedItemIdRef.current !== null) {
      const targetElement = rowRefs.current.get(focusedItemIdRef.current);
      if (targetElement) {
        targetElement.focus();
      }
      focusedItemIdRef.current = null;
    }
  });

  const errorDisplay = list.error !== null && (
    <ErrorDisplay
      message={formatIpcError(list.error, language.language)}
      onDismiss={list.dismissError}
      dismissLabel={t.errors.dismiss}
    />
  );

  if (imagesToPdf.items.length === 0) {
    return (
      <div className="images-view">
        {errorDisplay}
        <DropZone
          title={t.dropZone.titleImages}
          description={t.dropZone.descriptionImages}
          addFilesLabel={t.dropZone.addImages}
          addFolderLabel={t.dropZone.addFolder}
          onAddFiles={() => void list.add("files")}
          onAddFolder={() => void list.add("folder")}
        />
      </div>
    );
  }

  const isMerge = imagesToPdf.output === "merge";
  const skippedText = formatSkippedSummary(t, list.skipped);

  return (
    <div className="images-view">
      {errorDisplay}
      <OutputDirError tab="imagesToPdf" />

      <div className="images-view-header">
        <div className="images-view-title-group">
          <span className="label" id={headingId}>
            {pluralize(
              imagesToPdf.items.length,
              t.imagesToPdf.imageCountSingle,
              t.imagesToPdf.imageCountMultiple,
            )}
          </span>
          {skippedText ? <span className="hint">{skippedText}</span> : null}
        </div>
        <div className="images-view-actions">
          <button
            type="button"
            className="btn btn-ghost"
            disabled={disabled}
            onClick={() => void list.add("files")}
          >
            {t.imagesToPdf.addImages}
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={disabled}
            onClick={() => void list.add("folder")}
          >
            {t.imagesToPdf.addFolder}
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={disabled}
            onClick={() => void list.clearAll()}
          >
            {t.imagesToPdf.clearAll}
          </button>
        </div>
      </div>

      {isMerge ? (
        <>
          <ol
            ref={listRef}
            className={`images-list ${disabled ? "is-disabled" : ""} ${dragIndex !== null ? "is-dragging-active" : ""}`}
            aria-labelledby={headingId}
            {...handlers}
            onKeyDown={handleKeyDown}
          >
            {imagesToPdf.items.map((item, index) => {
              const status = imageRowStatus(item, job, isMerge, t);
              const reason = failureReason(
                item,
                job.kind === "imagesToPdf" ? job.results[item.id] : undefined,
                language.language,
              );

              return (
                <ListRow
                  key={item.id}
                  ref={(el) => {
                    if (el) {
                      rowRefs.current.set(item.id, el);
                    } else {
                      rowRefs.current.delete(item.id);
                    }
                  }}
                  index={index + 1}
                  title={item.name}
                  meta={formatMeta(item, t)}
                  customThumbnail={
                    <ImageThumbnail id={item.id} name={item.name} />
                  }
                  error={reason}
                  status={status}
                  className={rowDropClass(index, dragIndex, overIndex)}
                  data-row-index={index}
                  tabIndex={disabled ? undefined : 0}
                  onMoveUp={
                    disabled ? undefined : () => moveItem(index, index - 1)
                  }
                  onMoveDown={
                    disabled ? undefined : () => moveItem(index, index + 1)
                  }
                  onRemove={
                    disabled ? undefined : () => void list.remove(item.id)
                  }
                  canMoveUp={!disabled && index > 0}
                  canMoveDown={
                    !disabled && index < imagesToPdf.items.length - 1
                  }
                  labels={{
                    moveUp: t.listRow.moveUp,
                    moveDown: t.listRow.moveDown,
                    remove: t.listRow.remove,
                  }}
                  showDragHandle={!disabled}
                />
              );
            })}
          </ol>
        </>
      ) : (
        <div className="images-table-container">
          <table className="images-table">
            <thead>
              <tr>
                <th>{t.imagesToPdf.tableFileName}</th>
                <th>{t.imagesToPdf.tableSavedName}</th>
                <th className="images-table-status-col">
                  {t.imagesToPdf.tableStatus}
                </th>
                <th className="images-table-actions-col">
                  {t.imagesToPdf.tableActions}
                </th>
              </tr>
            </thead>
            <tbody>
              {imagesToPdf.items.map((item) => {
                const result =
                  job.kind === "imagesToPdf" ? job.results[item.id] : undefined;
                const outputName =
                  result?.outputs && result.outputs.length > 0
                    ? result.outputs[0]
                    : t.imagesToPdf.noOutputYet;

                const listStatus = imageRowStatus(item, job, isMerge, t);
                const reason = failureReason(item, result, language.language);

                return (
                  <tr key={item.id}>
                    <td>
                      <div className="images-table-name-cell">
                        <div className="images-table-thumb">
                          <ImageThumbnail id={item.id} name={item.name} />
                        </div>
                        <span>{item.name}</span>
                      </div>
                    </td>
                    <td className="mono">{outputName}</td>
                    <td
                      className={
                        listStatus
                          ? `images-table-status status-${listStatus.kind}`
                          : "images-table-status"
                      }
                    >
                      {listStatus?.text}
                      {reason !== null && (
                        <div className="images-table-reason">{reason}</div>
                      )}
                    </td>
                    <td className="images-table-actions-col">
                      <button
                        type="button"
                        className="btn btn-ghost"
                        disabled={disabled}
                        onClick={() => void list.remove(item.id)}
                      >
                        {t.imagesToPdf.remove}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
