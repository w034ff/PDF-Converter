import { useEffect, useState } from "react";
import { getThumbnail } from "../../ipc";

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
 * The picture is a `blob:` URL (design §9), released once another replaces
 * it or the component unmounts. A failed request leaves the picture as it
 * was.
 */
export function usePageImage(
  id: number,
  page: number,
  maxSide: number,
): string | null {
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  useEffect(() => {
    let disposed = false;
    getThumbnail(id, page, maxSide).then(
      (bytes) => {
        if (!disposed) {
          const src = URL.createObjectURL(
            new Blob([bytes], { type: PAGE_IMAGE_TYPE }),
          );
          setLoaded({ id, page, src });
        }
      },
      () => {
        // The page shows without a picture; the frames can still be used.
      },
    );
    return () => {
      disposed = true;
    };
  }, [id, page, maxSide]);

  const src = loaded?.src;
  useEffect(() => {
    if (src === undefined) {
      return;
    }
    return () => URL.revokeObjectURL(src);
  }, [src]);

  return loaded !== null && loaded.id === id && loaded.page === page
    ? loaded.src
    : null;
}
