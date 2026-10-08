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
import type { JobFinishedPayload } from "../../ipc";
import { isOutputDirError } from "../output";
import { jobOutcome } from "./jobSummary";
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

function finishedStatus(t: Translations, finished: JobFinishedPayload) {
  switch (jobOutcome(finished)) {
    case "failure":
      return <span className="error-text">{t.job.doneWithFailures}</span>;
    case "success":
      return <span className="success-text">{t.job.doneAllSucceeded}</span>;
    case "neutral":
      return (
        <span className="hint">
          {finished.cancelled ? t.job.cancelled : t.job.done}
        </span>
      );
  }
}

/**
 * The bottom bar's status and button for a conversion (mockups `ImagesEach`
 * and `PdfBatch`): progress and Cancel while it runs, then how it
 * ended, in red when anything failed (design §10.1).
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

  // An error comes first: one raised after `job-finished` (the merged PDF
  // could not be written) matters more than the counts before it.
  let status = idleStatus;
  if (
    job.error !== null &&
    !isOutputDirError(job.error) &&
    (job.kind === null || job.kind === tab)
  ) {
    status = (
      <span className="error-text" role="alert">
        {formatErrorMessage(
          job.error.code,
          job.error.detail,
          language.language,
        )}
      </span>
    );
  } else if (job.kind === tab && job.finished !== null) {
    status = finishedStatus(t, job.finished);
  }

  return (
    <>
      {status}
      {action}
    </>
  );
}
