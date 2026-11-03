import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";

function findRowElement(target: EventTarget | null): HTMLElement | null {
  if (target instanceof Element) {
    const row = target.closest(".list-row");
    if (row instanceof HTMLElement) {
      return row;
    }
  }
  return null;
}

/** Where the row `target` is in, among the rows of `list`; `null` when it is in none. */
export function rowIndexOf(
  list: HTMLElement | null,
  target: EventTarget | null,
): number | null {
  const row = findRowElement(target);
  if (list === null || row === null) {
    return null;
  }
  const index = Array.from(list.children).indexOf(row);
  return index >= 0 ? index : null;
}

function rowIndexAt(list: HTMLElement | null, x: number, y: number) {
  return rowIndexOf(list, document.elementFromPoint?.(x, y) ?? null);
}

function releaseCapture(e: ReactPointerEvent<HTMLElement>) {
  try {
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  } catch {
    // Pointer capture is missing in test environments.
  }
}

/** The class of row `index` while row `dragIndex` is dragged over row `overIndex`. */
export function rowDropClass(
  index: number,
  dragIndex: number | null,
  overIndex: number | null,
): string | undefined {
  if (dragIndex === null) {
    return undefined;
  }
  if (index === dragIndex) {
    return "is-dragging";
  }
  if (overIndex !== null && index === overIndex) {
    if (dragIndex < overIndex) {
      return "is-drop-after";
    }
    if (dragIndex > overIndex) {
      return "is-drop-before";
    }
  }
  return undefined;
}

export interface RowDrag {
  /** The row being dragged, or `null` when no drag is on. */
  dragIndex: number | null;
  /** The row the dragged one would go to. */
  overIndex: number | null;
  /** For the list element. */
  handlers: {
    onPointerDown: (e: ReactPointerEvent<HTMLElement>) => void;
    onPointerMove: (e: ReactPointerEvent<HTMLElement>) => void;
    onPointerUp: (e: ReactPointerEvent<HTMLElement>) => void;
    onPointerCancel: (e: ReactPointerEvent<HTMLElement>) => void;
  };
}

/**
 * Reorders the rows of `listRef` by dragging them with the main button; a
 * press on a button of a row is left to the button. `onMove` gets the row
 * dragged and the row it was dropped on.
 *
 * The list captures the pointer, so in a browser its own events target the
 * list and not a row; listeners on the window follow the pointer by its
 * position instead, also outside the list. Without pointer capture (as in
 * jsdom) the list's events find the row from their target.
 */
export function useRowDrag(
  listRef: RefObject<HTMLElement | null>,
  disabled: boolean,
  onMove: (fromIndex: number, toIndex: number) => void,
): RowDrag {
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);
  const dragIndexRef = useRef<number | null>(null);
  const overIndexRef = useRef<number | null>(null);
  const onMoveRef = useRef(onMove);
  useEffect(() => {
    onMoveRef.current = onMove;
  });

  const hover = useCallback((index: number | null) => {
    if (index !== null && index !== overIndexRef.current) {
      overIndexRef.current = index;
      setOverIndex(index);
    }
  }, []);

  const reset = useCallback(() => {
    dragIndexRef.current = null;
    overIndexRef.current = null;
    setDragIndex(null);
    setOverIndex(null);
  }, []);

  /** Ends the drag on row `index`, or on the last row hovered when `null`. */
  const drop = useCallback(
    (index: number | null) => {
      const from = dragIndexRef.current;
      const to = index ?? overIndexRef.current;
      if (from !== null && to !== null && to !== from) {
        onMoveRef.current(from, to);
      }
      reset();
    },
    [reset],
  );

  useEffect(() => {
    if (dragIndex === null) {
      return;
    }
    const onWindowPointerMove = (e: PointerEvent) => {
      hover(rowIndexAt(listRef.current, e.clientX, e.clientY));
    };
    const onWindowPointerUp = (e: PointerEvent) => {
      drop(rowIndexAt(listRef.current, e.clientX, e.clientY));
    };
    window.addEventListener("pointermove", onWindowPointerMove);
    window.addEventListener("pointerup", onWindowPointerUp);
    window.addEventListener("pointercancel", reset);
    return () => {
      window.removeEventListener("pointermove", onWindowPointerMove);
      window.removeEventListener("pointerup", onWindowPointerUp);
      window.removeEventListener("pointercancel", reset);
    };
  }, [dragIndex, listRef, hover, drop, reset]);

  return {
    dragIndex,
    overIndex,
    handlers: {
      onPointerDown(e) {
        if (disabled || e.button !== 0) {
          return;
        }
        if (e.target instanceof Element && e.target.closest("button")) {
          return;
        }
        const index = rowIndexOf(listRef.current, e.target);
        if (index === null) {
          return;
        }
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {
          // Pointer capture is missing in test environments.
        }
        dragIndexRef.current = index;
        overIndexRef.current = index;
        setDragIndex(index);
        setOverIndex(index);
      },
      onPointerMove(e) {
        if (dragIndexRef.current !== null) {
          hover(rowIndexOf(listRef.current, e.target));
        }
      },
      onPointerUp(e) {
        releaseCapture(e);
        if (dragIndexRef.current !== null) {
          drop(rowIndexOf(listRef.current, e.target));
        }
      },
      onPointerCancel(e) {
        releaseCapture(e);
        reset();
      },
    },
  };
}
