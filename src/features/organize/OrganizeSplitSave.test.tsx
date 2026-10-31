import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { formatErrorMessage } from "../../i18n";
import { MAX_PDF_PAGES } from "../../state";
import {
  FINISHED_NONE,
  SPLIT_SETTINGS,
  badges,
  breakButton,
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
  waitingPdf,
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

const REPORT = organizePdf(1, "report.pdf", 4);
const CONTRACT = organizePdf(2, "contract.pdf", 2);

function startButton(): HTMLElement {
  return screen.getByRole("button", { name: "保存を開始" });
}

function click(cell: HTMLElement) {
  pressCell(cell);
  releasePointer();
}

function settingsButton(name: string): HTMLElement {
  return within(screen.getByRole("complementary")).getByRole("button", {
    name,
  });
}

function outputDirText(): string {
  return screen.getByTestId("output-dir-name").textContent ?? "";
}

/** Opens the split tab with `pdfs`; a start answers when the test says. */
async function openSplit(
  pdfs = [REPORT, CONTRACT],
  options: {
    language?: "ja" | "en";
    settings?: typeof SPLIT_SETTINGS;
    handlers?: Record<string, (args: Record<string, unknown>) => unknown>;
  } = {},
) {
  const answer = deferred<undefined>();
  const calls = await openOrganizeTab({
    pdfs,
    settings: options.settings ?? SPLIT_SETTINGS,
    language: options.language,
    handlers: {
      start_organize_split: () => answer.promise,
      ...options.handlers,
    },
  });
  return { calls, answer };
}

/** The `parts` the last `start_organize_split` carried. */
function sentParts(calls: Parameters<typeof callsOf>[0]) {
  const sent = callsOf(calls, "start_organize_split");
  return sent.map((call) => call.args);
}

describe("starting the save in parts", () => {
  it("sends the outputs in the order of the grid, each page as it is turned", async () => {
    const { calls } = await openSplit();
    // report 1..4, contract 1..2 → delete 1:2, turn 2:1 right, break before 1:4 and before 2:1.
    click(cellOf(1, 2));
    fireEvent.click(screen.getByRole("button", { name: "削除" }));
    click(cellOf(2, 1));
    fireEvent.click(screen.getByRole("button", { name: "右に回す" }));
    fireEvent.click(breakButton(3, 4));
    fireEvent.click(breakButton(4, 1));

    fireEvent.click(startButton());
    await flush();

    expect(sentParts(calls)).toEqual([
      {
        parts: [
          [
            { id: 1, page: 1, rotation: 0 },
            { id: 1, page: 3, rotation: 0 },
          ],
          [{ id: 1, page: 4, rotation: 0 }],
          [
            { id: 2, page: 1, rotation: 90 },
            { id: 2, page: 2, rotation: 0 },
          ],
        ],
        password: null,
      },
    ]);
  });

  it("sends one output for the pages when there is no break", async () => {
    const { calls } = await openSplit([REPORT]);

    fireEvent.click(startButton());
    await flush();

    expect(sentParts(calls)[0]).toEqual({
      parts: [[1, 2, 3, 4].map((page) => ({ id: 1, page, rotation: 0 }))],
      password: null,
    });
  });

  it("asks for the folder first when none is chosen, and starts with the one chosen", async () => {
    const { calls } = await openSplit([REPORT], {
      settings: {
        ...SPLIT_SETTINGS,
        organize: { ...SPLIT_SETTINGS.organize, outputDir: null },
      },
      handlers: { pick_output_dir: () => ({ dirLabel: "chosen" }) },
    });
    expect(outputDirText()).toBe("未選択");

    fireEvent.click(startButton());
    await flush();

    expect(callsOf(calls, "pick_output_dir")[0].args).toEqual({
      kind: "organize",
    });
    expect(callsOf(calls, "start_organize_split")).toHaveLength(1);
    expect(outputDirText()).toBe("chosen");
  });

  it("starts nothing when the folder dialog is cancelled", async () => {
    const { calls } = await openSplit([REPORT], {
      settings: {
        ...SPLIT_SETTINGS,
        organize: { ...SPLIT_SETTINGS.organize, outputDir: null },
      },
    });

    fireEvent.click(startButton());
    await flush();

    expect(callsOf(calls, "pick_output_dir")).toHaveLength(1);
    expect(callsOf(calls, "start_organize_split")).toHaveLength(0);
    expect(startButton()).toBeEnabled();
  });

  it("does not ask for a folder that is chosen", async () => {
    const { calls } = await openSplit([REPORT]);

    fireEvent.click(startButton());
    await flush();

    expect(callsOf(calls, "pick_output_dir")).toHaveLength(0);
  });

  it("asks for the password first, for all the outputs, and sends it", async () => {
    const { calls } = await openSplit([REPORT], {
      settings: {
        ...SPLIT_SETTINGS,
        organize: { ...SPLIT_SETTINGS.organize, encrypt: true },
      },
    });
    fireEvent.click(breakButton(2, 3));

    fireEvent.click(startButton());
    await flush();
    expect(callsOf(calls, "start_organize_split")).toHaveLength(0);
    const dialog = screen.getByRole("dialog", { name: "パスワードを付ける" });
    expect(dialog).toHaveTextContent(
      "保存する 2 個の PDF すべてに、同じパスワードを付けます。",
    );
    fireEvent.change(within(dialog).getByLabelText("パスワード"), {
      target: { value: "s3cret" },
    });
    fireEvent.change(within(dialog).getByLabelText("もう一度入力"), {
      target: { value: "s3cret" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "保存を開始" }));
    await flush();

    expect(sentParts(calls)[0].password).toBe("s3cret");
  });

  it("starts nothing when the password dialog is cancelled", async () => {
    const { calls } = await openSplit([REPORT], {
      settings: {
        ...SPLIT_SETTINGS,
        organize: { ...SPLIT_SETTINGS.organize, encrypt: true },
      },
    });
    fireEvent.click(startButton());
    await flush();

    fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "キャンセル",
      }),
    );
    await flush();

    expect(callsOf(calls, "start_organize_split")).toHaveLength(0);
    expect(startButton()).toBeEnabled();
  });

  it("forgets the folder Rust has forgotten, and says why nothing started above the PDFs", async () => {
    await openSplit([REPORT], {
      handlers: {
        start_organize_split: () => {
          throw { code: "OutputDirMissing", detail: null };
        },
      },
    });
    expect(outputDirText()).toBe("parts");

    fireEvent.click(startButton());
    await flush();

    expect(outputDirText()).toBe("未選択");
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(
      formatErrorMessage("OutputDirMissing", null, "ja"),
    );
    // It is a band above the row of PDFs, not in the bottom bar.
    expect(
      document.querySelector(".organize-toolbar")?.previousElementSibling,
    ).toContainElement(alert);
    expect(startButton()).toBeEnabled();
  });

  it("is not pressable with no page, over the limit, or while it runs", async () => {
    const { calls } = await openSplit([REPORT]);
    fireEvent.click(startButton());
    await flush();
    expect(callsOf(calls, "start_organize_split")).toHaveLength(1);
    // The bar shows Cancel while it runs.
    expect(screen.queryByRole("button", { name: "保存を開始" })).toBeNull();
    expect(screen.getByRole("button", { name: "キャンセル" })).toBeEnabled();
  });

  it("is disabled until a PDF is added", async () => {
    await openOrganizeTab({ settings: SPLIT_SETTINGS });

    expect(startButton()).toBeDisabled();
  });
});

describe("what the bottom bar says before saving in parts", () => {
  it("counts the pages and the outputs, and the size of each output added up", async () => {
    // 4 pages of 4,000,000 bytes and 2 pages of 2,000,000.
    await openSplit();
    expect(
      screen.getByText(
        "6 ページを 1 個の PDF に分けて保存します（合計 約 6 MB）",
      ),
    ).toBeInTheDocument();

    fireEvent.click(breakButton(2, 3));
    expect(
      screen.getByText(
        "6 ページを 2 個の PDF に分けて保存します（合計 約 6 MB）",
      ),
    ).toBeInTheDocument();
  });

  it("adds the size of each output, rounded up on its own", async () => {
    // 1,048,576 bytes in 3 pages. As one output it is exactly 1 MB. A third of it
    // rounds up to 349,526 bytes, and three of those are 1,048,578 bytes: over 1 MB.
    const odd = organizePdf(1, "odd.pdf", 3, { fileBytes: 1_048_576 });
    await openSplit([odd]);
    expect(
      screen.getByText(
        "3 ページを 1 個の PDF に分けて保存します（合計 約 1 MB）",
      ),
    ).toBeInTheDocument();

    fireEvent.click(settingsButton("1 ページずつ"));
    expect(
      screen.getByText(
        "3 ページを 3 個の PDF に分けて保存します（合計 約 2 MB）",
      ),
    ).toBeInTheDocument();
  });

  it("says one page as one PDF in English, and the plural otherwise", async () => {
    await openSplit([organizePdf(1, "one.pdf", 1)], { language: "en" });
    expect(
      screen.getByText("Save 1 page as 1 PDF (about 1 MB in total)"),
    ).toBeInTheDocument();
    cleanup();

    await openSplit([REPORT], { language: "en" });
    expect(
      screen.getByText("Save 4 pages as 1 PDF (about 4 MB in total)"),
    ).toBeInTheDocument();
    fireEvent.click(breakButton(2, 3, "en"));
    expect(
      screen.getByText("Save 4 pages as 2 PDFs (about 4 MB in total)"),
    ).toBeInTheDocument();
  });

  it("tells which output has too many pages, and cannot be started until a break splits it", async () => {
    await openSplit([
      organizePdf(1, "small.pdf", 2),
      organizePdf(2, "big.pdf", MAX_PDF_PAGES + 1),
    ]);
    fireEvent.click(settingsButton("元の PDF ごと"));

    const reason = screen.getByText(
      `2 番目の PDF は 10001 ページあります。1 つの PDF は 10,000 ページまでです`,
    );
    expect(reason).toHaveClass("error-text");
    expect(startButton()).toBeDisabled();

    fireEvent.click(settingsButton("1 ページずつ"));
    expect(startButton()).toBeEnabled();
  });

  it("says it in English", async () => {
    await openSplit([organizePdf(1, "big.pdf", MAX_PDF_PAGES + 1)], {
      language: "en",
    });

    expect(
      screen.getByText(
        "PDF 1 has 10001 pages. One PDF has at most 10,000 pages",
      ),
    ).toBeInTheDocument();
  });

  it("adds how many PDFs are left out, as the save of one PDF does", async () => {
    await openSplit([REPORT, waitingPdf(3, "locked.pdf")]);

    expect(
      screen.getByText(
        "4 ページを 1 個の PDF に分けて保存します（合計 約 4 MB）（1 個の PDF は保存に入りません）",
      ),
    ).toBeInTheDocument();
  });

  it("asks for a break when one PDF would have too many pages, in the save of one PDF", async () => {
    await openOrganizeTab({
      pdfs: [organizePdf(1, "big.pdf", MAX_PDF_PAGES + 1)],
    });

    expect(
      screen.getByText(/区切りを入れて分けて保存してください/),
    ).toHaveClass("error-text");
  });
});

describe("what the bottom bar says after saving in parts", () => {
  /** Starts the save of `REPORT` in three outputs, and leaves it running. */
  async function runWith(language: "ja" | "en" = "ja") {
    const { answer } = await openSplit([REPORT], { language });
    fireEvent.click(breakButton(1, 2, language));
    fireEvent.click(breakButton(2, 3, language));
    fireEvent.click(
      screen.getByRole("button", {
        name: language === "ja" ? "保存を開始" : "Start saving",
      }),
    );
    await flush();
    await emitJobProgress({ done: 0, total: 4, current: "report_1.pdf" });
    return { answer };
  }

  it("counts the PDFs it saved when every output was", async () => {
    const { answer } = await runWith();
    await emitJobItem({ id: 1, status: "ok", outputs: ["report_1.pdf"] });
    await emitJobItem({ id: 2, status: "ok", outputs: ["report_2.pdf"] });
    await emitJobItem({ id: 3, status: "ok", outputs: ["report_3.pdf"] });
    await emitJobFinished({ ...FINISHED_NONE, succeeded: 3 });
    answer.resolve(undefined);
    await flush();

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("3 個の PDF を保存しました");
    expect(status).toHaveClass("success-text");
  });

  it("says it in English, one PDF or several", async () => {
    const { answer } = await runWith("en");
    await emitJobItem({ id: 1, status: "ok", outputs: ["report_1.pdf"] });
    await emitJobFinished({ ...FINISHED_NONE, succeeded: 3 });
    answer.resolve(undefined);
    await flush();

    expect(screen.getByRole("status")).toHaveTextContent("Saved 3 PDFs");
  });

  it("gives the counts and the reason of the first output that failed, with the PDF to blame", async () => {
    const { answer } = await runWith();
    const error = { code: "PdfOpenFailed", detail: null } as const;
    await emitJobItem({ id: 1, status: "ok", outputs: ["report_1.pdf"] });
    await emitJobItem({
      id: 2,
      status: "failed",
      outputs: [],
      error,
      sourceId: 1,
    });
    await emitJobItem({
      id: 3,
      status: "failed",
      outputs: [],
      error: { code: "WorkerCrashed", detail: null },
    });
    await emitJobFinished({ ...FINISHED_NONE, succeeded: 1, failed: 2 });
    answer.resolve(undefined);
    await flush();

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent(
      `変換が終わりました：成功 1 件 · 失敗 2 件（2 番目の PDF: report.pdf: ${formatErrorMessage(error.code, error.detail, "ja")}）`,
    );
    expect(status).toHaveClass("error-text");
  });

  it("gives the reason alone when no PDF of the list is to blame", async () => {
    const { answer } = await runWith();
    const error = { code: "WorkerCrashed", detail: null } as const;
    await emitJobItem({ id: 1, status: "failed", outputs: [], error });
    await emitJobFinished({ ...FINISHED_NONE, succeeded: 2, failed: 1 });
    answer.resolve(undefined);
    await flush();

    expect(screen.getByRole("status")).toHaveTextContent(
      `成功 2 件 · 失敗 1 件（1 番目の PDF: ${formatErrorMessage(error.code, error.detail, "ja")}）`,
    );
  });

  it("gives the counts of what was saved and what was not reached after Cancel, as the other tabs do", async () => {
    const { answer } = await runWith();
    await emitJobItem({ id: 1, status: "ok", outputs: ["report_1.pdf"] });
    await emitJobFinished({
      ...FINISHED_NONE,
      succeeded: 1,
      unprocessed: 2,
      cancelled: true,
    });
    answer.resolve(undefined);
    await flush();

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent(
      "キャンセルしました：成功 1 件 · 未処理 2 件",
    );
    expect(status).not.toHaveTextContent("保存していません");
  });

  it("is told by how the save started, not by the setting after it", async () => {
    const { answer } = await runWith();
    await emitJobItem({ id: 1, status: "ok", outputs: ["report_1.pdf"] });
    await emitJobFinished({ ...FINISHED_NONE, succeeded: 3 });
    answer.resolve(undefined);
    await flush();

    // The setting changing drops the result, so it is told the same until then.
    expect(screen.getByRole("status")).toHaveTextContent(
      "3 個の PDF を保存しました",
    );
    fireEvent.click(settingsButton("1 つの PDF"));
    expect(
      screen.queryByText(/個の PDF を保存しました/),
    ).not.toBeInTheDocument();
  });

  it("turns the badge of an output that failed red with a mark, and back when the result goes", async () => {
    const { answer } = await runWith();
    const error = { code: "PdfOpenFailed", detail: null } as const;
    await emitJobItem({ id: 1, status: "ok", outputs: ["report_1.pdf"] });
    await emitJobItem({
      id: 2,
      status: "failed",
      outputs: [],
      error,
      sourceId: 1,
    });
    await emitJobItem({ id: 3, status: "ok", outputs: ["report_3.pdf"] });
    await emitJobFinished({ ...FINISHED_NONE, succeeded: 2, failed: 1 });
    answer.resolve(undefined);
    await flush();

    const failed = badges().filter((badge) =>
      badge.classList.contains("is-failed"),
    );
    expect(failed.map((badge) => badge.textContent)).toEqual(["✕ 2"]);
    expect(cellOf(1, 2)).toHaveAttribute(
      "aria-label",
      "report.pdf 2 ページ、2 番目の PDF の先頭（保存に失敗）",
    );
    expect(cellOf(1, 1)).toHaveAttribute(
      "aria-label",
      "report.pdf 1 ページ、1 番目の PDF の先頭",
    );

    // Pressing between the pages changes the pages: the result goes, and the badge is as it was.
    fireEvent.click(breakButton(3, 4));
    expect(
      badges().filter((badge) => badge.classList.contains("is-failed")),
    ).toEqual([]);
    expect(badges().map((badge) => badge.textContent)).toEqual([
      "1",
      "2",
      "3",
      "4",
    ]);
  });

  it("numbers the outputs from 1 as the job-item does", async () => {
    // The output numbered 1 failing is the first badge, not the second.
    const { answer } = await runWith();
    await emitJobItem({
      id: 1,
      status: "failed",
      outputs: [],
      error: { code: "PdfOpenFailed", detail: null },
    });
    await emitJobFinished({ ...FINISHED_NONE, succeeded: 2, failed: 1 });
    answer.resolve(undefined);
    await flush();

    expect(
      badges()
        .filter((badge) => badge.classList.contains("is-failed"))
        .map((badge) => badge.textContent),
    ).toEqual(["✕ 1"]);
  });
});

describe("the settings of the save in parts", () => {
  it("is sent when the way of saving changes, and starts from what get_settings gave", async () => {
    const calls = await openOrganizeTab({
      pdfs: [REPORT],
      settings: SPLIT_SETTINGS,
    });
    expect(
      within(screen.getByRole("group", { name: "保存の方法" })).getByRole(
        "button",
        { name: "分けて保存" },
      ),
    ).toHaveAttribute("aria-pressed", "true");
    expect(outputDirText()).toBe("parts");

    fireEvent.click(settingsButton("1 つの PDF"));
    await flush();

    expect(
      callsOf(calls, "save_settings").map((call) => call.args.settings),
    ).toEqual([
      expect.objectContaining({
        organize: { output: "single", encrypt: false },
      }),
    ]);
  });
});
