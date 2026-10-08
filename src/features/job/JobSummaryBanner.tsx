import { getTranslations } from "../../i18n";
import {
  useAppState,
  type ActiveTab,
  type AppState,
  type JobState,
} from "../../state";
import { countPagesInIntervals } from "../pdf-to-images/pdfUtils";
import {
  formatJobSummary,
  jobOutcome,
  singlePdfPageCounts,
  type JobSummaryShape,
} from "./jobSummary";
import "./JobSummaryBanner.css";

export interface JobSummaryBannerProps {
  /** The screen the banner belongs to; it shows only that screen's result. */
  tab: ActiveTab;
  /** Items the backend counted as failed that only partly failed. */
  partialCount?: number;
}

/** How many pages the range selected in the PDF with `pageCount` pages. */
function selectedPageCount(
  pdfToImages: AppState["pdfToImages"],
  pageCount: number,
): number | null {
  if (pdfToImages.pageSelection === "all") {
    return pageCount;
  }
  return pdfToImages.rangeResult === null
    ? null
    : countPagesInIntervals(pdfToImages.rangeResult.intervals, pageCount);
}

/**
 * What the summary of the screen's last conversion counts: pages when one
 * PDF was converted, the pages of the PDF written by "Single PDF", items
 * otherwise.
 */
function summaryShape(
  tab: ActiveTab,
  job: JobState,
  pdfToImages: AppState["pdfToImages"],
  partialCount: number,
): JobSummaryShape {
  if (tab === "pdfToImages" && job.targets.length === 1) {
    const [target] = job.targets;
    const item = pdfToImages.items.find((pdf) => pdf.id === target.id);
    const selected =
      item === undefined
        ? null
        : selectedPageCount(pdfToImages, item.pageCount);
    if (selected !== null) {
      return singlePdfPageCounts(job.results[target.id], selected);
    }
  }
  if (
    tab === "imagesToPdf" &&
    job.savedName !== null &&
    job.finished !== null &&
    job.finished.succeeded > 0
  ) {
    return {
      kind: "mergedPdf",
      pages: job.finished.succeeded,
      failedImages: job.finished.failed,
    };
  }
  return { kind: "items", partial: partialCount };
}

/**
 * The result of the screen's last conversion, at the top of its list
 * (mockup `PdfBatch`), on both screens alike. Its mark and edge say at a
 * glance whether anything failed; the counts and the rows say what.
 */
export function JobSummaryBanner({
  tab,
  partialCount = 0,
}: JobSummaryBannerProps) {
  const { language, job, pdfToImages } = useAppState();
  if (job.kind !== tab || job.finished === null) {
    return null;
  }
  const t = getTranslations(language.language);
  const outcome = jobOutcome(job.finished);
  return (
    <div role="status" className={`job-summary is-${outcome}`}>
      <span className="job-summary-title">
        {formatJobSummary(
          t,
          job.finished,
          summaryShape(tab, job, pdfToImages, partialCount),
        )}
      </span>
    </div>
  );
}
