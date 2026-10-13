import { LockIcon } from "./LockIcon";
import "./NoticeDisplay.css";

export interface NoticeDisplayProps {
  message: string;
  /** Placed at the right end of the line; wraps to its own line when narrow. */
  action?: React.ReactNode;
}

/**
 * Band for a state that waits for the user rather than an error: a PDF that
 * opens once its password is entered (design §10.1, §10.3). It is a status,
 * not an alert, and has no dismiss button since the state stays until acted
 * on.
 */
export function NoticeDisplay({ message, action }: NoticeDisplayProps) {
  return (
    <div role="status" className="notice-display">
      <LockIcon size={18} className="notice-display-icon" />
      <div className="notice-display-body">
        <span className="notice-display-message">{message}</span>
        {action !== undefined && (
          <div className="notice-display-action">{action}</div>
        )}
      </div>
    </div>
  );
}
