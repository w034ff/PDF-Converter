import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cellOf,
  drawnCells,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  layOutScrollArea,
  openOrganizeTab,
  organizePdf,
  pressCell,
  releasePointer,
  scrollArea,
  selectedKeys,
} from "./organizeTestSupport";

beforeEach(() => {
  installFakeResizeObserver(700, 600);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

const BOOK = organizePdf(1, "book.pdf", 8);

/** A press and a release in place, which is a click on a page. */
function click(
  cell: HTMLElement,
  init: { shiftKey?: boolean; ctrlKey?: boolean } = {},
) {
  pressCell(cell, init);
  releasePointer();
}

describe("selecting pages with the pointer", () => {
  it("a press selects the page alone, with a check, and the band counts it", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));

    click(cellOf(1, 4));

    expect(selectedKeys()).toEqual(["1:4"]);
    expect(cellOf(1, 4)).toHaveTextContent("✓");
    expect(cellOf(1, 2)).not.toHaveTextContent("✓");
    expect(screen.getByText("1 ページを選択中")).toBeInTheDocument();
  });

  it("Ctrl puts a page in and out of the selection", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));

    click(cellOf(1, 5), { ctrlKey: true });
    click(cellOf(1, 7), { ctrlKey: true });
    expect(selectedKeys().sort()).toEqual(["1:2", "1:5", "1:7"]);
    expect(screen.getByText("3 ページを選択中")).toBeInTheDocument();

    click(cellOf(1, 5), { ctrlKey: true });
    expect(selectedKeys().sort()).toEqual(["1:2", "1:7"]);
  });

  it("Shift selects from the page last pressed to this one and deselects the rest", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    click(cellOf(1, 8), { ctrlKey: true });
    click(cellOf(1, 3));

    click(cellOf(1, 6), { shiftKey: true });
    expect(selectedKeys().sort()).toEqual(["1:3", "1:4", "1:5", "1:6"]);

    click(cellOf(1, 1), { shiftKey: true });
    expect(selectedKeys().sort()).toEqual(["1:1", "1:2", "1:3"]);
  });

  it("an empty place deselects everything, and a page does not", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    click(cellOf(1, 3), { ctrlKey: true });
    layOutScrollArea(700, 600);

    fireEvent.pointerDown(scrollArea(), { button: 0, pointerId: 1 });
    expect(selectedKeys()).toEqual([]);
    expect(
      screen.getByText("8 ページ", { selector: ".organize-band-count" }),
    ).toBeInTheDocument();

    click(cellOf(1, 2));
    fireEvent.pointerDown(screen.getByRole("listbox"), {
      button: 0,
      pointerId: 1,
    });
    expect(selectedKeys()).toEqual([]);
  });

  it("does not take a press on the scroll bar for an empty place", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    layOutScrollArea(700, 600);

    // Past the content width, where the scroll bar is.
    fireEvent.pointerDown(scrollArea(), {
      button: 0,
      pointerId: 1,
      clientX: 710,
      clientY: 100,
    });

    expect(selectedKeys()).toEqual(["1:2"]);
  });

  it("does not take a press of another button for a selection", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.pointerDown(cellOf(1, 2), { button: 2, pointerId: 1 });

    expect(selectedKeys()).toEqual([]);
  });

  it("a press on a selected page that does not move leaves that page alone selected when it ends", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    click(cellOf(1, 3), { ctrlKey: true });
    click(cellOf(1, 4), { ctrlKey: true });

    pressCell(cellOf(1, 3));
    // Held down, the others are still selected: that is what a drag would move.
    expect(selectedKeys().sort()).toEqual(["1:2", "1:3", "1:4"]);
    releasePointer();

    expect(selectedKeys()).toEqual(["1:3"]);
  });

  it("a press on a page that is not selected selects it at once", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    click(cellOf(1, 3), { ctrlKey: true });

    pressCell(cellOf(1, 6));

    expect(selectedKeys()).toEqual(["1:6"]);
    releasePointer();
  });

  it("keeps the band's buttons, pressed in while nothing is selected", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    for (const name of ["左に回す", "右に回す", "削除"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(
      screen.getByText("8 ページ", { selector: ".organize-band-count" }),
    ).toBeInTheDocument();

    click(cellOf(1, 2));
    for (const name of ["左に回す", "右に回す", "削除"]) {
      expect(screen.getByRole("button", { name })).toBeEnabled();
    }
  });

  it("selects pages of different PDFs, which are one grid", async () => {
    await openOrganizeTab({
      pdfs: [organizePdf(1, "a.pdf", 2), organizePdf(2, "b.pdf", 2)],
    });
    click(cellOf(1, 2));

    click(cellOf(2, 1), { shiftKey: true });

    expect(selectedKeys().sort()).toEqual(["1:2", "2:1"]);
  });

  it("marks the selection for a screen reader", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    await flush();

    expect(screen.getByRole("listbox")).toHaveAttribute(
      "aria-multiselectable",
      "true",
    );
    expect(
      drawnCells().filter(
        (cell) => cell.getAttribute("aria-selected") === "true",
      ),
    ).toHaveLength(1);
    expect(cellOf(1, 3)).toHaveAttribute("aria-selected", "false");
  });
});

describe("the band", () => {
  it("turns the selected pages left and right, and deletes them", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    click(cellOf(1, 3), { ctrlKey: true });

    fireEvent.click(screen.getByRole("button", { name: "左に回す" }));
    expect(cellOf(1, 2)).toHaveTextContent("270°");
    expect(cellOf(1, 3)).toHaveTextContent("270°");
    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    expect(cellOf(1, 2)).toHaveTextContent("90°");

    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    expect(drawnCells().map((cell) => cell.dataset.pageKey)).toEqual([
      "1:1",
      "1:4",
      "1:5",
      "1:6",
      "1:7",
      "1:8",
    ]);
    expect(selectedKeys()).toEqual([]);
    expect(
      screen.getByText("6 ページ", { selector: ".organize-band-count" }),
    ).toBeInTheDocument();
  });

  it("says the grid is empty when every page is deleted, and how to bring them back", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "a.pdf", 2)] });
    click(cellOf(1, 1));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    click(cellOf(1, 2));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    await flush();

    expect(screen.getByText("ページがありません")).toBeInTheDocument();
    expect(screen.getByText("Ctrl+Z で戻せます")).toBeInTheDocument();
    expect(screen.getByText("保存するページがありません")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "PDF を保存" })).toBeDisabled();

    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();

    expect(drawnCells()).toHaveLength(1);
  });
});
