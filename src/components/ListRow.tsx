import type { LiHTMLAttributes, ReactNode, Ref } from "react";
import "./ListRow.css";

export interface ListRowStatus {
  text: string;
  kind: "ok" | "failed" | "running" | "waiting";
}

export interface ListRowProps extends LiHTMLAttributes<HTMLLIElement> {
  index?: number;
  title: string;
  meta?: string;
  thumbnailSrc?: string | null;
  thumbnailAlt?: string;
  customThumbnail?: ReactNode;
  error?: string | null;
  status?: ListRowStatus | null;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  onRemove?: () => void;
  canMoveUp?: boolean;
  canMoveDown?: boolean;
  labels?: {
    moveUp?: string;
    moveDown?: string;
    remove?: string;
  };
  showDragHandle?: boolean;
  ref?: Ref<HTMLLIElement>;
}

/**
 * List row component displaying an item in the image/PDF queue (design §6.1).
 * Pure presentation component that does not invoke IPC.
 */
export function ListRow({
  index,
  title,
  meta,
  thumbnailSrc,
  thumbnailAlt,
  customThumbnail,
  error,
  status,
  onMoveUp,
  onMoveDown,
  onRemove,
  canMoveUp = true,
  canMoveDown = true,
  labels,
  showDragHandle = true,
  className,
  ref,
  ...rest
}: ListRowProps) {
  const moveUpLabel = labels?.moveUp ?? "上へ";
  const moveDownLabel = labels?.moveDown ?? "下へ";
  const removeLabel = labels?.remove ?? "一覧から外す";
  const altText = thumbnailAlt ?? `${title} のサムネイル`;

  const classes = ["list-row", error ? "has-error" : "", className]
    .filter(Boolean)
    .join(" ");

  return (
    <li ref={ref} className={classes} {...rest}>
      {index !== undefined && (
        <span className="mono list-row-index">{index}</span>
      )}

      {showDragHandle && (
        <span className="list-row-drag-handle" aria-hidden="true">
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
          >
            <circle cx="9" cy="6" r="1" fill="currentColor" />
            <circle cx="15" cy="6" r="1" fill="currentColor" />
            <circle cx="9" cy="12" r="1" fill="currentColor" />
            <circle cx="15" cy="12" r="1" fill="currentColor" />
            <circle cx="9" cy="18" r="1" fill="currentColor" />
            <circle cx="15" cy="18" r="1" fill="currentColor" />
          </svg>
        </span>
      )}

      <div className="list-row-thumb">
        {customThumbnail ? (
          customThumbnail
        ) : thumbnailSrc ? (
          <img src={thumbnailSrc} alt={altText} />
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

      <div className="list-row-content">
        <span className="list-row-title" title={title}>
          {title}
        </span>
        {meta && <span className="mono list-row-meta">{meta}</span>}
        {error && <span className="list-row-error">{error}</span>}
      </div>

      {status && (
        <span className={`list-row-status status-${status.kind}`}>
          {status.text}
        </span>
      )}

      {(onMoveUp || onMoveDown || onRemove) && (
        <div className="list-row-actions">
          {onMoveUp && (
            <button
              type="button"
              className="icon-btn"
              aria-label={moveUpLabel}
              onClick={onMoveUp}
              disabled={!canMoveUp}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M12 19V5M5 12l7-7 7 7" />
              </svg>
            </button>
          )}

          {onMoveDown && (
            <button
              type="button"
              className="icon-btn"
              aria-label={moveDownLabel}
              onClick={onMoveDown}
              disabled={!canMoveDown}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M12 5v14M5 12l7 7 7-7" />
              </svg>
            </button>
          )}

          {onRemove && (
            <button
              type="button"
              className="icon-btn"
              aria-label={removeLabel}
              onClick={onRemove}
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                aria-hidden="true"
              >
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      )}
    </li>
  );
}
