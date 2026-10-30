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

export interface ThumbnailOptions {
  /** How long to wait, once in view, before asking; asks at once without it. */
  delayMs?: number;
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
 * With `delayMs` the request waits that long once the element is in view, and
 * is not sent if the element is gone by then (organize §4): a grid scrolled
 * far in one move would otherwise queue a render for every page it passed.
 *
 * The picture is shown as a `blob:` URL (design §9), released when the item
 * changes or the component unmounts.
 */
export function useThumbnail(
  id: number,
  page?: number,
  options: ThumbnailOptions = {},
): Thumbnail {
  const { delayMs } = options;
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
    function request() {
      getThumbnail(id, page).then(
        (bytes) => {
          if (disposed) {
            return;
          }
          url = URL.createObjectURL(
            new Blob([bytes], { type: THUMBNAIL_TYPE }),
          );
          setLoaded({ key, src: url, failed: false });
        },
        () => {
          if (!disposed) {
            setLoaded({ key, src: null, failed: true });
          }
        },
      );
    }
    const timer = delayMs === undefined ? null : setTimeout(request, delayMs);
    if (timer === null) {
      request();
    }
    return () => {
      disposed = true;
      if (timer !== null) {
        clearTimeout(timer);
      }
      if (url !== null) {
        URL.revokeObjectURL(url);
      }
    };
  }, [visible, id, page, key, delayMs]);

  const current = loaded?.key === key ? loaded : null;
  return { ref, src: current?.src ?? null, failed: current?.failed ?? false };
}
