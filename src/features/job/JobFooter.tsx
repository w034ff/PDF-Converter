import type { ReactNode } from "react";
import { ProgressBar } from "../../components";
import { formatErrorMessage, formatMessage, getTranslations } from "../../i18n";
import {
  isJobActive,
  runningIds,
  useAppState,
  type ActiveTab,
  type JobState,
} from "../../state";
import type { Translations } from "../../i18n";
import { useJobRunner } from "./useJobRunner";

export interface JobFooterProps {
  /** The screen the footer belongs to. */
  tab: ActiveTab;
  /** What the left side shows when no conversion of this screen is in view. */
  idleStatus: ReactNode;
  /** The screen's own button ("PDF を保存", "変換を開始"), replaced by Cancel while a job runs. */
  action: ReactNode;
}

function runningText(t: Translations, job: JobState): string {
  const running = runningIds(job);
  const latest = running.at(-1);
  const name = job.targets.find((target) => target.id === latest)?.name;
  if (name === undefined) {
    return t.job.running;
  }
  if (running.length === 1) {
    return formatMessage(t.job.runningItem, { name });
  }
  return formatMessage(t.job.runningItemAndOthers, {
    name,
    count: running.length - 1,
  });
}

/**
 * The bottom bar's status and button for a conversion (mockups `ImagesEach`
 * and `PdfBatch`): progress and Cancel while it runs, "完了" once it ends.
 * A running conversion shows on both tabs, since it locks both; a finished
 * or failed one shows only on the tab that started it.
 */
export function JobFooter({ tab, idleStatus, action }: JobFooterProps) {
  const { language, job } = useAppState();
  const runner = useJobRunner();
  const t = getTranslations(language.language);

  if (isJobActive(job)) {
    const cancelling = job.phase === "cancelling";
    return (
      <>
        {job.progress === null ? (
          <span className="hint">{t.job.running}</span>
        ) : (
          <ProgressBar
            className="app-footer-progress"
            value={job.progress.done}
            max={job.progress.total}
            label={t.footer.progressLabel}
            statusText={cancelling ? t.footer.cancelling : runningText(t, job)}
            showCount
          />
        )}
        <button
          type="button"
          className="btn btn-ghost app-footer-action"
          disabled={cancelling}
          onClick={() => void runner.cancel()}
        >
          {cancelling ? t.footer.cancelling : t.footer.cancel}
        </button>
      </>
    );
  }

  let status = idleStatus;
  if (job.kind === tab && job.finished !== null) {
    status = (
      <span className="hint">
        {job.finished.cancelled ? t.job.cancelled : t.job.done}
      </span>
    );
  } else if (job.error !== null && (job.kind === null || job.kind === tab)) {
    status = (
      <span className="hint" role="alert">
        {formatErrorMessage(
          job.error.code,
          job.error.detail,
          language.language,
        )}
      </span>
    );
  }

  return (
    <>
      {status}
      {action}
    </>
  );
}
