import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import {
  holdPreviewImage,
  previewImageUrl,
  subscribePreviewImages,
} from "./previewImages";

export interface AreaPreviewImage {
  /** Attach to the element whose appearance on screen triggers the request. */
  ref: (element: Element | null) => void;
  /** A `blob:` URL for `<img src>`, or `null` until a picture has arrived. */
  src: string | null;
}

interface Size {
  id: number;
  page: number;
  maxSide: number;
}

/**
 * The picture of `page` of the crop PDF `id` at `maxSide`, for the preview of
 * a row of the list of areas (design §10.1). It is requested once the element
 * `ref` is attached to has come into view (without `IntersectionObserver`,
 * at once), and from then on at every size the row needs, without waiting to
 * be seen again. Rows of the same page and size share it (`previewImages`).
 *
 * A new `maxSide` of the same page keeps the picture it had until the new one
 * arrives, so a preview does not go blank while its area is resized. Another
 * page, or another PDF, shows nothing until its own picture is there.
 */
export function useAreaPreview(
  id: number,
  page: number,
  maxSide: number,
): AreaPreviewImage {
  const observable = typeof IntersectionObserver !== "undefined";
  const [element, setElement] = useState<Element | null>(null);
  const [seen, setSeen] = useState(false);
  const visible = !observable || seen;
  const ref = useCallback((next: Element | null) => setElement(next), []);

  useEffect(() => {
    if (!observable || element === null || seen) {
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        setSeen(true);
        observer.disconnect();
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [observable, element, seen]);

  useEffect(() => {
    if (!visible) {
      return;
    }
    return holdPreviewImage(id, page, maxSide);
  }, [visible, id, page, maxSide]);

  const wanted = useSyncExternalStore(subscribePreviewImages, () =>
    previewImageUrl(id, page, maxSide),
  );

  // The size whose picture is on show. It stays held until the picture of a
  // newer size has arrived to take its place.
  const [shown, setShown] = useState<Size | null>(null);
  if (
    wanted !== null &&
    (shown === null ||
      shown.id !== id ||
      shown.page !== page ||
      shown.maxSide !== maxSide)
  ) {
    setShown({ id, page, maxSide });
  }
  const shownId = shown?.id;
  const shownPage = shown?.page;
  const shownMaxSide = shown?.maxSide;
  useEffect(() => {
    if (
      shownId === undefined ||
      shownPage === undefined ||
      shownMaxSide === undefined
    ) {
      return;
    }
    return holdPreviewImage(shownId, shownPage, shownMaxSide);
  }, [shownId, shownPage, shownMaxSide]);

  const previous = useSyncExternalStore(subscribePreviewImages, () =>
    shown === null
      ? null
      : previewImageUrl(shown.id, shown.page, shown.maxSide),
  );
  const keepsPrevious =
    shown !== null && shown.id === id && shown.page === page;
  return { ref, src: wanted ?? (keepsPrevious ? previous : null) };
}
