import { getThumbnail } from "../../ipc";

/** MIME type of what `get_thumbnail` returns (design §7.1). */
const PREVIEW_IMAGE_TYPE = "image/png";

interface Entry {
  /** How many rows hold the picture; it is released when none does. */
  holders: number;
  /** The `blob:` URL once the picture has arrived; `null` while it is on its way or failed. */
  url: string | null;
}

/**
 * The pages' pictures the rows of the list of areas cut their previews from
 * (design §10.1), one per PDF, page and size, whichever rows use them. Rows
 * of the same page and size share one request and one `blob:` URL, so
 * moving an area does not make every row ask again.
 */
const entries = new Map<string, Entry>();
const listeners = new Set<() => void>();

function entryKey(id: number, page: number, maxSide: number): string {
  return `${id}:${page}:${maxSide}`;
}

function notify() {
  for (const listener of listeners) {
    listener();
  }
}

/** For `useSyncExternalStore`: calls `listener` whenever a picture arrives or goes. */
export function subscribePreviewImages(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The `blob:` URL of the picture, or `null` while it has not arrived, failed or is not held. */
export function previewImageUrl(
  id: number,
  page: number,
  maxSide: number,
): string | null {
  return entries.get(entryKey(id, page, maxSide))?.url ?? null;
}

/**
 * Holds the picture of `page` of the crop PDF `id` at `maxSide`, asking for
 * it if no row has. Returns the function that lets go. A picture nobody
 * holds is freed (`URL.revokeObjectURL`) after the current task, so a row
 * that lets go and holds again, as `StrictMode` does to an effect, keeps it.
 * A request that fails is not repeated while the picture is held.
 */
export function holdPreviewImage(
  id: number,
  page: number,
  maxSide: number,
): () => void {
  const key = entryKey(id, page, maxSide);
  let entry = entries.get(key);
  if (entry === undefined) {
    const created: Entry = { holders: 0, url: null };
    entry = created;
    entries.set(key, created);
    getThumbnail(id, page, maxSide).then(
      (bytes) => {
        // Nobody holds it any more: the picture is not wanted.
        if (entries.get(key) !== created) {
          return;
        }
        created.url = URL.createObjectURL(
          new Blob([bytes], { type: PREVIEW_IMAGE_TYPE }),
        );
        notify();
      },
      () => {
        // The row shows without a picture.
      },
    );
  }
  const held = entry;
  held.holders += 1;
  let released = false;
  return () => {
    if (released) {
      return;
    }
    released = true;
    held.holders -= 1;
    if (held.holders > 0) {
      return;
    }
    queueMicrotask(() => {
      if (held.holders > 0 || entries.get(key) !== held) {
        return;
      }
      entries.delete(key);
      if (held.url !== null) {
        URL.revokeObjectURL(held.url);
        notify();
      }
    });
  };
}
