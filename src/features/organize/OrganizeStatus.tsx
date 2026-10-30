import { useMemo } from "react";
import { formatMessage, getTranslations } from "../../i18n";
import { MAX_PDF_PAGES, useAppState } from "../../state";
import { pluralize } from "../job/jobSummary";
import { bytesToMegabytes, estimateOutputBytes } from "./estimate";

/**
 * What the bottom bar says while nothing is converting (organize §4): what
 * "PDF を保存" would do and about how big the PDF will be, or why it cannot be
 * saved; and, when some PDFs of the list will not be in it, how many.
 */
export function OrganizeStatus() {
  const { language, organize } = useAppState();
  const t = getTranslations(language.language).organize.footer;
  const { pdfs, pages } = organize;
  const megabytes = useMemo(
    () => bytesToMegabytes(estimateOutputBytes(pages, pdfs)),
    [pages, pdfs],
  );
  const leftOut = pdfs.filter((pdf) => pdf.error !== null).length;
  const suffix =
    leftOut === 0 ? "" : pluralize(leftOut, t.leftOutSingle, t.leftOutMultiple);

  if (pdfs.length === 0) {
    return <span className="hint">{t.noPdfs}</span>;
  }
  if (pages.length === 0) {
    return <span className="hint">{t.noPages + suffix}</span>;
  }
  if (pages.length > MAX_PDF_PAGES) {
    return (
      <span className="error-text">
        {formatMessage(t.tooManyPages, {
          max: MAX_PDF_PAGES.toLocaleString(language.language),
          count: pages.length,
        }) + suffix}
      </span>
    );
  }
  return (
    <span className="hint">
      {formatMessage(
        pages.length === 1 ? t.willSaveSingle : t.willSaveMultiple,
        { count: pages.length, size: megabytes },
      ) + suffix}
    </span>
  );
}
