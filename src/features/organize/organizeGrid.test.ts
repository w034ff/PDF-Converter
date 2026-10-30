import { describe, expect, it } from "vitest";
import { pageKey, type OrganizePage } from "../../state";
import {
  ORGANIZE_CELL_HEIGHT,
  ORGANIZE_CELL_WIDTH,
  ORGANIZE_COLUMN_GAP,
  ORGANIZE_DRAG_SCROLL_EDGE_PX,
  ORGANIZE_DRAG_SCROLL_MAX_STEP_PX,
  ORGANIZE_OVERSCAN_ROWS,
  ORGANIZE_PAGE_MAX_SIDE,
  ORGANIZE_ROW_GAP,
  breakButtonBox,
  breakGap,
  breakMarkLeft,
  cellOffset,
  dragScrollStep,
  dropLineLeft,
  scrollTopToCenterRow,
  focusIndexAfter,
  gridColumnCount,
  gridContentHeight,
  gridRowCount,
  indexRangeOfRows,
  insertPointAt,
  keyInsertIndex,
  scrollTopToShowRow,
  thumbnailBox,
  visibleRowRange,
} from "./organizeGrid";

const PITCH_X = ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP;
const PITCH_Y = ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP;

describe("the number of columns", () => {
  it("fits as many cells as the width and the gaps between them allow", () => {
    expect(gridColumnCount(ORGANIZE_CELL_WIDTH)).toBe(1);
    // One cell and a gap and a cell is 124 + 16 + 124 = 264.
    expect(
      gridColumnCount(2 * ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP - 1),
    ).toBe(1);
    expect(gridColumnCount(2 * ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP)).toBe(
      2,
    );
    expect(
      gridColumnCount(5 * ORGANIZE_CELL_WIDTH + 4 * ORGANIZE_COLUMN_GAP),
    ).toBe(5);
    expect(gridColumnCount(5 * PITCH_X - ORGANIZE_COLUMN_GAP - 1)).toBe(4);
  });

  it("is at least one, however narrow the room", () => {
    expect(gridColumnCount(0)).toBe(1);
    expect(gridColumnCount(40)).toBe(1);
    expect(gridColumnCount(-10)).toBe(1);
  });
});

describe("the size of the grid", () => {
  it("counts the rows a number of pages makes", () => {
    expect(gridRowCount(0, 5)).toBe(0);
    expect(gridRowCount(1, 5)).toBe(1);
    expect(gridRowCount(5, 5)).toBe(1);
    expect(gridRowCount(6, 5)).toBe(2);
    expect(gridRowCount(10_000, 7)).toBe(1429);
  });

  it("is as high as its rows and the gaps between them", () => {
    expect(gridContentHeight(0)).toBe(0);
    expect(gridContentHeight(1)).toBe(ORGANIZE_CELL_HEIGHT);
    expect(gridContentHeight(3)).toBe(
      3 * ORGANIZE_CELL_HEIGHT + 2 * ORGANIZE_ROW_GAP,
    );
  });

  it("puts a cell at its column and its row", () => {
    expect(cellOffset(0, 4)).toEqual({ left: 0, top: 0 });
    expect(cellOffset(3, 4)).toEqual({ left: 3 * PITCH_X, top: 0 });
    expect(cellOffset(4, 4)).toEqual({ left: 0, top: PITCH_Y });
    expect(cellOffset(10, 4)).toEqual({ left: 2 * PITCH_X, top: 2 * PITCH_Y });
  });
});

describe("the rows in view", () => {
  it("is the rows the scroll area shows, and the overscan on both sides", () => {
    // 600 px high at the top: rows 0 to 3 are in view (the 4th starts at 3 × 166).
    expect(visibleRowRange(0, 600, 100)).toEqual({
      first: 0,
      last: 3 + ORGANIZE_OVERSCAN_ROWS,
    });
    expect(visibleRowRange(10 * PITCH_Y, 600, 100)).toEqual({
      first: 10 - ORGANIZE_OVERSCAN_ROWS,
      last: 13 + ORGANIZE_OVERSCAN_ROWS,
    });
  });

  it("stops at the first and the last row", () => {
    expect(visibleRowRange(0, 600, 3)).toEqual({ first: 0, last: 2 });
    expect(visibleRowRange(99 * PITCH_Y, 600, 100)).toEqual({
      first: 99 - ORGANIZE_OVERSCAN_ROWS,
      last: 99,
    });
  });

  it("includes a row that is in view by its last pixel", () => {
    // Row 3 starts at 3 × 166 = 498; an area ending at 498 shows its first pixel.
    expect(visibleRowRange(0, 498, 100, 0)).toEqual({ first: 0, last: 3 });
    expect(visibleRowRange(0, 497, 100, 0)).toEqual({ first: 0, last: 2 });
  });

  it("has no rows for no pages, and is not thrown by a scroll past the end or an unmeasured area", () => {
    expect(visibleRowRange(0, 600, 0)).toBeNull();
    expect(visibleRowRange(99_999, 600, 4)).toEqual({ first: 3, last: 3 });
    expect(visibleRowRange(-50, 0, 10)).toEqual({
      first: 0,
      last: ORGANIZE_OVERSCAN_ROWS,
    });
  });

  it("gives the cells of the rows, up to the last page", () => {
    expect(indexRangeOfRows({ first: 2, last: 4 }, 5, 1000)).toEqual({
      start: 10,
      end: 25,
    });
    expect(indexRangeOfRows({ first: 2, last: 4 }, 5, 22)).toEqual({
      start: 10,
      end: 22,
    });
  });

  it("draws the same few cells whether there are a hundred pages or ten thousand", () => {
    const columns = 7;
    const drawn = (count: number, scrollTop: number) => {
      const range = visibleRowRange(
        scrollTop,
        600,
        gridRowCount(count, columns),
      );
      if (range === null) {
        return 0;
      }
      const { start, end } = indexRangeOfRows(range, columns, count);
      return end - start;
    };

    expect(drawn(10_000, 300 * PITCH_Y)).toBe(drawn(10_000, 700 * PITCH_Y));
    expect(drawn(10_000, 700 * PITCH_Y)).toBeLessThanOrEqual(7 * 9);
    expect(drawn(100, 0)).toBe(drawn(10_000, 0));
  });
});

describe("where dragged pages go", () => {
  // 4 columns, 10 pages: rows of 4, 4 and 2.
  const at = (x: number, y: number) => insertPointAt(x, y, 4, 10);

  it("is the gap between cells nearest to the pointer, in its row", () => {
    expect(at(0, 10)).toEqual({ index: 0, row: 0, slot: 0 });
    // The middle of the first cell is nearer to its left gap than to its right one up to the middle.
    expect(at(ORGANIZE_CELL_WIDTH / 2 - 1, 10).index).toBe(0);
    expect(at(ORGANIZE_CELL_WIDTH / 2 + 1, 10).index).toBe(1);
    expect(at(PITCH_X + 5, 10).index).toBe(1);
    expect(at(2 * PITCH_X + 100, PITCH_Y + 10)).toEqual({
      index: 4 + 3,
      row: 1,
      slot: 3,
    });
  });

  it("at the end of a full row is its end, which is the start of the next row's index", () => {
    expect(at(10_000, 10)).toEqual({ index: 4, row: 0, slot: 4 });
    expect(at(10_000, PITCH_Y + 10)).toEqual({ index: 8, row: 1, slot: 4 });
  });

  it("after the last cell of the last row is after the last page", () => {
    expect(at(10_000, 2 * PITCH_Y + 10)).toEqual({
      index: 10,
      row: 2,
      slot: 2,
    });
    expect(at(PITCH_X + 100, 2 * PITCH_Y + 10)).toEqual({
      index: 10,
      row: 2,
      slot: 2,
    });
  });

  it("takes a pointer below the grid for the last row and one above or left of it for the first place", () => {
    expect(at(0, 99_999)).toEqual({ index: 8, row: 2, slot: 0 });
    expect(at(-50, -50)).toEqual({ index: 0, row: 0, slot: 0 });
  });

  it("takes the gap between rows as the row above", () => {
    expect(at(0, ORGANIZE_CELL_HEIGHT + 3).row).toBe(0);
    expect(at(0, PITCH_Y - 1).row).toBe(0);
    expect(at(0, PITCH_Y).row).toBe(1);
  });

  it("is the start for a grid with no pages", () => {
    expect(insertPointAt(50, 50, 4, 0)).toEqual({ index: 0, row: 0, slot: 0 });
  });
});

describe("the box of a page", () => {
  const a4 = { widthPt: 595, heightPt: 842 };
  const landscape = { widthPt: 842, heightPt: 595 };

  it("fits the longer side to the page box", () => {
    expect(thumbnailBox(a4, 0)).toEqual({
      width: 84,
      height: ORGANIZE_PAGE_MAX_SIDE,
      pictureWidth: 84,
      pictureHeight: ORGANIZE_PAGE_MAX_SIDE,
    });
    expect(thumbnailBox(landscape, 0).width).toBe(ORGANIZE_PAGE_MAX_SIDE);
    expect(thumbnailBox({ widthPt: 100, heightPt: 100 }, 0)).toMatchObject({
      width: ORGANIZE_PAGE_MAX_SIDE,
      height: ORGANIZE_PAGE_MAX_SIDE,
    });
  });

  it("swaps the sides of the box for a quarter turn and keeps the picture's", () => {
    for (const rotation of [90, 270]) {
      expect(thumbnailBox(a4, rotation)).toEqual({
        width: ORGANIZE_PAGE_MAX_SIDE,
        height: 84,
        pictureWidth: 84,
        pictureHeight: ORGANIZE_PAGE_MAX_SIDE,
      });
    }
    for (const rotation of [0, 180]) {
      expect(thumbnailBox(a4, rotation).width).toBe(84);
    }
  });

  it("is the same before the picture comes as after, since it is made from the page's size", () => {
    expect(thumbnailBox(landscape, 90)).toEqual(thumbnailBox(landscape, 90));
    expect(thumbnailBox(landscape, 90).width).toBe(84);
  });

  it("is at least a pixel for a page of any shape", () => {
    expect(thumbnailBox({ widthPt: 1, heightPt: 2000 }, 0).width).toBe(1);
  });
});

describe("the focus keys", () => {
  it("moves by one, by a row, and to the ends, within the pages", () => {
    expect(focusIndexAfter("ArrowRight", 3, 4, 10)).toBe(4);
    expect(focusIndexAfter("ArrowLeft", 3, 4, 10)).toBe(2);
    expect(focusIndexAfter("ArrowDown", 3, 4, 10)).toBe(7);
    expect(focusIndexAfter("ArrowUp", 7, 4, 10)).toBe(3);
    expect(focusIndexAfter("Home", 7, 4, 10)).toBe(0);
    expect(focusIndexAfter("End", 2, 4, 10)).toBe(9);
  });

  it("stays on the edge", () => {
    expect(focusIndexAfter("ArrowLeft", 0, 4, 10)).toBe(0);
    expect(focusIndexAfter("ArrowRight", 9, 4, 10)).toBe(9);
    expect(focusIndexAfter("ArrowUp", 2, 4, 10)).toBe(2);
    expect(focusIndexAfter("ArrowDown", 8, 4, 10)).toBe(8);
  });
});

describe("where Alt and an arrow put pages", () => {
  function pages(count: number): OrganizePage[] {
    return Array.from({ length: count }, (_, index) => ({
      id: 1,
      page: index + 1,
      rotation: 0,
    }));
  }
  const keys = (...numbers: number[]) =>
    new Set(numbers.map((page) => pageKey(1, page)));

  it("left and up put the block in front of the page before its first, by one or a row", () => {
    // Pages 4 and 5 are positions 3 and 4.
    expect(keyInsertIndex(pages(10), keys(4, 5), "before", 1)).toBe(2);
    expect(keyInsertIndex(pages(10), keys(4, 5), "before", 4)).toBe(0);
  });

  it("right and down put the block behind the page after its last", () => {
    expect(keyInsertIndex(pages(10), keys(4, 5), "after", 1)).toBe(6);
    expect(keyInsertIndex(pages(10), keys(4, 5), "after", 4)).toBe(9);
  });

  it("stops at the edges", () => {
    expect(keyInsertIndex(pages(10), keys(1), "before", 1)).toBe(0);
    expect(keyInsertIndex(pages(10), keys(2), "before", 5)).toBe(0);
    expect(keyInsertIndex(pages(10), keys(10), "after", 1)).toBe(10);
    expect(keyInsertIndex(pages(10), keys(9), "after", 5)).toBe(10);
  });

  it("goes by the first and the last of pages that are apart", () => {
    expect(keyInsertIndex(pages(10), keys(3, 8), "before", 1)).toBe(1);
    expect(keyInsertIndex(pages(10), keys(3, 8), "after", 1)).toBe(9);
  });

  it("is the start when none of the keys is a page", () => {
    expect(keyInsertIndex(pages(3), keys(9), "after", 1)).toBe(0);
  });
});

describe("scrolling to a row", () => {
  it("stays where it is when the row is whole in view", () => {
    expect(scrollTopToShowRow(2, 2 * PITCH_Y, 600)).toBe(2 * PITCH_Y);
    expect(scrollTopToShowRow(3, 2 * PITCH_Y, 600)).toBe(2 * PITCH_Y);
  });

  it("scrolls up to a row above and down to a row below", () => {
    expect(scrollTopToShowRow(1, 5 * PITCH_Y, 600)).toBe(PITCH_Y);
    expect(scrollTopToShowRow(10, 0, 600)).toBe(
      10 * PITCH_Y + ORGANIZE_CELL_HEIGHT - 600,
    );
  });

  it("does not go above the top", () => {
    expect(scrollTopToShowRow(0, 0, 600)).toBe(0);
    expect(scrollTopToShowRow(0, 50, 600)).toBe(0);
    // A room lower than a row shows the row's top.
    expect(scrollTopToShowRow(0, 0, 10)).toBe(0);
    expect(scrollTopToShowRow(2, 0, 10)).toBe(2 * PITCH_Y);
  });
});

describe("scrolling while dragging", () => {
  const top = 100;
  const bottom = 700;

  it("does nothing away from the edges", () => {
    expect(
      dragScrollStep(top + ORGANIZE_DRAG_SCROLL_EDGE_PX, top, bottom),
    ).toBe(0);
    expect(dragScrollStep(400, top, bottom)).toBe(0);
    expect(
      dragScrollStep(bottom - ORGANIZE_DRAG_SCROLL_EDGE_PX, top, bottom),
    ).toBe(0);
  });

  it("scrolls up near the top and down near the bottom, faster the nearer", () => {
    const slow = dragScrollStep(
      top + ORGANIZE_DRAG_SCROLL_EDGE_PX - 1,
      top,
      bottom,
    );
    const fast = dragScrollStep(top + 1, top, bottom);
    expect(slow).toBeLessThan(0);
    expect(fast).toBeLessThan(slow);
    expect(dragScrollStep(bottom - 1, top, bottom)).toBeGreaterThan(
      dragScrollStep(bottom - ORGANIZE_DRAG_SCROLL_EDGE_PX + 1, top, bottom),
    );
    expect(dragScrollStep(bottom - 1, top, bottom)).toBeGreaterThan(0);
  });

  it("is at its fastest at the edge and beyond it", () => {
    expect(dragScrollStep(top, top, bottom)).toBe(
      -ORGANIZE_DRAG_SCROLL_MAX_STEP_PX,
    );
    expect(dragScrollStep(top - 300, top, bottom)).toBe(
      -ORGANIZE_DRAG_SCROLL_MAX_STEP_PX,
    );
    expect(dragScrollStep(bottom + 300, top, bottom)).toBe(
      ORGANIZE_DRAG_SCROLL_MAX_STEP_PX,
    );
  });
});

/** The width of the line, from its `left` less the lead: what `.organize-drop-line` is. */
const LINE_SPAN = { before: 1, after: 2 };
/** What the scrolling area shows beyond the grid on each side. */
const PADDING = 4;

describe("dropLineLeft", () => {
  it("draws the line in the middle of the gap between two cells", () => {
    expect(dropLineLeft(2, 600)).toBe(
      2 * (ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP) - ORGANIZE_COLUMN_GAP / 2,
    );
  });

  it("keeps the line before the first cell inside the scrolling area's padding", () => {
    expect(dropLineLeft(0, 600)).toBe(-3);
  });

  it("keeps the line after the last cell of a row inside the padding when the row fills the width", () => {
    // Four cells and their gaps are 544 px: the middle of the gap after them is 552, 8 px out.
    const left = dropLineLeft(4, 544);

    expect(left + LINE_SPAN.after).toBe(544 + PADDING);
    expect(left).toBeLessThan(4 * PITCH_X - ORGANIZE_COLUMN_GAP / 2);
  });

  it("leaves the line after the last cell where it is when the row leaves room", () => {
    expect(dropLineLeft(4, 600)).toBe(4 * PITCH_X - ORGANIZE_COLUMN_GAP / 2);
    // The line is whole in view up to the last pixel it fits.
    expect(dropLineLeft(4, 556) + LINE_SPAN.after).toBeLessThanOrEqual(
      556 + PADDING,
    );
    expect(dropLineLeft(4, 556)).toBe(4 * PITCH_X - ORGANIZE_COLUMN_GAP / 2);
  });
});

describe("breakGap", () => {
  it("is the gap before the page in its own row", () => {
    expect(breakGap(1, 5)).toEqual({ row: 0, slot: 1 });
    expect(breakGap(4, 5)).toEqual({ row: 0, slot: 4 });
    expect(breakGap(7, 5)).toEqual({ row: 1, slot: 2 });
  });

  it("is the end of the row before for a page that starts a row", () => {
    expect(breakGap(5, 5)).toEqual({ row: 0, slot: 5 });
    expect(breakGap(10, 5)).toEqual({ row: 1, slot: 5 });
    expect(breakGap(3, 1)).toEqual({ row: 2, slot: 1 });
  });
});

describe("breakButtonBox", () => {
  it("is the gap itself", () => {
    expect(breakButtonBox(2, 600)).toEqual({
      left: 2 * PITCH_X - ORGANIZE_COLUMN_GAP,
      width: ORGANIZE_COLUMN_GAP,
    });
  });

  it("stops where the scrolling area stops showing the gap after the last cell of a full row", () => {
    // The row of four ends at 544; the area shows 4 px more.
    expect(breakButtonBox(4, 544)).toEqual({ left: 544, width: PADDING });
    expect(breakButtonBox(4, 556)).toEqual({
      left: 544,
      width: ORGANIZE_COLUMN_GAP,
    });
    expect(breakButtonBox(4, 550)).toEqual({ left: 544, width: 10 });
  });

  it("is never negative", () => {
    expect(breakButtonBox(4, 0).width).toBe(0);
  });
});

describe("breakMarkLeft", () => {
  it("centres the mark on the line", () => {
    // The mark is 20 wide and the line 3, 9 px before the mark.
    expect(breakMarkLeft(2, 600)).toBe(dropLineLeft(2, 600) - 1 - 9);
  });

  it("keeps the mark in view at the end of a full row", () => {
    expect(breakMarkLeft(4, 544) + 20).toBe(544 + PADDING);
  });
});

describe("scrollTopToCenterRow", () => {
  it("puts the row in the middle of the room", () => {
    const row = 10;
    const top = row * (ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP);
    expect(scrollTopToCenterRow(row, 600)).toBe(
      top - (600 - ORGANIZE_CELL_HEIGHT) / 2,
    );
  });

  it("stops at the top", () => {
    expect(scrollTopToCenterRow(0, 600)).toBe(0);
  });
});
