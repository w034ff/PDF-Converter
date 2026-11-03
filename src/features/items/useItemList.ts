import { useState } from "react";
import {
  normalizeIpcError,
  removeItems,
  type AddResult,
  type AddSource,
  type IpcError,
  type Skipped,
} from "../../ipc";

export interface ItemListOptions<T extends { id: number }> {
  /** The items of the list as the screen shows them now. */
  items: readonly T[];
  /** The command that opens the dialog and adds what was chosen. */
  add: (source: AddSource) => Promise<AddResult<T> | null>;
  onAdded: (items: T[]) => void;
  onRemoved: (id: number) => void;
  onCleared: () => void;
}

export interface ItemList {
  /** The error of the last add or remove, until it is dismissed or the next one starts. */
  error: IpcError | null;
  dismissError: () => void;
  /** What the last add left out; "すべて削除" forgets it. */
  skipped: Skipped | null;
  add: (source: AddSource) => Promise<void>;
  remove: (id: number) => Promise<void>;
  clearAll: () => Promise<void>;
}

/**
 * Adding to, removing from and clearing one of the two lists (design §6.1).
 * The state changes only once Rust has done the same to its table, so the
 * two never disagree about which items there are.
 */
export function useItemList<T extends { id: number }>({
  items,
  add,
  onAdded,
  onRemoved,
  onCleared,
}: ItemListOptions<T>): ItemList {
  const [error, setError] = useState<IpcError | null>(null);
  const [skipped, setSkipped] = useState<Skipped | null>(null);

  return {
    error,
    dismissError: () => setError(null),
    skipped,
    async add(source) {
      setError(null);
      try {
        const result = await add(source);
        if (result !== null) {
          if (result.added.length > 0) {
            onAdded(result.added);
          }
          setSkipped(result.skipped);
        }
      } catch (err: unknown) {
        setError(normalizeIpcError(err));
      }
    },
    async remove(id) {
      setError(null);
      try {
        await removeItems([id]);
        onRemoved(id);
      } catch (err: unknown) {
        setError(normalizeIpcError(err));
      }
    },
    async clearAll() {
      setError(null);
      const ids = items.map((item) => item.id);
      try {
        await removeItems(ids);
        onCleared();
        setSkipped(null);
      } catch (err: unknown) {
        setError(normalizeIpcError(err));
      }
    },
  };
}
