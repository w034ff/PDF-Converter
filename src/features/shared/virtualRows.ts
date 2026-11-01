import { useState, type RefObject, type UIEvent } from "react";
import type { Room } from "./useRoom";

/** Rows drawn above and below the ones in view. */
export const VIRTUAL_OVERSCAN_ROWS = 2;

/**
 * How long a page of a long list is drawn before it asks for its thumbnail: a
 * list scrolled far in one move would otherwise queue a render for every page
 * it passed.
 */
export const LIST_THUMBNAIL_DELAY_MS = 100;

/** On the element of a row, the row's index; lets a scroll find the focused row. */
const ROW_ATTRIBUTE = "data-virtual-row";

/**
 * The top of each of `count` rows, and last where a row after them would
 * start, so `offsets[count]` is the height of them all with a gap after each.
 */
export function rowOffsets(
  count: number,
  rowHeight: (row: number) => number,
  gap: number,
): Float64Array {
  const offsets = new Float64Array(count + 1);
  for (let row = 0; row < count; row += 1) {
    offsets[row + 1] = offsets[row] + rowHeight(row) + gap;
  }
  return offsets;
}

/** The row `y` is in; the rows of `offsets` that end above it count as passed. */
function rowAt(offsets: Float64Array, y: number): number {
  let low = 0;
  let high = offsets.length - 1;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (offsets[middle] <= y) {
      low = middle;
    } else {
      high = middle - 1;
    }
  }
  return low;
}

/** The rows to draw, first to last inclusive; `last` is below `first` when there are none. */
export interface VisibleRows {
  first: number;
  last: number;
}

/**
 * The rows in view of a scroll area `viewportHeight` high scrolled to
 * `scrollTop`, and `overscan` rows more on each side, kept within the rows
 * there are.
 */
export function visibleRows(
  offsets: Float64Array,
  scrollTop: number,
  viewportHeight: number,
  overscan: number = VIRTUAL_OVERSCAN_ROWS,
): VisibleRows {
  const count = offsets.length - 1;
  const top = Math.max(scrollTop, 0);
  return {
    first: Math.max(rowAt(offsets, top) - overscan, 0),
    last: Math.min(
      rowAt(offsets, top + Math.max(viewportHeight, 0)) + overscan,
      count - 1,
    ),
  };
}

export interface VirtualRows extends VisibleRows {
  /** The height of the rows not drawn above `first`. */
  before: number;
  /** The height of the rows not drawn below `last`. */
  after: number;
  onScroll: (event: UIEvent<HTMLElement>) => void;
}

/**
 * The rows of `offsets` to draw in the scroll area `scrollRef`, which is
 * focusable. A row with the focus that is about to go passes the focus to the
 * area, so the Tab key goes on from the list instead of the top of the window.
 */
export function useVirtualRows(
  scrollRef: RefObject<HTMLElement | null>,
  room: Room,
  offsets: Float64Array,
): VirtualRows {
  const [scrollTop, setScrollTop] = useState(0);
  const range = visibleRows(offsets, scrollTop, room.height);

  function onScroll(event: UIEvent<HTMLElement>) {
    const next = event.currentTarget.scrollTop;
    const scroller = scrollRef.current;
    const focused = document.activeElement?.closest(`[${ROW_ATTRIBUTE}]`);
    if (scroller !== null && focused != null && scroller.contains(focused)) {
      const row = Number(focused.getAttribute(ROW_ATTRIBUTE));
      const { first, last } = visibleRows(offsets, next, room.height);
      if (row < first || row > last) {
        scroller.focus({ preventScroll: true });
      }
    }
    setScrollTop(next);
  }

  const count = offsets.length - 1;
  return {
    ...range,
    before: offsets[range.first],
    after: offsets[count] - offsets[range.last + 1],
    onScroll,
  };
}
