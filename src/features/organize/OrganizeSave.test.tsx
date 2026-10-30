import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { formatErrorMessage } from "../../i18n";
import type { SaveOrganizedResult } from "../../ipc";
import { MAX_PDF_PAGES } from "../../state";
import {
  FINISHED_NONE,
  callsOf,
  cellOf,
  deferred,
  emitJobFinished,
  emitJobItem,
  emitJobProgress,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openOrganizeTab,
  organizePdf,
  pressCell,
  releasePointer,
  savedPages,
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

const REPORT = organizePdf(1, "annual-report.pdf", 3);
const CONTRACT = organizePdf(2, "contract.pdf", 2);

function saveButton(): HTMLElement {
  return screen.getByRole("button", { name: "PDF を保存" });
}

function click(cell: HTMLElement) {
  pressCell(cell);
  releasePointer();
}

/**
 * Opens the tab with `pdfs` and a save that answers when the test says; the
 * events a save sends while it runs are sent with the `emit*` helpers.
 */
async function openWithSave(pdfs = [REPORT, CONTRACT]) {
  const answer = deferred<SaveOrganizedResult | null>();
  const calls = await openOrganizeTab({
    pdfs,
    handlers: { save_organized_pdf: () => answer.promise },
  });
  return { calls, answer };
}

/** What `save_organized_pdf` does for a PDF written: its events, then its answer. */
async function finishSaving(
  answer: ReturnType<typeof deferred<SaveOrganizedResult | null>>,
  result: SaveOrganizedResult,
  pages: number,
) {
  await emitJobProgress({ done: 0, total: pages, current: "out.pdf" });
  await emitJobProgress({ done: pages, total: pages, current: "out.pdf" });
  await emitJobItem({ id: 1, status: "ok", outputs: ["out.pdf"] });
  await emitJobFinished({ ...FINISHED_NONE, succeeded: 1 });
  answer.resolve(result);
  await flush();
}

describe("saving the pages as one PDF", () => {
  it("sends the pages in the order of the grid, as they are turned", async () => {
    const { calls } = await openWithSave();
    click(cellOf(1, 1));
    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    click(cellOf(2, 2));
    fireEvent.click(screen.getByRole("button", { name: "左に回す" }));
    click(cellOf(1, 2));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    pressCell(cellOf(2, 1));
    releasePointer();
    fireEvent.keyDown(document.activeElement ?? document.body, {
      key: "ArrowLeft",
      altKey: true,
    });

    fireEvent.click(saveButton());
    await flush();

    expect(callsOf(calls, "save_organized_pdf")).toHaveLength(1);
    expect(callsOf(calls, "save_organized_pdf")[0].args).toEqual({
      pages: [
        { id: 1, page: 1, rotation: 90 },
        { id: 1, page: 3, rotation: 0 },
        { id: 2, page: 1, rotation: 0 },
        { id: 2, page: 2, rotation: 270 },
      ],
      password: null,
    });
  });

  it("is the pages of the PDFs that opened, and none of those waiting or broken", async () => {
    const calls = await openOrganizeTab({
      pdfs: [
        REPORT,
        organizePdf(2, "locked.pdf", 0, {
          error: { code: "PasswordProtected", detail: null },
          encrypted: true,
        }),
      ],
    });

    fireEvent.click(saveButton());
    await flush();

    expect(savedPages(calls)).toEqual([
      [
        { id: 1, page: 1, rotation: 0 },
        { id: 1, page: 2, rotation: 0 },
        { id: 1, page: 3, rotation: 0 },
      ],
    ]);
  });

  it("asks for the password first when the setting says so, and sends it", async () => {
    const { calls } = await openWithSave();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );

    fireEvent.click(saveButton());
    await flush();
    expect(callsOf(calls, "save_organized_pdf")).toHaveLength(0);
    const dialog = screen.getByRole("dialog", { name: "パスワードを付ける" });
    expect(
      within(dialog).getByRole("button", { name: "保存先を選ぶ…" }),
    ).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("パスワード"), {
      target: { value: "s3cret" },
    });
    fireEvent.change(within(dialog).getByLabelText("もう一度入力"), {
      target: { value: "s3cret" },
    });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "保存先を選ぶ…" }),
    );
    await flush();

    expect(callsOf(calls, "save_organized_pdf")[0].args.password).toBe(
      "s3cret",
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("starts nothing when the password dialog is cancelled", async () => {
    const { calls } = await openWithSave();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );
    fireEvent.click(saveButton());
    await flush();

    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "キャンセル",
      }),
    );
    await flush();

    expect(callsOf(calls, "save_organized_pdf")).toHaveLength(0);
    expect(saveButton()).toBeEnabled();
  });

  it("goes back to the plain bottom bar when the save dialog is cancelled", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();
    expect(
      screen.getByRole("button", { name: "キャンセル" }),
    ).toBeInTheDocument();

    answer.resolve(null);
    await flush();

    expect(saveButton()).toBeEnabled();
    expect(
      screen.getByText(/5 ページを 1 つの PDF に保存します/),
    ).toBeInTheDocument();
  });

  it("shows the name of the file being made and the progress, then Cancel", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();
    expect(screen.getByText("変換中…")).toBeInTheDocument();

    await emitJobProgress({ done: 2, total: 5, current: "annual-report.pdf" });

    expect(screen.getByText("変換中：annual-report.pdf")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "2",
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuemax",
      "5",
    );
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    await flush();
    expect(screen.getAllByText("キャンセル中…").length).toBeGreaterThan(0);
    answer.resolve(null);
    await flush();
  });

  it("is cancelled by cancel_job", async () => {
    const { calls, answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();

    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    await flush();

    expect(callsOf(calls, "cancel_job")).toHaveLength(1);
    answer.resolve({ savedName: null, replacedId: null });
    await flush();
  });
});

describe("the pages of one PDF", () => {
  it("lets a save of exactly the most pages through", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "big.pdf", MAX_PDF_PAGES)] });

    expect(saveButton()).toBeEnabled();
    expect(screen.queryByText(/ページまでです/)).not.toBeInTheDocument();
  });

  it("refuses one page more, and says why in the bottom bar", async () => {
    await openOrganizeTab({
      pdfs: [
        organizePdf(1, "big.pdf", MAX_PDF_PAGES),
        organizePdf(2, "one.pdf", 1),
      ],
    });

    expect(saveButton()).toBeDisabled();
    const reason = screen.getByText(
      "1 つの PDF は 10,000 ページまでです（今は 10001 ページ）。区切りを入れて分けて保存してください",
    );
    expect(reason).toHaveClass("error-text");
  });

  it("is let through again once a page is deleted", async () => {
    await openOrganizeTab({
      pdfs: [
        organizePdf(1, "big.pdf", MAX_PDF_PAGES),
        organizePdf(2, "one.pdf", 1),
      ],
    });

    click(cellOf(1, 1));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    await flush();

    expect(saveButton()).toBeEnabled();
  });

  it("says the same in English", async () => {
    await openOrganizeTab({
      language: "en",
      pdfs: [organizePdf(1, "big.pdf", MAX_PDF_PAGES + 5)],
    });

    expect(
      screen.getByText(
        "One PDF has at most 10,000 pages (now 10005). Add breaks and save in parts",
      ),
    ).toBeInTheDocument();
  });
});

describe("what the bottom bar says before saving", () => {
  it("tells how many pages become the PDF and about how big it is", async () => {
    await openOrganizeTab({ pdfs: [REPORT, CONTRACT] });

    // 3 MB × 3 pages of 3 and 2 MB × 2 pages of 2: the whole of both, 5 MB.
    expect(
      screen.getByText("5 ページを 1 つの PDF に保存します（約 5 MB）"),
    ).toBeInTheDocument();

    click(cellOf(1, 1));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    await flush();

    // A third of the first PDF's 3,000,000 bytes is gone: 2,000,000 + 2,000,000.
    expect(
      screen.getByText("4 ページを 1 つの PDF に保存します（約 4 MB）"),
    ).toBeInTheDocument();
  });

  it("is in English too", async () => {
    await openOrganizeTab({
      language: "en",
      pdfs: [organizePdf(1, "a.pdf", 1)],
    });

    expect(
      screen.getByText("Save 1 page as one PDF (about 1 MB)"),
    ).toBeInTheDocument();
  });

  it("says there is nothing to save once every page is deleted", async () => {
    await openOrganizeTab({ pdfs: [organizePdf(1, "a.pdf", 1)] });

    click(cellOf(1, 1));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    await flush();

    expect(screen.getByText("保存するページがありません")).toBeInTheDocument();
  });
});

describe("what the bottom bar says after saving", () => {
  it("tells the pages written", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();

    await finishSaving(answer, { savedName: "out.pdf", replacedId: null }, 5);

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("5 ページを 1 つの PDF に保存しました");
    expect(status).toHaveClass("success-text");
    expect(status).not.toHaveTextContent("置き換えた");
  });

  it("tells that a PDF of the list was replaced, drops it, and does not ask Rust to", async () => {
    const { calls, answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();

    await finishSaving(
      answer,
      { savedName: "annual-report.pdf", replacedId: 1 },
      5,
    );

    expect(screen.getByRole("status")).toHaveTextContent(
      "5 ページを 1 つの PDF に保存しました · 置き換えた annual-report.pdf を一覧から外しました",
    );
    expect(callsOf(calls, "remove_organize_pdfs")).toHaveLength(0);
    expect(
      screen.queryByRole("button", { name: "annual-report.pdf を外す" }),
    ).toBeNull();
    expect(
      screen.getByRole("button", { name: "contract.pdf を外す" }),
    ).toBeInTheDocument();
    expect(cellOf(2, 1)).toBeInTheDocument();
    expect(() => cellOf(1, 1)).toThrow();
  });

  it("is told in English", async () => {
    const answer = deferred<SaveOrganizedResult | null>();
    await openOrganizeTab({
      language: "en",
      pdfs: [REPORT],
      handlers: { save_organized_pdf: () => answer.promise },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save PDF" }));
    await flush();

    await finishSaving(
      answer,
      { savedName: "annual-report.pdf", replacedId: 1 },
      3,
    );

    expect(screen.getByRole("status")).toHaveTextContent(
      "Saved 3 pages as one PDF · Removed the replaced annual-report.pdf from the list",
    );
  });

  it("says no PDF was saved after Cancel", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();
    await emitJobFinished({
      ...FINISHED_NONE,
      unprocessed: 1,
      cancelled: true,
    });
    answer.resolve({ savedName: null, replacedId: null });
    await flush();

    expect(screen.getByRole("status")).toHaveTextContent(
      "キャンセルしました：PDF は保存していません",
    );
  });

  it("names the PDF a failure came from, and the reason", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();
    const error = { code: "PdfOpenFailed", detail: null } as const;
    await emitJobItem({
      id: 1,
      status: "failed",
      outputs: [],
      error,
      sourceId: 2,
    });
    await emitJobFinished({ ...FINISHED_NONE, failed: 1 });
    answer.resolve({ savedName: null, replacedId: null });
    await flush();

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent(
      `PDF を保存しませんでした（contract.pdf: ${formatErrorMessage(error.code, error.detail, "ja")}）`,
    );
    expect(status).toHaveClass("error-text");
  });

  it("gives the reason alone when no PDF of the list is to blame", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();
    const error = { code: "WorkerCrashed", detail: null } as const;
    await emitJobItem({ id: 1, status: "failed", outputs: [], error });
    await emitJobFinished({ ...FINISHED_NONE, failed: 1 });
    answer.resolve({ savedName: null, replacedId: null });
    await flush();

    expect(screen.getByRole("status")).toHaveTextContent(
      `PDF を保存しませんでした（${formatErrorMessage(error.code, error.detail, "ja")}）`,
    );
  });

  it("shows OutputTooLarge, which Rust answers before anything starts, in the bottom bar", async () => {
    await openOrganizeTab({
      pdfs: [REPORT],
      handlers: {
        save_organized_pdf: () => {
          throw { code: "OutputTooLarge", detail: "613 MB / 512 MB" };
        },
      },
    });

    fireEvent.click(saveButton());
    await flush();

    const alert = screen.getByRole("alert");
    expect(alert).toHaveClass("error-text");
    expect(alert).toHaveTextContent("613 MB / 512 MB");
    expect(saveButton()).toBeEnabled();
  });

  it("drops the result once the pages change, and not when a page is only selected", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();
    await finishSaving(answer, { savedName: "out.pdf", replacedId: null }, 5);
    expect(screen.getByRole("status")).toBeInTheDocument();

    click(cellOf(2, 1));
    expect(screen.getByRole("status")).toHaveTextContent(
      "5 ページを 1 つの PDF に保存しました",
    );

    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    await flush();

    expect(screen.queryByText(/に保存しました/)).not.toBeInTheDocument();
    expect(
      screen.getByText(/5 ページを 1 つの PDF に保存します/),
    ).toBeInTheDocument();
  });

  it("drops the result when the password setting changes", async () => {
    const { answer } = await openWithSave();
    fireEvent.click(saveButton());
    await flush();
    await finishSaving(answer, { savedName: "out.pdf", replacedId: null }, 5);

    fireEvent.click(
      screen.getByRole("checkbox", { name: "パスワードを付ける" }),
    );

    expect(screen.queryByText(/に保存しました/)).not.toBeInTheDocument();
  });
});
