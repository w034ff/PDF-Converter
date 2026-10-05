import { useCallback, useEffect, useState } from "react";
import { getThumbnail } from "../../ipc";

/** MIME type of what `get_thumbnail` returns (design §7.1). */
const THUMBNAIL_TYPE = "image/png";

export interface Thumbnail {
  /** Attach to the element whose appearance on screen triggers the request. */
  ref: (element: Element | null) => void;
  /** A `blob:` URL for `<img src>`, or `null` until it has loaded. */
  src: string | null;
  /** True if `get_thumbnail` failed; the row then shows no picture. */
  failed: boolean;
}

interface Loaded {
  key: string;
  src: string | null;
  failed: boolean;
}

/**
 * The thumbnail of item `id` (and `page` of a PDF), requested only once the
 * element `ref` is attached to comes into view (design §6.1), so adding many
 * files does not queue a request for each. Without `IntersectionObserver`
 * the request is sent at once.
 *
 * The picture is shown as a `blob:` URL (design §9), released when the item
 * changes or the component unmounts.
 */
export function useThumbnail(id: number, page?: number): Thumbnail {
  // What was seen and loaded is tagged with the item it belongs to, so a
  // change of item drops both without resetting state in an effect.
  const key = `${id}:${page ?? ""}`;
  const observable = typeof IntersectionObserver !== "undefined";
  const [element, setElement] = useState<Element | null>(null);
  const [seenKey, setSeenKey] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const visible = !observable || seenKey === key;

  const ref = useCallback((next: Element | null) => setElement(next), []);

  useEffect(() => {
    if (!observable || element === null || seenKey === key) {
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        setSeenKey(key);
        observer.disconnect();
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [observable, element, seenKey, key]);

  useEffect(() => {
    if (!visible) {
      return;
    }
    let url: string | null = null;
    let disposed = false;
    getThumbnail(id, page).then(
      (bytes) => {
        if (disposed) {
          return;
        }
        url = URL.createObjectURL(new Blob([bytes], { type: THUMBNAIL_TYPE }));
        setLoaded({ key, src: url, failed: false });
      },
      () => {
        if (!disposed) {
          setLoaded({ key, src: null, failed: true });
        }
      },
    );
    return () => {
      disposed = true;
      if (url !== null) {
        URL.revokeObjectURL(url);
      }
    };
  }, [visible, id, page, key]);

  const current = loaded?.key === key ? loaded : null;
  return { ref, src: current?.src ?? null, failed: current?.failed ?? false };
}
