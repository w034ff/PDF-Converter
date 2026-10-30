import { clearMocks } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ORGANIZE_CELL_HEIGHT,
  ORGANIZE_ROW_GAP,
  scrollTopToShowRow,
} from "./organizeGrid";
import {
  cellOf,
  drawnCells,
  drawnKeys,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openOrganizeTab,
  organizePdf,
  pressCell,
  releasePointer,
  scrollArea,
  selectedKeys,
} from "./organizeTestSupport";

const COLUMNS = 5;
const ROOM_HEIGHT = 600;
const ROW_PITCH = ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP;

beforeEach(() => {
  installFakeResizeObserver(700, ROOM_HEIGHT);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

const BOOK = organizePdf(1, "book.pdf", 12);

function focusCell(id: number, page: number) {
  act(() => cellOf(id, page).focus());
}

function focusedKey(): string | undefined {
  return document.activeElement instanceof HTMLElement
    ? document.activeElement.dataset.pageKey
    : undefined;
}

/** Presses a key on whatever has the focus. */
function key(name: string, init: KeyboardEventInit = {}) {
  fireEvent.keyDown(document.activeElement ?? document.body, {
    key: name,
    ...init,
  });
}

function ctrlClick(cell: HTMLElement) {
  pressCell(cell, { ctrlKey: true });
  releasePointer();
}

function rotationsOf(...pages: number[]): (string | null | undefined)[] {
  return pages.map(
    (page) =>
      cellOf(1, page).querySelector(".organize-cell-angle")?.textContent,
  );
}

describe("moving the focus in the grid", () => {
  it("moves with the arrows by one page and by a row, without changing the selection", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 1);
    ctrlClick(cellOf(1, 1));
    focusCell(1, 1);

    key("ArrowRight");
    expect(focusedKey()).toBe("1:2");
    key("ArrowDown");
    expect(focusedKey()).toBe(`1:${2 + COLUMNS}`);
    key("ArrowLeft");
    expect(focusedKey()).toBe(`1:${1 + COLUMNS}`);
    key("ArrowUp");
    expect(focusedKey()).toBe("1:1");
    expect(selectedKeys()).toEqual(["1:1"]);
  });

  it("goes to the first and the last page with Home and End", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 6);

    key("End");
    expect(focusedKey()).toBe("1:12");
    key("Home");
    expect(focusedKey()).toBe("1:1");
  });

  it("stays on the edge", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 1);

    key("ArrowLeft");
    key("ArrowUp");
    expect(focusedKey()).toBe("1:1");

    key("End");
    key("ArrowRight");
    key("ArrowDown");
    expect(focusedKey()).toBe("1:12");
  });

  it("does not take the arrow keys with Ctrl", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 3);

    key("ArrowRight", { ctrlKey: true });

    expect(focusedKey()).toBe("1:3");
  });

  it("selects from the anchor to the focus with Shift", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    pressCell(cellOf(1, 3));
    releasePointer();
    focusCell(1, 3);

    key("ArrowRight", { shiftKey: true });
    key("ArrowRight", { shiftKey: true });
    expect(selectedKeys().sort()).toEqual(["1:3", "1:4", "1:5"]);

    key("ArrowLeft", { shiftKey: true });
    expect(selectedKeys().sort()).toEqual(["1:3", "1:4"]);

    key("ArrowDown", { shiftKey: true });
    expect(selectedKeys().sort()).toEqual(
      [3, 4, 5, 6, 7, 8, 9].map((page) => `1:${page}`),
    );
  });

  it("does not leave a key at the edge to jump the grid on a later change", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "big.pdf", 10_000)] });
    focusCell(1, 1);
    key("ArrowLeft");
    await flush();

    scrollArea().scrollTop = 5_000;
    fireEvent.scroll(scrollArea());
    await flush();
    focusCell(1, 30 * COLUMNS + 1);
    await flush();

    expect(scrollArea().scrollTop).toBe(5_000);
  });

  it("shows the page it reaches, scrolling the grid and giving it the focus", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "big.pdf", 10_000)] });
    focusCell(1, 1);

    key("End");
    await flush();

    expect(focusedKey()).toBe("1:10000");
    const lastRow = 10_000 / COLUMNS - 1;
    expect(scrollArea().scrollTop).toBe(
      scrollTopToShowRow(lastRow, 0, ROOM_HEIGHT),
    );
    expect(drawnKeys()).toContain("1:10000");
  });

  it("scrolls just far enough when the arrow leaves the view", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "long.pdf", 100)] });
    // Row 2 is the last row whole in a room 600 px high; row 3 is cut off.
    expect(2 * ROW_PITCH + ORGANIZE_CELL_HEIGHT).toBeLessThanOrEqual(
      ROOM_HEIGHT,
    );
    expect(3 * ROW_PITCH + ORGANIZE_CELL_HEIGHT).toBeGreaterThan(ROOM_HEIGHT);
    focusCell(1, 11);

    key("ArrowDown");
    await flush();

    expect(focusedKey()).toBe("1:16");
    expect(scrollArea().scrollTop).toBe(
      3 * ROW_PITCH + ORGANIZE_CELL_HEIGHT - ROOM_HEIGHT,
    );
  });

  it("leaves the grid where it is when the page it reaches is whole in view", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "long.pdf", 100)] });
    focusCell(1, 6);

    key("ArrowDown");
    await flush();

    expect(focusedKey()).toBe("1:11");
    expect(scrollArea().scrollTop).toBe(0);
  });
});

describe("selecting with the keys", () => {
  it("Space puts the focused page in and out, and makes it the anchor", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 4);

    key(" ");
    key("ArrowRight");
    key(" ");
    expect(selectedKeys().sort()).toEqual(["1:4", "1:5"]);

    key(" ");
    expect(selectedKeys()).toEqual(["1:4"]);
  });

  it("Ctrl+A selects every page, and only while the grid has the focus", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);

    const notPrevented = fireEvent.keyDown(
      document.activeElement ?? document.body,
      {
        key: "a",
        ctrlKey: true,
      },
    );

    expect(notPrevented).toBe(false);
    expect(selectedKeys()).toHaveLength(12);

    // The band's button has the focus: the browser's own Ctrl+A is left alone.
    fireEvent.keyDown(document.body, { key: "Escape" });
    const free = fireEvent.keyDown(
      screen.getByRole("button", { name: "削除" }),
      {
        key: "a",
        ctrlKey: true,
      },
    );
    expect(free).toBe(true);
  });

  it("Esc deselects", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);
    key(" ");
    expect(selectedKeys()).toEqual(["1:2"]);

    key("Escape");

    expect(selectedKeys()).toEqual([]);
  });

  it("makes the first page the only stop of Tab, until the focus is elsewhere", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    expect(
      drawnCells()
        .filter((cell) => cell.getAttribute("tabindex") === "0")
        .map((cell) => cell.dataset.pageKey),
    ).toEqual(["1:1"]);

    focusCell(1, 5);

    expect(
      drawnCells()
        .filter((cell) => cell.getAttribute("tabindex") === "0")
        .map((cell) => cell.dataset.pageKey),
    ).toEqual(["1:5"]);
  });
});

describe("moving, turning and deleting with the keys", () => {
  it("Alt+arrow moves the selected pages as a block by one place, keeping the selection and the focus", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    ctrlClick(cellOf(1, 4));
    ctrlClick(cellOf(1, 5));
    focusCell(1, 5);

    key("ArrowLeft", { altKey: true });
    expect(drawnKeys().slice(0, 6)).toEqual([
      "1:1",
      "1:2",
      "1:4",
      "1:5",
      "1:3",
      "1:6",
    ]);
    expect(selectedKeys().sort()).toEqual(["1:4", "1:5"]);
    expect(focusedKey()).toBe("1:5");

    key("ArrowRight", { altKey: true });
    key("ArrowRight", { altKey: true });
    // Behind the page after the block, then behind the next one.
    expect(drawnKeys().slice(0, 7)).toEqual([
      "1:1",
      "1:2",
      "1:3",
      "1:6",
      "1:4",
      "1:5",
      "1:7",
    ]);
  });

  it("Alt+up and down move by a row", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);

    key("ArrowDown", { altKey: true });
    expect(drawnKeys()).toEqual([
      "1:1",
      "1:3",
      "1:4",
      "1:5",
      "1:6",
      "1:7",
      "1:2",
      "1:8",
      "1:9",
      "1:10",
      "1:11",
      "1:12",
    ]);

    key("ArrowUp", { altKey: true });
    expect(drawnKeys().slice(0, 3)).toEqual(["1:1", "1:2", "1:3"]);
  });

  it("moves the focused page when nothing is selected, and selects nothing", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 3);

    key("ArrowRight", { altKey: true });

    expect(drawnKeys().slice(0, 5)).toEqual([
      "1:1",
      "1:2",
      "1:4",
      "1:3",
      "1:5",
    ]);
    expect(selectedKeys()).toEqual([]);
    expect(focusedKey()).toBe("1:3");
  });

  it("stops at the edge and then makes no step", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);
    key("Delete");
    expect(drawnKeys()).toHaveLength(11);
    focusCell(1, 1);

    key("ArrowLeft", { altKey: true });
    key("ArrowUp", { altKey: true });
    expect(drawnKeys().slice(0, 3)).toEqual(["1:1", "1:3", "1:4"]);
    // Had they made steps, this would undo one of them and not the deletion.
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();

    expect(drawnKeys()).toHaveLength(12);
  });

  it("R turns right and L turns left, the selected pages or else the focused one", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);
    key("r");
    expect(rotationsOf(1, 2, 3)).toEqual([undefined, "90°", undefined]);
    key("L");
    key("l");
    expect(rotationsOf(2)).toEqual(["270°"]);

    ctrlClick(cellOf(1, 5));
    ctrlClick(cellOf(1, 6));
    focusCell(1, 5);
    key("R");
    expect(rotationsOf(2, 5, 6)).toEqual(["270°", "90°", "90°"]);
  });

  it("Delete and Backspace delete, and the focus goes to the next page", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);

    key("Delete");
    expect(drawnKeys().slice(0, 3)).toEqual(["1:1", "1:3", "1:4"]);
    expect(focusedKey()).toBe("1:3");

    ctrlClick(cellOf(1, 5));
    ctrlClick(cellOf(1, 6));
    key("Backspace");
    expect(drawnKeys()).toEqual([
      "1:1",
      "1:3",
      "1:4",
      "1:7",
      "1:8",
      "1:9",
      "1:10",
      "1:11",
      "1:12",
    ]);
    expect(selectedKeys()).toEqual([]);
    expect(focusedKey()).toBe("1:7");
  });

  it("puts the focus on the page before when the last page is deleted", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 12);

    key("Delete");
    await flush();

    expect(focusedKey()).toBe("1:11");
  });
});

describe("the keys of the whole tab", () => {
  it("Ctrl+Z undoes and Ctrl+Y or Ctrl+Shift+Z redoes, wherever the focus is", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);
    key("Delete");
    expect(drawnKeys()).toHaveLength(11);

    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    expect(drawnKeys()).toHaveLength(12);
    fireEvent.keyDown(document.body, { key: "y", ctrlKey: true });
    expect(drawnKeys()).toHaveLength(11);
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    fireEvent.keyDown(document.body, {
      key: "Z",
      ctrlKey: true,
      shiftKey: true,
    });
    expect(drawnKeys()).toHaveLength(11);
  });

  it("does not work in a field that takes text", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);
    key("Delete");
    const field = document.createElement("input");
    field.type = "text";
    document.body.append(field);

    fireEvent.keyDown(field, { key: "z", ctrlKey: true });
    fireEvent.keyDown(field, { key: "?" });

    expect(drawnKeys()).toHaveLength(11);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    field.remove();
  });

  it("does not work while a dialog is open", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    focusCell(1, 2);
    key("Delete");
    fireEvent.click(screen.getByRole("button", { name: "すべて外す" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    expect(drawnKeys()).toHaveLength(11);
  });

  it("? lists the keys in a dialog that Esc closes and gives the focus back from", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.keyDown(document.body, { key: "?" });

    const dialog = screen.getByRole("dialog", { name: "キー操作" });
    expect(within(dialog).getByText("Delete")).toBeInTheDocument();
    expect(within(dialog).getByText("Alt")).toBeInTheDocument();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await flush();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens the same list from the button of the band", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.click(screen.getByRole("button", { name: "キー操作の一覧" }));

    expect(
      screen.getByRole("dialog", { name: "キー操作" }),
    ).toBeInTheDocument();
  });

  it("does not undo while a conversion runs", async () => {
    await openOrganizeTab({
      pdfs: [BOOK],
      handlers: { save_organized_pdf: () => new Promise(() => {}) },
    });
    focusCell(1, 2);
    key("Delete");
    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    await flush();

    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    focusCell(1, 3);
    key("Delete");
    key("r");
    key("ArrowRight", { altKey: true });

    expect(drawnKeys()).toHaveLength(11);
    expect(rotationsOf(3)).toEqual([undefined]);
    expect(drawnKeys().slice(0, 4)).toEqual(["1:1", "1:3", "1:4", "1:5"]);
  });
});
