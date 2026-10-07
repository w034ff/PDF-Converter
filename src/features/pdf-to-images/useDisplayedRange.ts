import { useState } from "react";
import type { CheckPageRangeResult } from "../../ipc";
import type { PageSelection, PdfToImagesState } from "../../state";

/** The page selection the screen shows: the state's, or the last settled one. */
export interface DisplayedRange {
  pageSelection: PageSelection;
  rangeResult: CheckPageRangeResult | null;
}

/**
 * The page selection to show while a new range text waits for its first
 * `check_page_range` answer: the one shown before it.
 *
 * Until that answer the range has no result, and showing that would mark
 * every page as left out for a moment, as when "Select range" fills in the
 * pages of "All". A text that has a stale result while it is rechecked keeps
 * showing that result, as before.
 */
export function useDisplayedRange(state: PdfToImagesState): DisplayedRange {
  const isPending =
    state.pageSelection === "range" &&
    state.rangeText.trim().length > 0 &&
    state.rangeError === null &&
    state.rangeResult === null;
  const [settled, setSettled] = useState<DisplayedRange>({
    pageSelection: state.pageSelection,
    rangeResult: state.rangeResult,
  });
  if (
    !isPending &&
    (settled.pageSelection !== state.pageSelection ||
      settled.rangeResult !== state.rangeResult)
  ) {
    setSettled({
      pageSelection: state.pageSelection,
      rangeResult: state.rangeResult,
    });
  }
  return isPending
    ? settled
    : { pageSelection: state.pageSelection, rangeResult: state.rangeResult };
}
