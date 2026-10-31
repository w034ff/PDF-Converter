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
import {
  formatJobSummary,
  jobOutcome,
  organizeSummaryShape,
  summaryShape,
} from "./jobSummary";
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
 * The file being made, for the organize tab: its output is not an item of a
 * list, so `job-progress` names the file instead of one of the targets.
 */
function organizeRunningText(t: Translations, job: JobState): string {
  const name = job.progress?.current;
  return name === null || name === undefined
    ? t.job.running
    : formatMessage(t.job.runningItem, { name });
}

/** How the screen's last conversion ended, with its counts, in the outcome's color. */
function finishedStatus(finished: JobFinishedPayload, summary: string) {
  const className = {
    failure: "error-text",
    success: "success-text",
    neutral: "hint",
  }[jobOutcome(finished)];
  return (
    <span role="status" className={className}>
      {summary}
    </span>
  );
}

/**
 * The bottom bar's status and button for a conversion (mockups `ImagesEach`
 * and `PdfBatch`): progress and Cancel while it runs, then how it ended
 * with its counts, in red when anything failed (design §10.1). It is the
 * only place the result is told, so the lists above it do not move.
 * A running conversion shows on both tabs, since it locks both; a finished
 * or failed one shows only on the tab that started it.
 */
export function JobFooter({ tab, idleStatus, action }: JobFooterProps) {
  const { language, job, crop, imagesToPdf, pdfToImages, organize } =
    useAppState();
  const runner = useJobRunner();
  const t = getTranslations(language.language);

  // The dialog is modal and nothing runs yet, so there is nothing to cancel.
  if (isJobActive(job) && job.choosingDestination) {
    return (
      <>
        <span className="hint">{t.job.choosingDestination}</span>
        {action}
      </>
    );
  }

  if (isJobActive(job)) {
    const cancelling = job.phase === "cancelling";
    // The pages are not items with names, so the bar says what is going on.
    const detecting = job.kind === "crop" && crop.lastJob === "detectAll";
    return (
      <>
        {job.progress === null ? (
          <span className="hint">
            {detecting ? t.crop.footer.detecting : t.job.running}
          </span>
        ) : (
          <ProgressBar
            className="app-footer-progress"
            value={job.progress.done}
            max={job.progress.total}
            label={t.footer.progressLabel}
            statusText={
              cancelling
                ? t.footer.cancelling
                : detecting
                  ? t.crop.footer.detecting
                  : job.kind === "organize"
                    ? organizeRunningText(t, job)
                    : runningText(t, job)
            }
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
    // A PDF that failed on some pages is counted as failed by `job-finished`;
    // the rows say which, and the summary lists them apart.
    const partial =
      tab === "pdfToImages"
        ? Object.values(job.results).filter(
            (result) => result.status === "partial",
          ).length
        : 0;
    status = finishedStatus(
      job.finished,
      formatJobSummary(
        t,
        job.finished,
        tab === "organize"
          ? organizeSummaryShape(job, organize.pdfs, language.language)
          : summaryShape(tab, job, imagesToPdf, pdfToImages, crop, partial),
      ),
    );
  }

  return (
    <>
      {status}
      {action}
    </>
  );
}
