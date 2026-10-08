import { useEffect, useId, useRef, useState } from "react";
import {
  DropZone,
  ErrorDisplay,
  ListRow,
  type ListRowStatus,
} from "../../components";
import { JobSummaryBanner } from "../job/JobSummaryBanner";
import { OutputDirError } from "../output";
import {
  formatErrorMessage,
  formatMessage,
  getTranslations,
  type Language,
  type Translations,
} from "../../i18n";
import {
  addImages,
  normalizeIpcError,
  removeItems,
  type AddSource,
  type ImageItem,
  type IpcError,
  type JobItemPayload,
  type Skipped,
} from "../../ipc";
import {
  isJobActive,
  jobRowStatus,
  useAppDispatch,
  useAppState,
  type JobRowStatus,
  type JobState,
} from "../../state";
import { useThumbnail } from "../items/useThumbnail";
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

function formatMeta(item: ImageItem): string | undefined {
  if (item.error !== null || item.format === null) {
    return undefined;
  }
  const formatMap: Record<string, string> = {
    png: "PNG",
    jpeg: "JPEG",
    webp: "WebP",
    bmp: "BMP",
  };
  const formatName = formatMap[item.format] ?? item.format.toUpperCase();
  return `${item.width} × ${item.height} · ${formatName} · ${formatBytes(item.bytes)}`;
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
  return error === null
    ? null
    : formatErrorMessage(error.code, error.detail, language);
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

function formatSkippedMessage(
  skipped: Skipped | null,
  t: Translations,
): string | null {
  if (!skipped) {
    return null;
  }
  const total = skipped.unsupported + skipped.folders + skipped.duplicates;
  if (total <= 0) {
    return null;
  }
  const reasons: string[] = [];
  if (skipped.folders > 0) {
    reasons.push(t.imagesToPdf.skippedFolders);
  }
  if (skipped.unsupported > 0) {
    reasons.push(t.imagesToPdf.skippedUnsupported);
  }
  if (skipped.duplicates > 0) {
    reasons.push(t.imagesToPdf.skippedDuplicates);
  }
  return formatMessage(t.imagesToPdf.skipped, {
    count: total,
    reasons: reasons.join(t.imagesToPdf.skippedSeparator),
  });
}

function findRowElement(target: EventTarget | null): HTMLElement | null {
  if (target instanceof Element) {
    const row = target.closest(".list-row");
    if (row instanceof HTMLElement) {
      return row;
    }
  }
  return null;
}

function findRowFromPoint(x: number, y: number): HTMLElement | null {
  const el = document.elementFromPoint?.(x, y);
  return findRowElement(el);
}

function getRowDropClass(
  index: number,
  dragIdx: number | null,
  overIdx: number | null,
): string | undefined {
  if (dragIdx === null) {
    return undefined;
  }
  if (index === dragIdx) {
    return "is-dragging";
  }
  if (overIdx !== null && index === overIdx) {
    if (dragIdx < overIdx) {
      return "is-drop-after";
    }
    if (dragIdx > overIdx) {
      return "is-drop-before";
    }
  }
  return undefined;
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

  const [skipped, setSkipped] = useState<Skipped | null>(null);
  const [error, setError] = useState<IpcError | null>(null);

  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);

  const dragIndexRef = useRef<number | null>(null);
  const overIndexRef = useRef<number | null>(null);
  const listRef = useRef<HTMLOListElement | null>(null);
  const focusedItemIdRef = useRef<number | null>(null);
  const rowRefs = useRef<Map<number, HTMLLIElement>>(new Map());

  async function handleAdd(source: AddSource) {
    setError(null);
    try {
      const result = await addImages(source);
      if (result !== null) {
        if (result.added.length > 0) {
          dispatch({ type: "ADD_IMAGE_ITEMS", items: result.added });
        }
        setSkipped(result.skipped);
      }
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
  }

  async function handleRemove(id: number) {
    setError(null);
    try {
      await removeItems([id]);
      dispatch({ type: "REMOVE_IMAGE_ITEM", id });
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
  }

  async function handleClearAll() {
    setError(null);
    const ids = imagesToPdf.items.map((item) => item.id);
    try {
      await removeItems(ids);
      dispatch({ type: "CLEAR_IMAGE_ITEMS" });
      setSkipped(null);
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLOListElement>) => {
    if (
      disabled ||
      !e.altKey ||
      (e.key !== "ArrowUp" && e.key !== "ArrowDown")
    ) {
      return;
    }
    const row = findRowElement(e.target);
    if (!row || !listRef.current) {
      return;
    }
    const i = Array.from(listRef.current.children).indexOf(row);
    if (i < 0) {
      return;
    }

    if (e.key === "ArrowUp" && i > 0) {
      e.preventDefault();
      focusedItemIdRef.current = imagesToPdf.items[i].id;
      dispatch({ type: "MOVE_IMAGE_ITEM", fromIndex: i, toIndex: i - 1 });
    } else if (e.key === "ArrowDown" && i < imagesToPdf.items.length - 1) {
      e.preventDefault();
      focusedItemIdRef.current = imagesToPdf.items[i].id;
      dispatch({ type: "MOVE_IMAGE_ITEM", fromIndex: i, toIndex: i + 1 });
    }
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLOListElement>) => {
    if (disabled || e.button !== 0) {
      return;
    }
    if (e.target instanceof Element && e.target.closest("button")) {
      return;
    }
    const row = findRowElement(e.target);
    if (!row || !listRef.current) {
      return;
    }
    const i = Array.from(listRef.current.children).indexOf(row);
    if (i < 0) {
      return;
    }

    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      // Ignored if pointer capture is unsupported in test environments
    }

    dragIndexRef.current = i;
    overIndexRef.current = i;
    setDragIndex(i);
    setOverIndex(i);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLOListElement>) => {
    if (dragIndexRef.current === null || !listRef.current) {
      return;
    }
    const row = findRowElement(e.target);
    if (row) {
      const i = Array.from(listRef.current.children).indexOf(row);
      if (i >= 0 && i !== overIndexRef.current) {
        overIndexRef.current = i;
        setOverIndex(i);
      }
    }
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLOListElement>) => {
    try {
      if (e.currentTarget.hasPointerCapture(e.pointerId)) {
        e.currentTarget.releasePointerCapture(e.pointerId);
      }
    } catch {
      // Ignored
    }
    const fromIdx = dragIndexRef.current;
    if (fromIdx === null || !listRef.current) {
      return;
    }
    const row = findRowElement(e.target);
    const toIdx = row
      ? Array.from(listRef.current.children).indexOf(row)
      : overIndexRef.current;
    if (toIdx !== null && toIdx >= 0 && toIdx !== fromIdx) {
      dispatch({
        type: "MOVE_IMAGE_ITEM",
        fromIndex: fromIdx,
        toIndex: toIdx,
      });
    }
    dragIndexRef.current = null;
    overIndexRef.current = null;
    setDragIndex(null);
    setOverIndex(null);
  };

  const handlePointerCancel = (e: React.PointerEvent<HTMLOListElement>) => {
    try {
      if (e.currentTarget.hasPointerCapture(e.pointerId)) {
        e.currentTarget.releasePointerCapture(e.pointerId);
      }
    } catch {
      // Ignored
    }
    dragIndexRef.current = null;
    overIndexRef.current = null;
    setDragIndex(null);
    setOverIndex(null);
  };

  // Window pointer handlers to track drag even if cursor leaves row bounds
  useEffect(() => {
    if (dragIndex === null) {
      return;
    }

    const onWindowPointerMove = (e: PointerEvent) => {
      const row = findRowFromPoint(e.clientX, e.clientY);
      if (row && listRef.current) {
        const i = Array.from(listRef.current.children).indexOf(row);
        if (i >= 0 && i !== overIndexRef.current) {
          overIndexRef.current = i;
          setOverIndex(i);
        }
      }
    };

    const onWindowPointerUp = (e: PointerEvent) => {
      const fromIdx = dragIndexRef.current;
      let toIdx = overIndexRef.current;
      const row = findRowFromPoint(e.clientX, e.clientY);
      if (row && listRef.current) {
        const i = Array.from(listRef.current.children).indexOf(row);
        if (i >= 0) {
          toIdx = i;
        }
      }
      if (fromIdx !== null && toIdx !== null && toIdx !== fromIdx) {
        dispatch({
          type: "MOVE_IMAGE_ITEM",
          fromIndex: fromIdx,
          toIndex: toIdx,
        });
      }
      dragIndexRef.current = null;
      overIndexRef.current = null;
      setDragIndex(null);
      setOverIndex(null);
    };

    const onWindowPointerCancel = () => {
      dragIndexRef.current = null;
      overIndexRef.current = null;
      setDragIndex(null);
      setOverIndex(null);
    };

    window.addEventListener("pointermove", onWindowPointerMove);
    window.addEventListener("pointerup", onWindowPointerUp);
    window.addEventListener("pointercancel", onWindowPointerCancel);
    return () => {
      window.removeEventListener("pointermove", onWindowPointerMove);
      window.removeEventListener("pointerup", onWindowPointerUp);
      window.removeEventListener("pointercancel", onWindowPointerCancel);
    };
  }, [dragIndex, dispatch]);

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

  const errorDisplay = error !== null && (
    <ErrorDisplay
      message={formatErrorMessage(error.code, error.detail, language.language)}
      onDismiss={() => setError(null)}
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
          onAddFiles={() => void handleAdd("files")}
          onAddFolder={() => void handleAdd("folder")}
        />
      </div>
    );
  }

  const isMerge = imagesToPdf.output === "merge";
  const skippedText = formatSkippedMessage(skipped, t);

  return (
    <div className="images-view">
      {errorDisplay}
      <OutputDirError tab="imagesToPdf" />

      <JobSummaryBanner
        tab="imagesToPdf"
        detail={
          job.savedName !== null && (
            <span className="mono">
              {formatMessage(t.imagesToPdf.savedName, { name: job.savedName })}
            </span>
          )
        }
      />

      <div className="images-view-header">
        <div className="images-view-title-group">
          <span className="label" id={headingId}>
            {formatMessage(t.imagesToPdf.imageCount, {
              count: imagesToPdf.items.length,
            })}
          </span>
          {skippedText ? (
            <span className="hint">{skippedText}</span>
          ) : isMerge ? (
            <span className="hint">{t.imagesToPdf.reorderHint}</span>
          ) : null}
        </div>
        <div className="images-view-actions">
          <button
            type="button"
            className="btn btn-ghost"
            disabled={disabled}
            onClick={() => void handleAdd("files")}
          >
            {t.imagesToPdf.addImages}
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={disabled}
            onClick={() => void handleAdd("folder")}
          >
            {t.imagesToPdf.addFolder}
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={disabled}
            onClick={() => void handleClearAll()}
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
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerCancel}
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
                  meta={formatMeta(item)}
                  customThumbnail={
                    <ImageThumbnail id={item.id} name={item.name} />
                  }
                  error={reason}
                  status={status}
                  className={getRowDropClass(index, dragIndex, overIndex)}
                  data-row-index={index}
                  tabIndex={disabled ? undefined : 0}
                  onMoveUp={
                    disabled
                      ? undefined
                      : () =>
                          dispatch({
                            type: "MOVE_IMAGE_ITEM",
                            fromIndex: index,
                            toIndex: index - 1,
                          })
                  }
                  onMoveDown={
                    disabled
                      ? undefined
                      : () =>
                          dispatch({
                            type: "MOVE_IMAGE_ITEM",
                            fromIndex: index,
                            toIndex: index + 1,
                          })
                  }
                  onRemove={
                    disabled ? undefined : () => void handleRemove(item.id)
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
                        onClick={() => void handleRemove(item.id)}
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
