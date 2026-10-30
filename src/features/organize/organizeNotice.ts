import type { IpcError, Skipped } from "../../ipc";
import type { OrganizeNotice } from "../../state";

/** What an addition left out or failed on, or `null` when there is nothing to tell. */
export function noticeOf(
  skipped: Skipped | null,
  error: IpcError | null,
): OrganizeNotice | null {
  const hasSkipped =
    skipped !== null &&
    skipped.unsupported + skipped.folders + skipped.duplicates > 0;
  return hasSkipped || error !== null
    ? { skipped: hasSkipped ? skipped : null, error }
    : null;
}
