import { useEffect, useId, useRef, useState } from "react";
import {
  DropZone,
  ErrorDisplay,
  ListRow,
  type ListRowStatus,
} from "../../components";
import { formatJobSummary } from "../job/jobSummary";
import {
  formatErrorMessage,
  formatMessage,
  getTranslations,
  type Translations,
} from "../../i18n";
import {
  addImages,
  normalizeIpcError,
  removeItems,
  type AddSource,
  type ImageItem,
  type IpcError,
  type Skipped,
} from "../../ipc";
import {
  isJobActive,
  jobRowStatus,
  useAppDispatch,
  useAppState,
  type JobRowStatus,
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
  return `（${formatMessage(t.imagesToPdf.skipped, {
    count: total,
    reasons: reasons.join(t.imagesToPdf.skippedSeparator),
  })}）`;
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

function ImageThumbnail({ id, name }: { id: number; name: string }) {
  const { language } = useAppState();
  const t = getTranslations(language.language);
  const { ref: attachRef, src: imageSrc } = useThumbnail(id);
  const altText = formatMessage(t.listRow.thumbnailAlt, { name });

  return (
    <div ref={attachRef} className="image-thumb-container">
      {imageSrc ? (
        <img src={imageSrc} alt={altText} />
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

  async function handleAdd(source: AddSource) {
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
    try {
      await removeItems([id]);
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
    dispatch({ type: "REMOVE_IMAGE_ITEM", id });
  }

  async function handleClearAll() {
    const ids = imagesToPdf.items.map((item) => item.id);
    try {
      await removeItems(ids);
    } catch (err: unknown) {
      setError(normalizeIpcError(err));
    }
    dispatch({ type: "CLEAR_IMAGE_ITEMS" });
    setSkipped(null);
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

  // Keep DOM row attributes (tabindex, data-row-index, drag indicators, focus) in sync
  useEffect(() => {
    if (!listRef.current) {
      return;
    }
    const rows = listRef.current.querySelectorAll<HTMLLIElement>(".list-row");
    rows.forEach((row, i) => {
      row.setAttribute("data-row-index", String(i));
      if (!disabled) {
        row.tabIndex = 0;
      } else {
        row.removeAttribute("tabindex");
      }
      if (i === dragIndex) {
        row.classList.add("is-dragging");
      } else {
        row.classList.remove("is-dragging");
      }
      if (dragIndex !== null && i === overIndex && i !== dragIndex) {
        row.classList.add("is-drop-target");
      } else {
        row.classList.remove("is-drop-target");
      }
    });

    if (focusedItemIdRef.current !== null) {
      const focusIdx = imagesToPdf.items.findIndex(
        (item) => item.id === focusedItemIdRef.current,
      );
      if (focusIdx >= 0 && rows[focusIdx]) {
        rows[focusIdx].focus();
      }
      focusedItemIdRef.current = null;
    }
  });

  if (imagesToPdf.items.length === 0) {
    return (
      <DropZone
        title={t.dropZone.titleImages}
        description={t.dropZone.descriptionImages}
        addFilesLabel={t.dropZone.addImages}
        addFolderLabel={t.dropZone.addFolder}
        onAddFiles={() => void handleAdd("files")}
        onAddFolder={() => void handleAdd("folder")}
      />
    );
  }

  const isMerge = imagesToPdf.output === "merge";
  const validPageCount = imagesToPdf.items.filter(
    (item) => item.error === null,
  ).length;
  const skippedText = formatSkippedMessage(skipped, t);

  const summaryText =
    job.kind === "imagesToPdf" && job.finished !== null
      ? formatJobSummary(t, job.finished)
      : null;

  return (
    <div className="images-view">
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

      {summaryText && (
        <div className="images-job-summary" role="status">
          <span>{summaryText}</span>
          {job.savedName && (
            <span className="mono images-job-saved-name">
              （{job.savedName}）
            </span>
          )}
        </div>
      )}

      {isMerge ? (
        <>
          <ol
            ref={listRef}
            className="images-list"
            aria-labelledby={headingId}
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onKeyDown={handleKeyDown}
          >
            {imagesToPdf.items.map((item, index) => {
              const rowStatus =
                job.kind === "imagesToPdf" ? jobRowStatus(job, item.id) : null;
              const status = toListRowStatus(rowStatus, t);

              return (
                <ListRow
                  key={item.id}
                  index={index + 1}
                  title={item.name}
                  meta={formatMeta(item)}
                  customThumbnail={
                    <ImageThumbnail id={item.id} name={item.name} />
                  }
                  error={
                    item.error
                      ? formatErrorMessage(
                          item.error.code,
                          item.error.detail,
                          language.language,
                        )
                      : null
                  }
                  status={status}
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
          <div className="images-merge-summary hint">
            {formatMessage(t.imagesToPdf.mergePageCount, {
              count: validPageCount,
            })}
          </div>
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

                const rowStatus =
                  job.kind === "imagesToPdf"
                    ? jobRowStatus(job, item.id)
                    : null;
                const listStatus = toListRowStatus(rowStatus, t);

                let cellStatusText = "—";
                let statusKind: "ok" | "failed" | "running" | "waiting" =
                  "waiting";
                if (listStatus) {
                  cellStatusText = listStatus.text;
                  statusKind = listStatus.kind;
                } else if (item.error !== null) {
                  cellStatusText = formatErrorMessage(
                    item.error.code,
                    item.error.detail,
                    language.language,
                  );
                  statusKind = "failed";
                }

                return (
                  <tr key={item.id}>
                    <td>{item.name}</td>
                    <td className="mono">{outputName}</td>
                    <td className={`images-table-status status-${statusKind}`}>
                      {cellStatusText}
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
