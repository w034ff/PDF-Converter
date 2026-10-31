/**
 * Thumbnails kept after the elements that showed them are gone, as `blob:`
 * URLs the cache owns, the least recently used going first. A URL may be
 * released while an image still shows it: a loaded image no longer needs it.
 */
export interface ThumbnailCache {
  /** The URL of `key`, or `null`; does not count as a use, so it can be read while rendering. */
  peek(key: string): string | null;
  /** Counts as a use of `key`, which stays longer. */
  touch(key: string): void;
  /** Keeps `blob` under `key` unless one is there already, and returns the URL `key` has. */
  put(key: string, blob: Blob): string;
  /** Releases every URL. */
  clear(): void;
}

/** A cache of at most `capacity` thumbnails. */
export function createThumbnailCache(capacity: number): ThumbnailCache {
  // A Map keeps its insertion order, so the first key is the least recently used.
  const urls = new Map<string, string>();

  return {
    peek: (key) => urls.get(key) ?? null,
    touch(key) {
      const url = urls.get(key);
      if (url !== undefined) {
        urls.delete(key);
        urls.set(key, url);
      }
    },
    put(key, blob) {
      const existing = urls.get(key);
      if (existing !== undefined) {
        return existing;
      }
      const url = URL.createObjectURL(blob);
      urls.set(key, url);
      for (const [oldest, oldestUrl] of urls) {
        if (urls.size <= capacity) {
          break;
        }
        URL.revokeObjectURL(oldestUrl);
        urls.delete(oldest);
      }
      return url;
    },
    clear() {
      for (const url of urls.values()) {
        URL.revokeObjectURL(url);
      }
      urls.clear();
    },
  };
}
