import { useEffect, useState } from "react";
import { getThumbnail } from "../../ipc";
import { cropPageImages } from "./cropImageCaches";

/** MIME type of what `get_thumbnail` returns (design §7.1). */
const PAGE_IMAGE_TYPE = "image/png";

interface Loaded {
  id: number;
  page: number;
  src: string;
}

/**
 * The picture of `page` of the crop PDF `id`, asked for at `maxSide` pixels
 * on its longer side. A new `maxSide` for the same page keeps showing the
 * picture it had until the new one arrives (design §10.1); another page, or
 * another PDF, shows nothing until its own picture is there, so a page is
 * never shown as the one before it.
 *
 * The picture is a `blob:` URL (design §9), which `cropPageImages` owns: one
 * it holds is shown at once and not asked for again. Only a picture still
 * wanted goes in, so pages passed quickly do not push out the one on screen.
 * A failed request leaves the picture as it was.
 */
export function usePageImage(
  id: number,
  page: number,
  maxSide: number,
): string | null {
  const key = `${id}:${page}:${maxSide}`;
  const cached = cropPageImages.peek(key);
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  // Kept as the picture to show until the next size of this page arrives.
  if (cached !== null && loaded?.src !== cached) {
    setLoaded({ id, page, src: cached });
  }

  useEffect(() => {
    if (cropPageImages.peek(key) !== null) {
      cropPageImages.touch(key);
      return;
    }
    let disposed = false;
    getThumbnail(id, page, maxSide).then(
      (bytes) => {
        if (!disposed) {
          const blob = new Blob([bytes], { type: PAGE_IMAGE_TYPE });
          setLoaded({ id, page, src: cropPageImages.put(key, blob) });
        }
      },
      () => {
        // The page shows without a picture; the frames can still be used.
      },
    );
    return () => {
      disposed = true;
    };
  }, [key, id, page, maxSide]);

  if (cached !== null) {
    return cached;
  }
  return loaded !== null && loaded.id === id && loaded.page === page
    ? loaded.src
    : null;
}
