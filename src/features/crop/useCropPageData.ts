import { useEffect } from "react";
import { detectLayout, listPageImages, normalizeIpcError } from "../../ipc";
import { useAppDispatch, useAppState } from "../../state";

/**
 * Asks Rust for the embedded images and the layout candidates of the page
 * shown (design §6.10), each once per page, and keeps the answers in the crop
 * state until the PDF is replaced.
 *
 * The embedded images are cheap and asked at once. The layout analysis is
 * asked for one page at a time: while an answer is awaited the next page is
 * not asked, and when it arrives the page shown by then is asked, so pages
 * flipped through meanwhile are skipped (they are asked when shown again) and
 * the analysis worker does not pile up requests that delay the page in view.
 * Nothing is asked after `LayoutUnavailable`, which Rust answers for every
 * page alike.
 *
 * Mount it once, above the screens, with `enabled` telling whether the crop
 * tab is shown: a page is asked about only while it is shown, and an effect
 * of a component mounted per visit would run twice under `StrictMode` and ask
 * twice before the first request is recorded.
 */
export function useCropPageData(enabled: boolean): void {
  const { crop } = useAppState();
  const dispatch = useAppDispatch();
  const { pdf, page, pageImages, layouts, layoutUnavailable } = crop;
  const id = enabled && pdf !== null && pdf.error === null ? pdf.id : null;

  const imagesRequested = pageImages[page] !== undefined;
  useEffect(() => {
    if (id === null || imagesRequested) {
      return;
    }
    dispatch({ type: "CROP_PAGE_IMAGES_STARTED", id, page });
    listPageImages(id, page).then(
      (images) => dispatch({ type: "CROP_PAGE_IMAGES_DONE", id, page, images }),
      (failure: unknown) =>
        dispatch({
          type: "CROP_PAGE_IMAGES_FAILED",
          id,
          page,
          error: normalizeIpcError(failure),
        }),
    );
  }, [id, page, imagesRequested, dispatch]);

  const layoutRequested = layouts[page] !== undefined;
  const layoutBusy = Object.values(layouts).some(
    (result) => result?.status === "pending",
  );
  useEffect(() => {
    if (id === null || layoutRequested || layoutBusy || layoutUnavailable) {
      return;
    }
    dispatch({ type: "CROP_LAYOUT_STARTED", id, page });
    detectLayout(id, page).then(
      (candidates) =>
        dispatch({ type: "CROP_LAYOUT_DONE", id, page, candidates }),
      (failure: unknown) =>
        dispatch({
          type: "CROP_LAYOUT_FAILED",
          id,
          page,
          error: normalizeIpcError(failure),
        }),
    );
  }, [id, page, layoutRequested, layoutBusy, layoutUnavailable, dispatch]);
}
