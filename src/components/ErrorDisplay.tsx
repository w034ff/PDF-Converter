import "./ErrorDisplay.css";

export interface ErrorDisplayProps {
  message: string;
  detail?: string | null;
  onDismiss?: () => void;
  dismissLabel?: string;
  className?: string;
}

/**
 * Error display component with role="alert" (design §6.6, §10.1).
 * Pure presentation component that does not invoke IPC directly.
 */
export function ErrorDisplay({
  message,
  detail,
  onDismiss,
  dismissLabel = "閉じる",
  className = "",
}: ErrorDisplayProps) {
  return (
    <div
      role="alert"
      className={`error-display ${className}`}
      data-testid="error-display"
    >
      <svg
        className="error-display-icon"
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="8" x2="12" y2="12" />
        <line x1="12" y1="16" x2="12.01" y2="16" />
      </svg>
      <div className="error-display-body">
        <span className="error-display-message">{message}</span>
        {detail && <span className="mono error-display-detail">{detail}</span>}
      </div>
      {onDismiss && (
        <button
          type="button"
          className="icon-btn error-display-dismiss"
          aria-label={dismissLabel}
          onClick={onDismiss}
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
  );
}
