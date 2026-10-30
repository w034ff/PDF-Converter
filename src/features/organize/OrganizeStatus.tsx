import { useMemo } from "react";
import { formatMessage, getTranslations } from "../../i18n";
import { MAX_PDF_PAGES, splitIntoParts, useAppState } from "../../state";
import { pluralize } from "../job/jobSummary";
import {
  bytesToMegabytes,
  estimateOutputBytes,
  estimatePartsBytes,
} from "./estimate";

/**
 * What the bottom bar says while nothing is converting (organize §4): what the
 * save would do and about how big the PDFs will be, or why it cannot be
 * saved; and, when some PDFs of the list will not be in it, how many.
 */
export function OrganizeStatus() {
  const { language, organize } = useAppState();
  const t = getTranslations(language.language).organize.footer;
  const { pdfs, pages, breaks, output } = organize;
  const isSplit = output === "split";
  // The outputs, once for the pages: the status draws again with each key.
  const parts = useMemo(
    () => (isSplit ? splitIntoParts(pages, breaks) : []),
    [isSplit, pages, breaks],
  );
  const megabytes = useMemo(
    () =>
      bytesToMegabytes(
        isSplit
          ? estimatePartsBytes(parts, pdfs)
          : estimateOutputBytes(pages, pdfs),
      ),
    [isSplit, parts, pages, pdfs],
  );
  const leftOut = pdfs.filter((pdf) => pdf.error !== null).length;
  const suffix =
    leftOut === 0 ? "" : pluralize(leftOut, t.leftOutSingle, t.leftOutMultiple);
  const max = MAX_PDF_PAGES.toLocaleString(language.language);

  if (pdfs.length === 0) {
    return <span className="hint">{t.noPdfs}</span>;
  }
  if (pages.length === 0) {
    return <span className="hint">{t.noPages + suffix}</span>;
  }
  if (isSplit) {
    const tooLong = parts.findIndex((part) => part.length > MAX_PDF_PAGES);
    if (tooLong >= 0) {
      return (
        <span className="error-text">
          {formatMessage(t.splitTooManyPages, {
            part: tooLong + 1,
            count: parts[tooLong].length,
            max,
          }) + suffix}
        </span>
      );
    }
    const template =
      pages.length === 1
        ? t.willSplitSingle
        : parts.length === 1
          ? t.willSplitOnePart
          : t.willSplitMultiple;
    return (
      <span className="hint">
        {formatMessage(template, {
          pages: pages.length,
          parts: parts.length,
          size: megabytes,
        }) + suffix}
      </span>
    );
  }
  if (pages.length > MAX_PDF_PAGES) {
    return (
      <span className="error-text">
        {formatMessage(t.tooManyPages, { max, count: pages.length }) + suffix}
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
