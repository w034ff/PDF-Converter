import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  callsOf,
  cellOf,
  drawnKeys,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openOrganizeTab,
  organizePdf,
  pressCell,
  releasePointer,
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

const PDFS = [organizePdf(1, "a.pdf", 3), organizePdf(2, "b.pdf", 2)];

function clearAllButton(): HTMLElement {
  return screen.getByRole("button", { name: "すべて外す" });
}

/** Deletes the first page: a page operation, which "すべて外す" would lose. */
function editPages() {
  pressCell(cellOf(1, 1));
  releasePointer();
  fireEvent.click(screen.getByRole("button", { name: "削除" }));
}

describe("removing all the PDFs", () => {
  it("asks no question before any page was edited", async () => {
    const calls = await openOrganizeTab({ pdfs: PDFS });

    fireEvent.click(clearAllButton());
    await flush();

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(callsOf(calls, "remove_organize_pdfs")[0].args).toEqual({
      ids: [1, 2],
    });
    expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
  });

  it("asks after a page was edited, with the focus on Cancel", async () => {
    await openOrganizeTab({ pdfs: PDFS });
    editPages();

    fireEvent.click(clearAllButton());
    await flush();

    const dialog = screen.getByRole("dialog", {
      name: "すべての PDF を外しますか？",
    });
    expect(within(dialog).getByText(/取り消せません/)).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "キャンセル" }),
    ).toHaveFocus();
  });

  it("keeps everything when the question is cancelled", async () => {
    const calls = await openOrganizeTab({ pdfs: PDFS });
    editPages();
    fireEvent.click(clearAllButton());
    await flush();

    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "キャンセル",
      }),
    );
    await flush();

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(callsOf(calls, "remove_organize_pdfs")).toHaveLength(0);
    expect(drawnKeys()).toEqual(["1:2", "1:3", "2:1", "2:2"]);
    expect(clearAllButton()).toHaveFocus();
  });

  it("removes the PDFs when it is confirmed, and starts again from nothing", async () => {
    const calls = await openOrganizeTab({ pdfs: PDFS });
    editPages();
    fireEvent.click(clearAllButton());
    await flush();

    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "すべて外す",
      }),
    );
    await flush();

    expect(callsOf(calls, "remove_organize_pdfs")[0].args).toEqual({
      ids: [1, 2],
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
    // The history is gone with the pages: nothing is undone into the empty tab.
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();
    expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
  });

  it("keeps the settings", async () => {
    await openOrganizeTab({ pdfs: PDFS });
    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );
    editPages();
    fireEvent.click(clearAllButton());
    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "すべて外す",
      }),
    );
    await flush();

    expect(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    ).toBeChecked();
  });

  it("asks nothing after the edit is undone", async () => {
    await openOrganizeTab({ pdfs: PDFS });
    editPages();
    fireEvent.keyDown(document.body, { key: "z", ctrlKey: true });
    await flush();

    fireEvent.click(clearAllButton());
    await flush();

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("asks nothing for removing one PDF, whatever was edited", async () => {
    await openOrganizeTab({ pdfs: PDFS });
    editPages();

    fireEvent.click(screen.getByRole("button", { name: "b.pdf を外す" }));
    await flush();

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(drawnKeys()).toEqual(["1:2", "1:3"]);
  });

  it("keeps the PDFs when Rust refuses to remove them", async () => {
    await openOrganizeTab({
      pdfs: PDFS,
      handlers: {
        remove_organize_pdfs: () => {
          throw { code: "ConversionRunning", detail: null };
        },
      },
    });

    fireEvent.click(clearAllButton());
    await flush();

    expect(drawnKeys()).toEqual(["1:1", "1:2", "1:3", "2:1", "2:2"]);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
