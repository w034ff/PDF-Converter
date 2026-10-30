import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DRAG_THRESHOLD_PX } from "../shared/pointer";
import {
  ORGANIZE_CELL_HEIGHT,
  ORGANIZE_OVERSCAN_ROWS,
  ORGANIZE_PAGE_BOX_SIDE,
  ORGANIZE_ROW_GAP,
  breakButtonBox,
  dropLineLeft,
} from "./organizeGrid";
import {
  SPLIT_SETTINGS,
  badges,
  breakButton,
  breakButtons,
  breakLines,
  cellOf,
  drawnCells,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  movePointer,
  openOrganizeTab,
  organizePdf,
  pressCell,
  releasePointer,
  scrollGridTo,
} from "./organizeTestSupport";

const ROOM_WIDTH = 700; // five columns
const ROOM_HEIGHT = 600;
const COLUMNS = 5;
const ROW_PITCH = ORGANIZE_CELL_HEIGHT + ORGANIZE_ROW_GAP;
const ROWS_IN_VIEW = Math.floor(ROOM_HEIGHT / ROW_PITCH) + 1;

beforeEach(() => {
  installFakeResizeObserver(ROOM_WIDTH, ROOM_HEIGHT);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

const BOOK = organizePdf(1, "book.pdf", 8);

function openSplit(pdfs = [BOOK], width = ROOM_WIDTH) {
  installFakeResizeObserver(width, ROOM_HEIGHT);
  return openOrganizeTab({ pdfs, settings: SPLIT_SETTINGS });
}

/** A button of the settings panel; "すべて外す" is on the toolbar of the PDFs as well. */
function settingsButton(name: string): HTMLElement {
  return within(screen.getByRole("complementary")).getByRole("button", {
    name,
  });
}

function key(cell: HTMLElement, key: string) {
  fireEvent.keyDown(cell, { key });
}

function click(cell: HTMLElement) {
  pressCell(cell);
  releasePointer();
}

function badgeOf(cell: HTMLElement): string | null {
  return cell.querySelector(".organize-part-badge")?.textContent ?? null;
}

describe("the choice of how to save", () => {
  it("shows the settings of one PDF, with no breaks, as it starts", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    const choice = screen.getByRole("group", { name: "保存の方法" });
    expect(
      within(choice).getByRole("button", { name: "1 つの PDF" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByText("並んだ順に、1 つの PDF にまとめます。"),
    ).toBeInTheDocument();
    expect(screen.queryByText("区切り")).not.toBeInTheDocument();
    expect(screen.queryByText("保存先フォルダ")).not.toBeInTheDocument();
    expect(breakButtons()).toEqual([]);
    expect(badges()).toEqual([]);
    expect(
      screen.getByRole("button", { name: "PDF を保存" }),
    ).toBeInTheDocument();
  });

  it("changes the settings and the button when saving in parts is chosen, and back", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.click(screen.getByRole("button", { name: "分けて保存" }));

    expect(
      screen.getByText("区切りごとに、別の PDF として保存します。"),
    ).toBeInTheDocument();
    expect(screen.getByText("区切り")).toBeInTheDocument();
    expect(
      screen.getByText(
        "ページの間を押すと、区切りを入れたり外したりできます。",
      ),
    ).toBeInTheDocument();
    for (const name of ["1 ページずつ", "元の PDF ごと"]) {
      expect(screen.getByRole("button", { name })).toBeEnabled();
    }
    expect(screen.getByText("保存先フォルダ")).toBeInTheDocument();
    expect(screen.getByTestId("output-dir-name")).toHaveTextContent("未選択");
    expect(
      screen.getByRole("button", { name: "保存を開始" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "PDF を保存" }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "1 つの PDF" }));
    expect(screen.queryByText("保存先フォルダ")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "PDF を保存" }),
    ).toBeInTheDocument();
  });

  it("takes the lines and badges off when one PDF is chosen, and brings them back with the breaks", async () => {
    await openSplit();
    fireEvent.click(breakButton(2, 3));
    expect(breakLines()).toHaveLength(1);
    expect(badges().map((badge) => badge.textContent)).toEqual(["1", "2"]);

    fireEvent.click(screen.getByRole("button", { name: "1 つの PDF" }));
    expect(breakLines()).toEqual([]);
    expect(badges()).toEqual([]);
    expect(breakButtons()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "分けて保存" }));
    expect(breakLines()).toHaveLength(1);
    expect(badges().map((badge) => badge.textContent)).toEqual(["1", "2"]);
    expect(breakButton(2, 3)).toHaveAttribute("aria-pressed", "true");
  });

  it("forgets no break while the setting is one PDF", async () => {
    await openSplit();
    fireEvent.click(settingsButton("1 ページずつ"));
    fireEvent.click(settingsButton("1 つの PDF"));
    fireEvent.click(settingsButton("分けて保存"));

    expect(badges()).toHaveLength(8);
  });
});

describe("the badges and the lines", () => {
  it("number the outputs on the first page of each, the first page included", async () => {
    await openSplit();
    fireEvent.click(breakButton(2, 3));
    fireEvent.click(breakButton(5, 6));

    expect(badgeOf(cellOf(1, 1))).toBe("1");
    expect(badgeOf(cellOf(1, 2))).toBeNull();
    expect(badgeOf(cellOf(1, 3))).toBe("2");
    expect(badgeOf(cellOf(1, 6))).toBe("3");
    expect(badgeOf(cellOf(1, 7))).toBeNull();
  });

  it("number the first page even with no break", async () => {
    await openSplit();

    expect(badgeOf(cellOf(1, 1))).toBe("1");
    expect(badges()).toHaveLength(1);
    expect(breakLines()).toEqual([]);
  });

  it("draw a line in the middle of the gap, in the row of the pages around it", async () => {
    await openSplit();
    fireEvent.click(breakButton(2, 3));

    const [line] = breakLines();
    expect(line).toHaveStyle({
      left: `${dropLineLeft(2, ROOM_WIDTH)}px`,
      top: "0px",
      height: `${ORGANIZE_PAGE_BOX_SIDE}px`,
    });
    expect(line.style.left).toBe("272px");
  });

  it("draw the line of a page that starts a row to the right of the row before it", async () => {
    await openSplit();
    // Page 6 is the first of the second row.
    fireEvent.click(breakButton(5, 6));

    const [line] = breakLines();
    expect(line).toHaveStyle({ left: "692px", top: "0px" });
    expect(badgeOf(cellOf(1, 6))).toBe("2");
    // The number is on the cell of the second row, whose top is one row lower.
    expect(cellOf(1, 6)).toHaveStyle({ top: `${ROW_PITCH}px` });
  });

  it("keep the line of a row's end whole when the row fills the width", async () => {
    // Four columns fit 544 px with nothing to spare: the gap after the last cell is cut off.
    await openSplit([BOOK], 544);
    fireEvent.click(breakButton(4, 5));

    const [line] = breakLines();
    expect(line.style.left).toBe(`${dropLineLeft(4, 544)}px`);
    expect(Number.parseFloat(line.style.left)).toBeLessThan(4 * 140 - 8);
    const button = breakButton(4, 5);
    const { left, width } = breakButtonBox(4, 544);
    expect(button).toHaveStyle({ left: `${left}px`, width: `${width}px` });
    expect(width).toBeLessThan(16);
  });

  it("name the output on the first page of it for a screen reader", async () => {
    await openSplit();
    fireEvent.click(breakButton(2, 3));

    expect(cellOf(1, 3)).toHaveAttribute(
      "aria-label",
      "book.pdf 3 ページ、2 番目の PDF の先頭",
    );
    expect(cellOf(1, 1)).toHaveAttribute(
      "aria-label",
      "book.pdf 1 ページ、1 番目の PDF の先頭",
    );
  });
});

describe("pressing between the pages", () => {
  it("puts a break in and takes it out", async () => {
    await openSplit();
    const button = breakButton(3, 4);
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(button).toHaveAttribute("tabindex", "-1");

    fireEvent.click(button);
    expect(breakButton(3, 4)).toHaveAttribute("aria-pressed", "true");
    expect(badgeOf(cellOf(1, 4))).toBe("2");
    expect(
      screen.getByText(
        "8 ページを 2 個の PDF に分けて保存します（合計 約 8 MB）",
      ),
    ).toBeInTheDocument();

    fireEvent.click(breakButton(3, 4));
    expect(breakButton(3, 4)).toHaveAttribute("aria-pressed", "false");
    expect(breakLines()).toEqual([]);
  });

  it("has a place between the pages of a row's end and the next row's start", async () => {
    await openSplit();

    // Page 5 ends the first row, page 6 starts the second; the place is drawn at the end of the first.
    expect(breakButton(5, 6)).toHaveStyle({
      left: `${5 * 140 - 16}px`,
      top: "0px",
      width: "16px",
      height: `${ORGANIZE_PAGE_BOX_SIDE}px`,
    });
    // Every page but the first has one.
    expect(breakButtons()).toHaveLength(7);
  });

  it("is not a stop of the Tab key and does not take the focus the keys work on", async () => {
    await openSplit();
    click(cellOf(1, 2));
    const cell = cellOf(1, 2);
    cell.focus();

    const down = fireEvent.mouseDown(breakButton(2, 3));

    expect(down).toBe(false);
    expect(document.activeElement).toBe(cell);
    for (const button of breakButtons()) {
      expect(button).toHaveAttribute("tabindex", "-1");
    }
  });

  it("is undone like a page operation", async () => {
    await openSplit();
    fireEvent.click(breakButton(2, 3));
    click(cellOf(1, 1));

    fireEvent.keyDown(document.activeElement ?? document.body, {
      key: "z",
      ctrlKey: true,
    });

    expect(breakLines()).toEqual([]);
  });

  it("cannot be pressed while a page is dragged", async () => {
    await openSplit();
    pressCell(cellOf(1, 1), { clientX: 40, clientY: 40 });
    movePointer(40 + DRAG_THRESHOLD_PX + 20, 40);

    expect(breakButton(2, 3)).toBeDisabled();
    expect(document.querySelector(".organize-grid")).toHaveClass("is-dragging");
    fireEvent.click(breakButton(2, 3));
    expect(breakLines()).toEqual([]);

    releasePointer();
    expect(breakButton(2, 3)).toBeEnabled();
  });

  it("is kept from a press on a page: it selects the page and starts no break", async () => {
    await openSplit();

    click(cellOf(1, 3));

    expect(cellOf(1, 3)).toHaveAttribute("aria-selected", "true");
    expect(breakLines()).toEqual([]);
  });
});

describe("the B key", () => {
  it("puts the break before the focused page in and out", async () => {
    await openSplit();
    click(cellOf(1, 4));

    key(cellOf(1, 4), "b");
    expect(breakButton(3, 4)).toHaveAttribute("aria-pressed", "true");
    expect(badgeOf(cellOf(1, 4))).toBe("2");

    key(cellOf(1, 4), "B");
    expect(breakButton(3, 4)).toHaveAttribute("aria-pressed", "false");
  });

  it("does nothing on the first page", async () => {
    await openSplit();
    click(cellOf(1, 1));

    key(cellOf(1, 1), "b");

    expect(breakLines()).toEqual([]);
    expect(settingsButton("すべて外す")).toBeDisabled();
  });

  it("does nothing when the pages are saved as one PDF", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 4));

    key(cellOf(1, 4), "b");
    fireEvent.click(screen.getByRole("button", { name: "分けて保存" }));

    expect(breakLines()).toEqual([]);
  });

  it("is listed among the keys", async () => {
    await openSplit();
    fireEvent.click(screen.getByRole("button", { name: "キー操作の一覧" }));

    expect(
      screen.getByText(
        "フォーカスのページの前に区切りを入れる・外す（分けて保存のとき）",
      ),
    ).toBeInTheDocument();
  });
});

describe("the shortcuts", () => {
  it("put a break before each page, before each PDF's first page, and take them all off", async () => {
    const second = organizePdf(2, "second.pdf", 3);
    await openSplit([organizePdf(1, "first.pdf", 3), second]);
    const clear = settingsButton("すべて外す");
    expect(clear).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "元の PDF ごと" }));
    expect(badges().map((badge) => badge.textContent)).toEqual(["1", "2"]);
    expect(badgeOf(cellOf(2, 1))).toBe("2");
    expect(clear).toBeEnabled();

    fireEvent.click(screen.getByRole("button", { name: "1 ページずつ" }));
    expect(badges()).toHaveLength(6);

    fireEvent.click(clear);
    expect(badges().map((badge) => badge.textContent)).toEqual(["1"]);
    expect(clear).toBeDisabled();
  });

  it("are not pressable with no page", async () => {
    await openOrganizeTab({ settings: SPLIT_SETTINGS });
    await flush();

    for (const name of ["1 ページずつ", "元の PDF ごと", "すべて外す"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
  });
});

describe("a grid of ten thousand pages", () => {
  const BIG = organizePdf(1, "big.pdf", 10_000);

  it("draws the badges, lines and places that press only for the cells in view", async () => {
    await openSplit([BIG]);
    fireEvent.click(screen.getByRole("button", { name: "1 ページずつ" }));

    const drawn = COLUMNS * (ROWS_IN_VIEW + ORGANIZE_OVERSCAN_ROWS);
    expect(drawnCells()).toHaveLength(drawn);
    expect(badges()).toHaveLength(drawn);
    // The first page has no break before it.
    expect(breakButtons()).toHaveLength(drawn - 1);
    expect(breakLines()).toHaveLength(drawn - 1);

    scrollGridTo(500 * ROW_PITCH);
    await flush();

    // The rows in view and two more on each side, and the page Tab enters, far above.
    const rows = ROWS_IN_VIEW - 1 + 2 * ORGANIZE_OVERSCAN_ROWS + 1;
    expect(drawnCells()).toHaveLength(COLUMNS * rows + 1);
    expect(badges()).toHaveLength(COLUMNS * rows + 1);
    expect(breakButtons()).toHaveLength(COLUMNS * rows);
    expect(breakLines()).toHaveLength(COLUMNS * rows);
    expect(
      breakLines().every((line) => Number.parseFloat(line.style.top) > 80_000),
    ).toBe(true);
  });
});

describe("while a save runs", () => {
  it("is not the place to press a break", async () => {
    await openSplit();
    fireEvent.click(breakButton(2, 3));
    fireEvent.click(screen.getByRole("button", { name: "保存を開始" }));
    await flush();

    expect(breakButton(2, 3)).toBeDisabled();
    click(cellOf(1, 5));
    key(cellOf(1, 5), "b");
    expect(breakLines()).toHaveLength(1);
    expect(settingsButton("1 ページずつ")).toBeDisabled();
    expect(settingsButton("すべて外す")).toBeDisabled();
  });
});
