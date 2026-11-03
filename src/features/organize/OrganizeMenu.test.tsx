import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  SPLIT_SETTINGS,
  breakLines,
  cellOf,
  deferred,
  drawnCells,
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
const ADD_BREAK = "前に区切りを追加";
const REMOVE_BREAK = "前の区切りを解除";

function click(cell: HTMLElement, init: { ctrlKey?: boolean } = {}) {
  pressCell(cell, init);
  releasePointer();
}

function item(name: string): HTMLElement {
  return screen.getByRole("menuitem", { name });
}

function itemNames(): (string | null)[] {
  return screen
    .getAllByRole("menuitem")
    .map((element) => element.firstChild?.textContent ?? null);
}

function angleOf(page: number): string | null | undefined {
  return cellOf(1, page).querySelector(".organize-cell-angle")?.textContent;
}

describe("the menu of a page", () => {
  it("replaces the browser's menu, on a page and on an empty place", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    expect(fireEvent.contextMenu(scrollArea())).toBe(false);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    expect(fireEvent.contextMenu(cellOf(1, 2))).toBe(false);
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("selects a page outside the selection alone", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    click(cellOf(1, 3), { ctrlKey: true });

    fireEvent.contextMenu(cellOf(1, 5));

    expect(selectedKeys()).toEqual(["1:5"]);
  });

  it("works on the whole selection from a page in it", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });
    click(cellOf(1, 2));
    click(cellOf(1, 3), { ctrlKey: true });

    fireEvent.contextMenu(cellOf(1, 3));
    expect(selectedKeys()).toEqual(["1:2", "1:3"]);
    fireEvent.click(item("右に回す"));

    expect(angleOf(2)).toBe("90°");
    expect(angleOf(3)).toBe("90°");
    expect(angleOf(4)).toBeUndefined();
  });

  it("turns left and deletes", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.contextMenu(cellOf(1, 2));
    fireEvent.click(item("左に回す"));
    expect(angleOf(2)).toBe("270°");

    fireEvent.contextMenu(cellOf(1, 2));
    fireEvent.click(item("削除"));
    expect(drawnCells()).toHaveLength(7);
    expect(document.activeElement).toHaveAttribute("data-page-key", "1:3");
  });

  it("names the keys that do the same", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.contextMenu(cellOf(1, 2));

    expect(item("左に回す")).toHaveAttribute("aria-keyshortcuts", "L");
    expect(item("右に回す")).toHaveAttribute("aria-keyshortcuts", "R");
    expect(item("削除")).toHaveAttribute("aria-keyshortcuts", "Delete");
    expect(item("削除")).toHaveTextContent("Delete");
  });

  it("gives the focus back to the page on Esc", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.contextMenu(cellOf(1, 2));
    fireEvent.keyDown(item("左に回す"), { key: "Escape" });

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(document.activeElement).toBe(cellOf(1, 2));
  });

  it("leaves the break out of the save into one PDF", async () => {
    await openOrganizeTab({ pdfs: [BOOK] });

    fireEvent.contextMenu(cellOf(1, 2));

    expect(itemNames()).toEqual(["左に回す", "右に回す", "削除"]);
  });

  it("puts in and takes out the break before the page pressed alone", async () => {
    await openOrganizeTab({ pdfs: [BOOK], settings: SPLIT_SETTINGS });
    click(cellOf(1, 2));
    click(cellOf(1, 3), { ctrlKey: true });

    fireEvent.contextMenu(cellOf(1, 3));
    fireEvent.click(item(ADD_BREAK));
    expect(breakLines()).toHaveLength(1);

    fireEvent.contextMenu(cellOf(1, 3));
    fireEvent.click(item(REMOVE_BREAK));
    expect(breakLines()).toHaveLength(0);
  });

  it("cannot break before the first page", async () => {
    await openOrganizeTab({ pdfs: [BOOK], settings: SPLIT_SETTINGS });

    fireEvent.contextMenu(cellOf(1, 1));

    expect(item(ADD_BREAK)).toBeDisabled();
  });

  it("does not open during a save, and closes when one starts", async () => {
    const saving = deferred<null>();
    await openOrganizeTab({
      pdfs: [BOOK],
      handlers: { save_organized_pdf: () => saving.promise },
    });
    fireEvent.contextMenu(cellOf(1, 2));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "PDF を保存" }));
    await flush();

    expect(fireEvent.contextMenu(cellOf(1, 3))).toBe(false);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
