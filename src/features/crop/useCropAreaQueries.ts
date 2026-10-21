import { useEffect } from "react";
import { checkOnTop, listPageImages, normalizeIpcError } from "../../ipc";
import { useAppDispatch, useAppState } from "../../state";

/**
 * Asks Rust for what the photo areas need and the screen has not asked yet
 * (design §6.10): the embedded images of a page with a photo area, whose
 * pixel sizes the list of areas shows, and then whether anything is drawn
 * over each photo (`check_on_top`). Only one is in flight at a time, the
 * images of the lowest page first and then the photos by `key`: both open the
 * PDF in a pool worker, and "全ページの候補" can add photos on every page of
 * a long PDF at once.
 *
 * The page shown asks for its own embedded images at once
 * (`useCropPageData`) and is not held up by this queue, but its answer is
 * waited for like the rest. A request that fails is not repeated: a photo
 * without an answer has no mark, and a page without images has no size.
 *
 * Mount it once, above the screens, for the same reason as
 * `useCropPageData`: an effect of a component mounted per visit would run
 * twice under `StrictMode` and ask twice before the first request is
 * recorded.
 */
export function useCropAreaQueries(): void {
  const { crop } = useAppState();
  const dispatch = useAppDispatch();
  const { pdf, areas, pageImages, onTopRequests } = crop;
  const id = pdf !== null && pdf.error === null ? pdf.id : null;

  const busy =
    Object.values(pageImages).some((result) => result?.status === "pending") ||
    Object.values(onTopRequests).some((request) => request === "pending");

  const photos = areas.flatMap((area) =>
    area.kind === "photo" && area.imageIndex !== null
      ? [
          {
            key: area.key,
            page: area.page,
            imageIndex: area.imageIndex,
            onTop: area.onTop,
          },
        ]
      : [],
  );
  const imagesPage = photos
    .map((photo) => photo.page)
    .filter((page) => pageImages[page] === undefined)
    .sort((a, b) => a - b)
    .at(0);
  const onTopPhoto = photos.find(
    (photo) => photo.onTop === null && onTopRequests[photo.key] === undefined,
  );
  const onTopKey = onTopPhoto?.key;
  const onTopPage = onTopPhoto?.page;
  const onTopImageIndex = onTopPhoto?.imageIndex;

  useEffect(() => {
    if (id === null || busy) {
      return;
    }
    if (imagesPage !== undefined) {
      const page = imagesPage;
      dispatch({ type: "CROP_PAGE_IMAGES_STARTED", id, page });
      listPageImages(id, page).then(
        (images) =>
          dispatch({ type: "CROP_PAGE_IMAGES_DONE", id, page, images }),
        (failure: unknown) =>
          dispatch({
            type: "CROP_PAGE_IMAGES_FAILED",
            id,
            page,
            error: normalizeIpcError(failure),
          }),
      );
      return;
    }
    if (
      onTopKey !== undefined &&
      onTopPage !== undefined &&
      onTopImageIndex !== undefined
    ) {
      const key = onTopKey;
      dispatch({ type: "CROP_ON_TOP_STARTED", id, key });
      checkOnTop(id, onTopPage, onTopImageIndex).then(
        (onTop) => dispatch({ type: "CROP_ON_TOP_DONE", id, key, onTop }),
        () => dispatch({ type: "CROP_ON_TOP_FAILED", id, key }),
      );
    }
  }, [id, busy, imagesPage, onTopKey, onTopPage, onTopImageIndex, dispatch]);
}
