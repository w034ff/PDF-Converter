import { clearMocks } from "@tauri-apps/api/mocks";
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ORGANIZE_CELL_HEIGHT,
  ORGANIZE_OVERSCAN_ROWS,
  ORGANIZE_PAGE_MAX_SIDE,
  ORGANIZE_ROW_GAP,
  ORGANIZE_THUMBNAIL_DELAY_MS,
  scrollTopToCenterRow,
} from "./organizeGrid";
import {
  LANDSCAPE,
  callsOf,
  cellOf,
  drawnCells,
  drawnKeys,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openOrganizeTab,
  organizePdf,
  pressCell,
  resizeRoom,
  scrollArea,
  scrollGridTo,
  releasePointer,
} from "./organizeTestSupport";

const ROOM_WIDTH = 700; // five columns
const ROOM_HEIGHT = 600;
const COLUMNS = 5;
const ROW_PITCH = ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP;
/** Rows in view in the room, counting the one cut off at the bottom. */
const ROWS_IN_VIEW = Math.floor(ROOM_HEIGHT / ROW_PITCH) + 1;

beforeEach(() => {
  installFakeResizeObserver(ROOM_WIDTH, ROOM_HEIGHT);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const BIG = organizePdf(1, "big.pdf", 10_000);

describe("the grid of ten thousand pages", () => {
  it("draws only the rows in view and a few more", async () => {
    await openOrganizeTab({ pdfs: [BIG] });

    expect(
      screen.getByText("10000 ページ", { selector: ".organize-band-count" }),
    ).toBeInTheDocument();
    expect(drawnCells()).toHaveLength(
      COLUMNS * (ROWS_IN_VIEW + ORGANIZE_OVERSCAN_ROWS),
    );
    expect(drawnKeys().slice(0, 3)).toEqual(["1:1", "1:2", "1:3"]);
  });

  it("is as high as all its rows, so the scroll bar is the size of the whole grid", async () => {
    await openOrganizeTab({ pdfs: [BIG] });

    const rows = 10_000 / COLUMNS;
    expect(screen.getByRole("listbox")).toHaveStyle({
      height: `${rows * ORGANIZE_CELL_HEIGHT + (rows - 1) * ORGANIZE_ROW_GAP}px`,
    });
  });

  it("shows the page last worked on when its tab comes back", async () => {
    await openOrganizeTab({ pdfs: [BIG] });
    scrollGridTo(50 * ROW_PITCH);
    await flush();
    const page = 50 * COLUMNS + 1;
    pressCell(cellOf(1, page));
    releasePointer();
    await flush();

    fireEvent.click(screen.getByRole("tab", { name: "切り出し" }));
    await flush();
    const tab = screen.getByRole("tab", { name: "整理" });
    tab.focus();
    fireEvent.click(tab);
    await flush();

    expect(scrollArea().scrollTop).toBe(scrollTopToCenterRow(50, ROOM_HEIGHT));
    expect(drawnKeys()).toContain(`1:${page}`);
    expect(document.activeElement).toBe(tab);
  });

  it("draws other pages when it is scrolled, as many as in the middle of any grid, and the one Tab enters", async () => {
    await openOrganizeTab({ pdfs: [BIG] });

    scrollGridTo(1000 * ROW_PITCH);
    await flush();

    const keys = drawnKeys();
    expect(keys).not.toContain("1:2");
    expect(keys).toContain(`1:${1000 * COLUMNS + 1}`);
    expect(keys).toContain(`1:${(1000 + ROWS_IN_VIEW) * COLUMNS}`);
    // The first page is the tab stop until the focus is somewhere else.
    expect(keys).toContain("1:1");
    // The rows in view, the overscan above and below them, and the tab stop.
    expect(keys).toHaveLength(
      COLUMNS * (ROWS_IN_VIEW + 2 * ORGANIZE_OVERSCAN_ROWS) + 1,
    );
  });

  it("keeps drawing the focused page after it has left the view", async () => {
    await openOrganizeTab({ pdfs: [BIG] });
    fireEvent.focus(cellOf(1, 7));
    expect(cellOf(1, 7)).toHaveAttribute("tabindex", "0");

    scrollGridTo(3000 * ROW_PITCH);
    await flush();

    expect(drawnKeys()).toContain("1:7");
    expect(cellOf(1, 7)).toHaveAttribute("tabindex", "0");
    // Only that cell is a tab stop.
    expect(
      drawnCells().filter((cell) => cell.getAttribute("tabindex") === "0"),
    ).toHaveLength(1);
  });

  it("numbers every page for a screen reader, whichever are drawn", async () => {
    await openOrganizeTab({ pdfs: [BIG] });
    scrollGridTo(500 * ROW_PITCH);
    await flush();

    const cell = cellOf(1, 500 * COLUMNS + 3);

    expect(cell).toHaveAttribute("aria-setsize", "10000");
    expect(cell).toHaveAttribute("aria-posinset", String(500 * COLUMNS + 3));
    expect(cell).toHaveAccessibleName("big.pdf 2503 ページ");
  });

  it("changes the number of columns with the width", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "a.pdf", 40)] });
    const first = drawnCells().length;

    resizeRoom(400, ROOM_HEIGHT);
    await flush();

    // 400 px holds two columns.
    expect(drawnCells().length).toBe(
      2 * (ROWS_IN_VIEW + ORGANIZE_OVERSCAN_ROWS),
    );
    expect(first).toBe(COLUMNS * (ROWS_IN_VIEW + ORGANIZE_OVERSCAN_ROWS));
  });

  it("draws as many cells for a hundred pages as for ten thousand where there are enough", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "a.pdf", 100)] });

    expect(drawnCells()).toHaveLength(
      COLUMNS * (ROWS_IN_VIEW + ORGANIZE_OVERSCAN_ROWS),
    );
  });
});

describe("a cell", () => {
  it("has the size of its page's box before any picture comes", async () => {
    await openOrganizeTab({
      pdfs: [
        organizePdf(1, "mixed.pdf", 2, {
          pages: [{ widthPt: 595, heightPt: 842 }, LANDSCAPE],
        }),
      ],
    });

    const [portrait, landscape] = ["1:1", "1:2"].map((key) =>
      cellOf(1, Number(key.split(":")[1])).querySelector<HTMLElement>(
        ".organize-thumb",
      ),
    );
    expect(portrait).toHaveStyle({
      width: "84px",
      height: `${ORGANIZE_PAGE_MAX_SIDE}px`,
    });
    expect(landscape).toHaveStyle({
      width: `${ORGANIZE_PAGE_MAX_SIDE}px`,
      height: "84px",
    });
    expect(cellOf(1, 1).querySelector("img")).toBeNull();
  });

  it("names the PDF by a color and its name without the extension, and the page by its number in the PDF", async () => {
    await openOrganizeTab({
      pdfs: [
        organizePdf(1, "annual-report.pdf", 2),
        organizePdf(2, "memo.final.pdf", 1),
      ],
    });

    const caption = (cell: HTMLElement) =>
      cell.querySelector(".organize-cell-caption")?.textContent;
    expect(caption(cellOf(1, 2))).toBe("p.2annual-report");
    expect(caption(cellOf(2, 1))).toBe("p.1memo.final");
    const dot = (cell: HTMLElement) =>
      cell.querySelector<HTMLElement>(".organize-dot")?.style.background;
    expect(dot(cellOf(1, 1))).toBe("var(--organize-source-1)");
    expect(dot(cellOf(2, 1))).toBe("var(--organize-source-2)");
  });

  it("turns the picture, swaps the box and shows the angle when the page is turned", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "a.pdf", 2)] });
    await flush();
    pressCell(cellOf(1, 2));
    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    await flush();

    const cell = cellOf(1, 2);
    expect(cell.querySelector(".organize-thumb")).toHaveStyle({
      width: `${ORGANIZE_PAGE_MAX_SIDE}px`,
      height: "84px",
    });
    expect(cell).toHaveTextContent("90°");
    expect(cell).toHaveAccessibleName("a.pdf 2 ページ、90°");
    expect(cellOf(1, 1)).not.toHaveTextContent("°");
    expect(cellOf(1, 1)).toHaveAccessibleName("a.pdf 1 ページ");
  });

  it("does not ask for a new picture when a page is turned, but turns the one it has", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const calls = await openOrganizeTab({ pdfs: [organizePdf(1, "a.pdf", 2)] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ORGANIZE_THUMBNAIL_DELAY_MS);
    });
    const asked = callsOf(calls, "get_thumbnail").length;
    expect(asked).toBe(2);

    pressCell(cellOf(1, 2));
    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ORGANIZE_THUMBNAIL_DELAY_MS * 2);
    });

    expect(callsOf(calls, "get_thumbnail")).toHaveLength(asked);
    expect(cellOf(1, 2).querySelector("img")).toHaveStyle({
      transform: "translate(-50%, -50%) rotate(90deg)",
    });
  });
});

describe("the pictures of the cells", () => {
  it("are asked for only after a delay", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const calls = await openOrganizeTab({ pdfs: [BIG] });

    expect(callsOf(calls, "get_thumbnail")).toHaveLength(0);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ORGANIZE_THUMBNAIL_DELAY_MS - 1);
    });
    expect(callsOf(calls, "get_thumbnail")).toHaveLength(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });

    expect(callsOf(calls, "get_thumbnail")).toHaveLength(drawnCells().length);
  });

  it("are not asked for by a cell that is gone before the delay", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const calls = await openOrganizeTab({ pdfs: [BIG] });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ORGANIZE_THUMBNAIL_DELAY_MS);
    });
    const before = callsOf(calls, "get_thumbnail").length;

    // A fast scroll through the middle on to the end.
    scrollGridTo(900 * ROW_PITCH);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ORGANIZE_THUMBNAIL_DELAY_MS / 2);
    });
    scrollGridTo(1800 * ROW_PITCH);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ORGANIZE_THUMBNAIL_DELAY_MS * 2);
    });

    const asked = callsOf(calls, "get_thumbnail")
      .slice(before)
      .map((call) => Number(call.args.page));
    expect(asked.length).toBeGreaterThan(0);
    expect(asked.every((page) => page > 1800 * COLUMNS - 3 * COLUMNS)).toBe(
      true,
    );
    expect(
      asked.some((page) => page > 900 * COLUMNS && page < 1000 * COLUMNS),
    ).toBe(false);
  });
});
