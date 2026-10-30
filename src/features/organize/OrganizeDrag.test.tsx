import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DRAG_THRESHOLD_PX } from "../shared/pointer";
import {
  ORGANIZE_CELL_WIDTH,
  ORGANIZE_COLUMN_GAP,
  ORGANIZE_DRAG_SCROLL_EDGE_PX,
  ORGANIZE_PAGE_BOX_SIDE,
  cellOffset,
  insertPointAt,
} from "./organizeGrid";
import {
  drawnKeys,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  movePointer,
  openOrganizeTab,
  organizePdf,
  pressCell,
  releasePointer,
  cellOf,
  scrollArea,
  selectedKeys,
} from "./organizeTestSupport";

const COLUMNS = 5;

beforeEach(() => {
  installFakeResizeObserver(700, 600);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

const BOOK = organizePdf(1, "book.pdf", 10);

/** The point on the left half of the cell at `index`, where a drag puts pages in front of it. */
function inFrontOf(index: number): [number, number] {
  const { left, top } = cellOffset(index, COLUMNS);
  return [left + 10, top + 20];
}

/** Where the pointer is when a press starts on a cell of the first row. */
const START: [number, number] = [40, 40];

function press(cell: HTMLElement, at: [number, number] = START) {
  pressCell(cell, { clientX: at[0], clientY: at[1] });
}

function dropLine(): HTMLElement | null {
  return document.querySelector(".organize-drop-line");
}

function order(): string[] {
  return drawnKeys();
}

function ctrlClick(cell: HTMLElement) {
  pressCell(cell, { ctrlKey: true });
  releasePointer();
}

describe("dragging a page", () => {
  it("is a click until the pointer has moved the threshold", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    press(cellOf(1, 2));

    movePointer(START[0] + DRAG_THRESHOLD_PX - 1, START[1]);
    expect(dropLine()).toBeNull();
    releasePointer();

    expect(order()).toEqual(drawnKeys());
    expect(order().slice(0, 3)).toEqual(["1:1", "1:2", "1:3"]);
    expect(selectedKeys()).toEqual(["1:2"]);
  });

  it("shows a line where the page would go once it has moved, and nothing changes until the release", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    press(cellOf(1, 2));

    const [x, y] = inFrontOf(4);
    movePointer(x, y);

    const line = dropLine();
    expect(line).not.toBeNull();
    const slotLeft =
      4 * (ORGANIZE_CELL_WIDTH + ORGANIZE_COLUMN_GAP) - ORGANIZE_COLUMN_GAP / 2;
    expect(line).toHaveStyle({
      left: `${slotLeft}px`,
      top: "0px",
      height: `${ORGANIZE_PAGE_BOX_SIDE}px`,
    });
    expect(order().slice(0, 5)).toEqual(["1:1", "1:2", "1:3", "1:4", "1:5"]);

    releasePointer();
  });

  it("moves one page to where the line was, on the release", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    press(cellOf(1, 2));

    movePointer(...inFrontOf(4));
    releasePointer();
    await flush();

    expect(order()).toEqual([
      "1:1",
      "1:3",
      "1:4",
      "1:2",
      "1:5",
      "1:6",
      "1:7",
      "1:8",
      "1:9",
      "1:10",
    ]);
    expect(dropLine()).toBeNull();
    expect(selectedKeys()).toEqual(["1:2"]);
  });

  it("moves the selected pages together, in their order", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    ctrlClick(cellOf(1, 2));
    ctrlClick(cellOf(1, 4));
    ctrlClick(cellOf(1, 9));

    press(cellOf(1, 4));
    movePointer(...inFrontOf(0));
    releasePointer();
    await flush();

    expect(order()).toEqual([
      "1:2",
      "1:4",
      "1:9",
      "1:1",
      "1:3",
      "1:5",
      "1:6",
      "1:7",
      "1:8",
      "1:10",
    ]);
    expect(selectedKeys().sort()).toEqual(["1:2", "1:4", "1:9"]);
  });

  it("drags only the page pressed when it was not one of the selected", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    ctrlClick(cellOf(1, 2));
    ctrlClick(cellOf(1, 3));

    press(cellOf(1, 7));
    movePointer(...inFrontOf(0));
    releasePointer();
    await flush();

    expect(order()).toEqual([
      "1:7",
      "1:1",
      "1:2",
      "1:3",
      "1:4",
      "1:5",
      "1:6",
      "1:8",
      "1:9",
      "1:10",
    ]);
    expect(selectedKeys()).toEqual(["1:7"]);
  });

  it("puts pages after the last one when the pointer is past the end", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    press(cellOf(1, 1));

    // Below the last row and to its right.
    movePointer(690, 900);
    releasePointer();
    await flush();

    expect(order().at(-1)).toBe("1:1");
    expect(order().slice(0, 3)).toEqual(["1:2", "1:3", "1:4"]);
  });

  it("changes the grid once, so one undo puts it back", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    press(cellOf(1, 2));
    movePointer(...inFrontOf(4));
    movePointer(...inFrontOf(7));
    movePointer(...inFrontOf(5));
    releasePointer();
    await flush();
    expect(order().indexOf("1:2")).toBe(4);

    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();

    expect(order()).toEqual([
      "1:1",
      "1:2",
      "1:3",
      "1:4",
      "1:5",
      "1:6",
      "1:7",
      "1:8",
      "1:9",
      "1:10",
    ]);
  });

  it("does nothing when the pages are put where they were", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    press(cellOf(1, 2));

    movePointer(...inFrontOf(1));
    releasePointer();
    await flush();
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();

    expect(order()).toEqual([
      "1:1",
      "1:2",
      "1:3",
      "1:4",
      "1:5",
      "1:6",
      "1:7",
      "1:8",
      "1:9",
      "1:10",
    ]);
  });
});

describe("putting a drag down", () => {
  async function startDrag() {
    await openOrganizeTab({ pdfs: [BOOK] });
    ctrlClick(cellOf(1, 2));
    ctrlClick(cellOf(1, 3));
    press(cellOf(1, 2));
    movePointer(...inFrontOf(6));
    expect(dropLine()).not.toBeNull();
  }

  function expectUntouched() {
    expect(dropLine()).toBeNull();
    expect(order().slice(0, 4)).toEqual(["1:1", "1:2", "1:3", "1:4"]);
  }

  it("Esc puts it down, keeps the selection, and goes no further", async () => {
    await startDrag();

    fireEvent.keyDown(window, { key: "Escape" });
    releasePointer();
    await flush();

    expectUntouched();
    expect(selectedKeys().sort()).toEqual(["1:2", "1:3"]);
  });

  it("pointercancel puts it down", async () => {
    await startDrag();

    fireEvent.pointerCancel(window, { pointerId: 1 });
    releasePointer();
    await flush();

    expectUntouched();
    expect(selectedKeys().sort()).toEqual(["1:2", "1:3"]);
  });

  it("a window that loses the focus puts it down", async () => {
    await startDrag();

    fireEvent.blur(window);
    releasePointer();
    await flush();

    expectUntouched();
  });

  it("leaves the selection of a press that was not a drag as it was after Esc", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    ctrlClick(cellOf(1, 2));
    ctrlClick(cellOf(1, 3));
    press(cellOf(1, 2));

    fireEvent.keyDown(window, { key: "Escape" });
    releasePointer();

    expect(selectedKeys().sort()).toEqual(["1:2", "1:3"]);
  });

  it("lets no key work on the page while it is dragged", async () => {
    await startDrag();
    const before = order();

    fireEvent.keyDown(document.body, { key: "Delete" });
    fireEvent.keyDown(document.body, { key: "?" });
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(order()).toEqual(before);
    releasePointer();
  });
});

/** A box at (`left`, `top`), which jsdom would put at 0, 0. */
function rect(
  top: number,
  left: number,
  width: number,
  height: number,
): DOMRect {
  return {
    x: left,
    y: top,
    top,
    left,
    bottom: top + height,
    right: left + width,
    width,
    height,
    toJSON: () => ({}),
  };
}

describe("scrolling while dragging", () => {
  let frames: FrameRequestCallback[];

  beforeEach(() => {
    frames = [];
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frames.push(callback);
      return frames.length;
    });
    vi.stubGlobal("cancelAnimationFrame", () => {});
  });

  function runFrame() {
    const callback = frames.shift();
    act(() => callback?.(0));
  }

  async function dragNear(clientY: number, scrollTop: number) {
    await openOrganizeTab({ pdfs: [organizePdf(1, "big.pdf", 400)] });
    const area = scrollArea();
    area.scrollTop = scrollTop;
    area.getBoundingClientRect = () => rect(100, 0, 700, 600);
    press(cellOf(1, 2));
    movePointer(300, clientY);
    return area;
  }

  it("scrolls down while the pointer is near the bottom edge, a step each frame", async () => {
    const area = await dragNear(700 - 5, 2000);

    runFrame();
    const once = area.scrollTop;
    runFrame();

    expect(once).toBeGreaterThan(2000);
    expect(area.scrollTop).toBeGreaterThan(once);
    releasePointer();
  });

  it("scrolls up near the top edge", async () => {
    const area = await dragNear(100 + 5, 2000);

    runFrame();

    expect(area.scrollTop).toBeLessThan(2000);
    releasePointer();
  });

  it("does not scroll away from the edges", async () => {
    const area = await dragNear(100 + ORGANIZE_DRAG_SCROLL_EDGE_PX + 20, 2000);

    runFrame();
    runFrame();

    expect(area.scrollTop).toBe(2000);
    releasePointer();
  });

  it("stops scrolling when the drag ends", async () => {
    await dragNear(700 - 5, 2000);
    const cancel = vi.fn();
    vi.stubGlobal("cancelAnimationFrame", cancel);

    releasePointer();

    expect(cancel).toHaveBeenCalled();
  });

  it("carries the page to a place the grid has scrolled to", async () => {
    await dragNear(700 - 5, 2000);
    const area = scrollArea();
    // The grid moves up under the pointer as the area scrolls.
    screen.getByRole("listbox").getBoundingClientRect = () =>
      rect(100 - area.scrollTop, 0, 700, 100_000);
    for (let frame = 0; frame < 5; frame += 1) {
      runFrame();
    }
    const scrolled = area.scrollTop;
    expect(scrolled).toBeGreaterThan(2000);

    releasePointer();
    await flush();

    // The page left its place in front, so it lands one before the position picked.
    const picked = insertPointAt(300, 700 - 5 - 100 + scrolled, COLUMNS, 400);
    expect(picked.index).toBeGreaterThan(5 * 12);
    expect(cellOf(1, 2)).toHaveAttribute("aria-posinset", String(picked.index));
  });
});

describe("while a conversion runs", () => {
  it("does not start a drag", async () => {
    await openOrganizeTab({
      pdfs: [BOOK],
      handlers: { save_organized_pdf: () => new Promise(() => {}) },
    });
    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    await flush();

    press(cellOf(1, 2));
    movePointer(...inFrontOf(6));
    releasePointer();
    await flush();

    expect(dropLine()).toBeNull();
    expect(order().slice(0, 3)).toEqual(["1:1", "1:2", "1:3"]);
    // Pressing still selects.
    expect(selectedKeys()).toEqual(["1:2"]);
  });
});
