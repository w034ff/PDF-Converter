export interface PdfFileIconProps {
  /** Adds a dashed area under the letters, for the crop tab, which cuts parts out of a page. */
  area?: boolean;
}

/**
 * A page with "PDF" on it, for the drop zones that take PDFs.
 * The letters are strokes, not text, so that they do not move with the
 * fonts of the system.
 */
export function PdfFileIcon({ area = false }: PdfFileIconProps) {
  return (
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
      <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <path d="M14 3v6h6" />
      {area ? (
        <g strokeWidth="1.25">
          <path d="M6.85 13.75V10.5h.975a.8125 .8125 0 0 1 0 1.625H6.85" />
          <path d="M9.95 10.5h.65a1.625 1.625 0 0 1 0 3.25h-.65z" />
          <path d="M15.15 10.5H13.5v3.25M13.5 12.125h1.3" />
          <rect
            x="7"
            y="15.25"
            width="10"
            height="3.25"
            rx="0.5"
            strokeDasharray="1.75 1.25"
          />
        </g>
      ) : (
        <g strokeWidth="1.25">
          <path d="M6.25 16.5V12h1.35a1.125 1.125 0 0 1 0 2.25H6.25" />
          <path d="M10.5 12h.9a2.25 2.25 0 0 1 0 4.5h-.9z" />
          <path d="M17.75 12H15.5v4.5M15.5 14.25h1.8" />
        </g>
      )}
    </svg>
  );
}
