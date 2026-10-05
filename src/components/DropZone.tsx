import type { ReactNode } from "react";
import "./DropZone.css";

export interface DropZoneProps {
  title: string;
  description: string;
  addFilesLabel?: string;
  addFolderLabel?: string;
  onAddFiles?: () => void;
  onAddFolder?: () => void;
  isDragOver?: boolean;
  icon?: ReactNode;
}

/**
 * Drop zone area displayed when queue is empty (design §10.1, mockup Main.dc.html).
 * Pure presentation component that does not invoke IPC directly.
 */
export function DropZone({
  title,
  description,
  addFilesLabel = "ファイルを追加",
  addFolderLabel = "フォルダを追加",
  onAddFiles,
  onAddFolder,
  isDragOver = false,
  icon,
}: DropZoneProps) {
  return (
    <div
      className={`drop-zone ${isDragOver ? "is-drag-over" : ""}`}
      data-testid="drop-zone"
    >
      {icon ? (
        icon
      ) : (
        <svg
          className="drop-zone-icon"
          width="48"
          height="48"
          viewBox="0 0 24 24"
          fill="none"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <circle cx="8.5" cy="8.5" r="1.5" />
          <path d="M21 15l-5-5L5 21" />
        </svg>
      )}

      <p className="drop-zone-title">{title}</p>
      <p className="drop-zone-description">{description}</p>

      {(onAddFiles || onAddFolder) && (
        <div className="drop-zone-actions">
          {onAddFiles && (
            <button
              type="button"
              className="btn btn-primary"
              onClick={onAddFiles}
            >
              {addFilesLabel}
            </button>
          )}
          {onAddFolder && (
            <button
              type="button"
              className="btn btn-ghost"
              onClick={onAddFolder}
            >
              {addFolderLabel}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
