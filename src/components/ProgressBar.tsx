import "./ProgressBar.css";

export interface ProgressBarProps {
  value: number;
  max: number;
  min?: number;
  label?: string;
  statusText?: string;
  showCount?: boolean;
  className?: string;
}

/**
 * Accessible progress bar component (design §7.2, §10.1).
 * Renders role="progressbar" with aria-valuenow, aria-valuemin, aria-valuemax.
 */
export function ProgressBar({
  value,
  max,
  min = 0,
  label = "進捗",
  statusText,
  showCount = false,
  className = "",
}: ProgressBarProps) {
  const range = max - min;
  const percentage =
    range > 0 ? Math.min(100, Math.max(0, ((value - min) / range) * 100)) : 0;

  return (
    <div className={`progress-bar-container ${className}`}>
      {(statusText || showCount) && (
        <div className="progress-bar-header">
          {statusText && <span>{statusText}</span>}
          {showCount && (
            <span className="mono">
              {value} / {max}
            </span>
          )}
        </div>
      )}
      <div
        className="progress-bar-track"
        role="progressbar"
        aria-label={label}
        aria-valuenow={value}
        aria-valuemin={min}
        aria-valuemax={max}
      >
        <div
          className="progress-bar-fill"
          data-testid="progress-bar-fill"
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  );
}
