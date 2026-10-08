import type { ReactNode } from "react";
import { getTranslations } from "../../i18n";
import { useAppState, type ActiveTab } from "../../state";
import { formatJobSummary, jobOutcome } from "./jobSummary";
import "./JobSummaryBanner.css";

export interface JobSummaryBannerProps {
  /** The screen the banner belongs to; it shows only that screen's result. */
  tab: ActiveTab;
  /** Items the backend counted as failed that only partly failed. */
  partialCount?: number;
  /** A line under the counts, such as the name a merged PDF was saved as. */
  detail?: ReactNode;
}

/**
 * The result of the screen's last conversion, at the top of its list
 * (mockup `PdfBatch`), on both screens alike. Its mark and edge say at a
 * glance whether anything failed; the counts and the rows say what.
 */
export function JobSummaryBanner({
  tab,
  partialCount = 0,
  detail,
}: JobSummaryBannerProps) {
  const { language, job } = useAppState();
  if (job.kind !== tab || job.finished === null) {
    return null;
  }
  const t = getTranslations(language.language);
  const outcome = jobOutcome(job.finished);
  return (
    <div role="status" className={`job-summary is-${outcome}`}>
      <span className="job-summary-title">
        {formatJobSummary(t, job.finished, partialCount)}
      </span>
      {detail}
    </div>
  );
}
