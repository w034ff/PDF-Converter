import { InfoIcon } from "./InfoIcon";
import { LockIcon } from "./LockIcon";
import "./NoticeDisplay.css";

export interface NoticeDisplayProps {
  message: string;
  /** Placed at the right end of the line; wraps to its own line when narrow. */
  action?: React.ReactNode;
  /**
   * The mark before the message: the lock for a PDF waiting for its password,
   * the "i" for any other notice. Defaults to the lock.
   */
  icon?: "lock" | "info";
}

/**
 * Band for a state that is not an error: a PDF that opens once its password
 * is entered, or a limit the user can work around (design §10.1, §10.3). It
 * is a status, not an alert, and has no dismiss button since the state stays
 * until acted on.
 */
export function NoticeDisplay({
  message,
  action,
  icon = "lock",
}: NoticeDisplayProps) {
  return (
    <div role="status" className="notice-display">
      {icon === "lock" ? (
        <LockIcon size={18} className="notice-display-icon" />
      ) : (
        <InfoIcon size={18} className="notice-display-icon" />
      )}
      <div className="notice-display-body">
        <span className="notice-display-message">{message}</span>
        {action !== undefined && (
          <div className="notice-display-action">{action}</div>
        )}
      </div>
    </div>
  );
}
