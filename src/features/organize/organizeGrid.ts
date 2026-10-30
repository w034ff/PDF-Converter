import type { PageSizePt } from "../../ipc";
import { keyOfPage, type OrganizePage, type PageKey } from "../../state";

/** Width of a cell of the grid, in CSS px (mockup OrganizeSingle). */
export const ORGANIZE_CELL_WIDTH = 124;

/** The page box inside a cell is a square of this side. */
export const ORGANIZE_PAGE_BOX_SIDE = 124;

/** The longer side of a page is fitted to this, in CSS px. */
export const ORGANIZE_PAGE_MAX_SIDE = 119;

/** Between the page box and the caption under it. */
export const ORGANIZE_CAPTION_GAP = 6;

/** The caption's line; fixed so the cell's height is. */
export const ORGANIZE_CAPTION_HEIGHT = 18;

/** Height of a cell: the page box and its caption. */
export const ORGANIZE_CELL_HEIGHT =
  ORGANIZE_PAGE_BOX_SIDE + ORGANIZE_CAPTION_GAP + ORGANIZE_CAPTION_HEIGHT;

/**
 * Between two cells of a row. The split lines of the next task are drawn in
 * this space, so it is wide enough to press.
 */
export const ORGANIZE_COLUMN_GAP = 16;

export const ORGANIZE_ROW_GAP = 18;

/** Rows drawn above and below the ones in view. */
export const ORGANIZE_OVERSCAN_ROWS = 2;

/** How long a cell is drawn before it asks for its thumbnail (organize §4). */
export const ORGANIZE_THUMBNAIL_DELAY_MS = 150;

/** Within this many px of the top or bottom edge a drag scrolls the grid. */
export const ORGANIZE_DRAG_SCROLL_EDGE_PX = 40;

/** The most px a drag scrolls in one frame, at the edge itself. */
export const ORGANIZE_DRAG_SCROLL_MAX_STEP_PX = 24;

const QUARTER_TURN = 90;
const HALF = 2;

/** How many cells fit in a row of `width` CSS px; at least one. */
export function gridColumnCount(width: number): number {
  return Math.max(
    1,
    Math.floor(
      (width + ORGANIZE_COLUMN_GAP) /
        (ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP),
    ),
  );
}

export function gridRowCount(count: number, columns: number): number {
  return Math.ceil(count / columns);
}

/** The height of `rows` rows of cells. */
export function gridContentHeight(rows: number): number {
  return rows === 0
    ? 0
    : rows * ORGANIZE_CELL_HEIGHT + (rows - 1) * ORGANIZE_ROW_GAP;
}

export function rowOfIndex(index: number, columns: number): number {
  return Math.floor(index / columns);
}

/** Where the cell at `index` is, from the top left of the grid. */
export function cellOffset(
  index: number,
  columns: number,
): { left: number; top: number } {
  return {
    left: (index % columns) * (ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP),
    top: rowOfIndex(index, columns) * (ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP),
  };
}

/** The rows to draw, first to last inclusive. */
export interface RowRange {
  first: number;
  last: number;
}

/**
 * The rows in view of a scroll area `viewportHeight` high scrolled to
 * `scrollTop`, and `overscan` rows more on each side, kept within the
 * `rowCount` rows there are. `null` when there are none.
 */
export function visibleRowRange(
  scrollTop: number,
  viewportHeight: number,
  rowCount: number,
  overscan: number = ORGANIZE_OVERSCAN_ROWS,
): RowRange | null {
  if (rowCount <= 0) {
    return null;
  }
  const pitch = ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP;
  const top = Math.max(scrollTop, 0);
  const firstInView = Math.floor(top / pitch);
  const lastInView = Math.floor((top + Math.max(viewportHeight, 0)) / pitch);
  return {
    first: Math.min(Math.max(firstInView - overscan, 0), rowCount - 1),
    last: Math.min(Math.max(lastInView + overscan, 0), rowCount - 1),
  };
}

/** The indices of the cells of the rows of `range`, from the first to the last (exclusive). */
export function indexRangeOfRows(
  range: RowRange,
  columns: number,
  count: number,
): { start: number; end: number } {
  return {
    start: range.first * columns,
    end: Math.min((range.last + 1) * columns, count),
  };
}

/** Where a dragged block goes in the grid: its place among the pages and where the line is drawn. */
export interface InsertPoint {
  /** A position among the pages as they are before the move, from 0 to the page count. */
  index: number;
  /** The row the line is in. */
  row: number;
  /** Which gap of the row: 0 is before its first cell. */
  slot: number;
}

/**
 * The place a pointer at (`x`, `y`), from the top left of the grid's content
 * (scrolling included), picks to put pages: the gap between two cells that is
 * nearest to it in its row. Below the last row it is the last row; past the
 * last cell of a row, after that cell. Nothing in the DOM is asked, since
 * cells out of view are not drawn.
 */
export function insertPointAt(
  x: number,
  y: number,
  columns: number,
  count: number,
): InsertPoint {
  if (count <= 0) {
    return { index: 0, row: 0, slot: 0 };
  }
  const lastRow = rowOfIndex(count - 1, columns);
  const row = Math.min(
    Math.max(Math.floor(y / (ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP)), 0),
    lastRow,
  );
  const inRow = Math.min(columns, count - row * columns);
  const pitch = ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP;
  const slot = Math.min(
    Math.max(Math.round((x + ORGANIZE_COLUMN_GAP / HALF) / pitch), 0),
    inRow,
  );
  return { index: row * columns + slot, row, slot };
}

/**
 * Where the line of a drop in gap `slot` of a row is drawn. The gap before the
 * first cell is outside the grid, and the padding of the scrolling area (4 px)
 * would clip a line placed by the pitch alone, so it stops at this.
 */
const ORGANIZE_DROP_LINE_MIN_LEFT = -3;

export function dropLineLeft(slot: number): number {
  return Math.max(
    slot * (ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP) -
      ORGANIZE_COLUMN_GAP / HALF,
    ORGANIZE_DROP_LINE_MIN_LEFT,
  );
}

/** The size a page is drawn at in its box, and the size of the picture inside it before it is turned. */
export interface ThumbnailBox {
  /** The box, turned. */
  width: number;
  height: number;
  /** The picture, as the page is: turn it by `rotation` inside the box. */
  pictureWidth: number;
  pictureHeight: number;
}

/**
 * The box a page takes in a cell: its longer side is `ORGANIZE_PAGE_MAX_SIDE`,
 * and a page turned by 90 or 270 degrees swaps its sides. The same size
 * before the picture arrives as after, since it is made from the page's size.
 */
export function thumbnailBox(size: PageSizePt, rotation: number): ThumbnailBox {
  const longSide = Math.max(size.widthPt, size.heightPt);
  const scale = longSide > 0 ? ORGANIZE_PAGE_MAX_SIDE / longSide : 0;
  const pictureWidth = Math.max(1, Math.round(size.widthPt * scale));
  const pictureHeight = Math.max(1, Math.round(size.heightPt * scale));
  const sideways = rotation % (QUARTER_TURN * HALF) !== 0;
  return {
    width: sideways ? pictureHeight : pictureWidth,
    height: sideways ? pictureWidth : pictureHeight,
    pictureWidth,
    pictureHeight,
  };
}

/** A key that moves the focus in the grid. */
export type FocusKey =
  "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End";

export function isFocusKey(key: string): key is FocusKey {
  return (
    key === "ArrowLeft" ||
    key === "ArrowRight" ||
    key === "ArrowUp" ||
    key === "ArrowDown" ||
    key === "Home" ||
    key === "End"
  );
}

/** Where `key` takes the focus from `index`, kept within the `count` pages. */
export function focusIndexAfter(
  key: FocusKey,
  index: number,
  columns: number,
  count: number,
): number {
  const last = count - 1;
  switch (key) {
    case "ArrowLeft":
      return Math.max(index - 1, 0);
    case "ArrowRight":
      return Math.min(index + 1, last);
    case "ArrowUp":
      return index - columns < 0 ? index : index - columns;
    case "ArrowDown":
      return index + columns > last ? index : index + columns;
    case "Home":
      return 0;
    case "End":
      return last;
  }
}

/**
 * Where Alt and an arrow put the pages of `keys`: left and up put them in
 * front of the page `step` places before the first of them, right and down
 * behind the page `step` places after the last, and an edge stops them there.
 * The result is for `moveSelected`: a position among the pages as they are.
 */
export function keyInsertIndex(
  pages: readonly OrganizePage[],
  keys: ReadonlySet<PageKey>,
  direction: "before" | "after",
  step: number,
): number {
  let first = -1;
  let last = -1;
  pages.forEach((page, index) => {
    if (keys.has(keyOfPage(page))) {
      first = first < 0 ? index : first;
      last = index;
    }
  });
  if (first < 0) {
    return 0;
  }
  return direction === "before"
    ? Math.max(first - step, 0)
    : Math.min(last + 1 + step, pages.length);
}

/**
 * The scroll position that shows row `row` whole, or `scrollTop` itself when
 * it is shown already (so a page in view does not make the grid jump).
 */
export function scrollTopToShowRow(
  row: number,
  scrollTop: number,
  viewportHeight: number,
): number {
  const top = row * (ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP);
  const bottom = top + ORGANIZE_CELL_HEIGHT;
  if (top < scrollTop) {
    return top;
  }
  if (bottom > scrollTop + viewportHeight) {
    // A room lower than a row shows its top, not its bottom.
    return Math.max(Math.min(bottom - viewportHeight, top), 0);
  }
  return scrollTop;
}

/**
 * How far one frame of a drag scrolls, in px: negative up. Zero in the middle
 * of the area; within `ORGANIZE_DRAG_SCROLL_EDGE_PX` of an edge the nearer it
 * is the faster, up to `ORGANIZE_DRAG_SCROLL_MAX_STEP_PX` at (or beyond) the edge.
 */
export function dragScrollStep(
  pointerY: number,
  areaTop: number,
  areaBottom: number,
): number {
  const intoTop = ORGANIZE_DRAG_SCROLL_EDGE_PX - (pointerY - areaTop);
  if (intoTop > 0) {
    return -Math.ceil(
      (Math.min(intoTop, ORGANIZE_DRAG_SCROLL_EDGE_PX) /
        ORGANIZE_DRAG_SCROLL_EDGE_PX) *
        ORGANIZE_DRAG_SCROLL_MAX_STEP_PX,
    );
  }
  const intoBottom = ORGANIZE_DRAG_SCROLL_EDGE_PX - (areaBottom - pointerY);
  if (intoBottom > 0) {
    return Math.ceil(
      (Math.min(intoBottom, ORGANIZE_DRAG_SCROLL_EDGE_PX) /
        ORGANIZE_DRAG_SCROLL_EDGE_PX) *
        ORGANIZE_DRAG_SCROLL_MAX_STEP_PX,
    );
  }
  return 0;
}
