import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Candidate, IpcError } from "../../ipc";
import {
  areaRows,
  callsOf,
  candidate,
  cropPdf,
  deferred,
  emitJobFinished,
  emitJobProgress,
  emitLayoutPage,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openPdfInCropTab,
  rowText,
  type Call,
  type Handler,
} from "./cropTestSupport";

beforeEach(() => {
  installFakeResizeObserver(600, 800);
  installFakeObjectUrls();
});

afterEach(() => {
  cleanup();
  clearMocks();
  vi.unstubAllGlobals();
});

const TIMEOUT: IpcError = { code: "WorkerTimeout", detail: null };
const UNAVAILABLE: IpcError = { code: "LayoutUnavailable", detail: null };

const chart: Candidate = candidate("chart", [50, 60, 250, 160]);
const table: Candidate = candidate("table", [50, 300, 250, 400]);

function pickPage(page: number) {
  // A page with areas says how many in its name.
  fireEvent.click(
    screen.getByRole("button", { name: new RegExp(`^ページ ${page}(、|$)`) }),
  );
}

async function startDetectAll() {
  fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
  fireEvent.click(
    screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
  );
  await flush();
}

async function answer(
  page: number,
  candidates: Candidate[],
  error: IpcError | null = null,
) {
  await emitLayoutPage({ id: 7, page, candidates, error });
}

function pages(): string[] {
  return areaRows().map((row) => rowText(row).slice(0, 3));
}

function detectLayoutPages(calls: Call[]): unknown[] {
  return callsOf(calls, "detect_layout").map((c) => c.args.page);
}

describe("starting the search of all pages", () => {
  it("adds the candidates of the pages already analyzed at once, without selecting them", async () => {
    const calls = await openPdfInCropTab({
      detect_layout: () => [chart, table],
    });
    await startDetectAll();

    expect(callsOf(calls, "start_detect_all").map((c) => c.args)).toEqual([
      { id: 7 },
    ]);
    expect(areaRows().map((row) => rowText(row))).toEqual([
      expect.stringMatching(/^p\.1グラフ/),
      expect.stringMatching(/^p\.1表/),
    ]);
    expect(
      screen
        .queryAllByTestId("crop-area")
        .filter((a) => a.dataset.selected === "true"),
    ).toHaveLength(0);
  });

  it("says in the bottom bar that all pages are searched, with a way to cancel", async () => {
    await openPdfInCropTab();
    await startDetectAll();
    expect(
      screen.getByText("全ページの候補を探しています…"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "キャンセル" })).toBeEnabled();

    await emitJobProgress({ done: 1, total: 4, current: null });
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
    expect(
      screen.getByText("全ページの候補を探しています…"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/変換中/)).toBeNull();
  });

  it("keeps saying cancelling once it is cancelled", async () => {
    await openPdfInCropTab();
    await startDetectAll();
    await emitJobProgress({ done: 1, total: 4, current: null });
    fireEvent.click(screen.getByRole("button", { name: "キャンセル" }));
    await flush();
    expect(screen.getAllByText("キャンセル中…").length).toBeGreaterThan(0);
    expect(screen.queryByText("全ページの候補を探しています…")).toBeNull();
  });

  it("leaves the other bottom bars as they were", async () => {
    await openPdfInCropTab();
    await startDetectAll();
    await emitJobProgress({ done: 1, total: 4, current: null });
    fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
    // The conversion shows on every tab, and what it is doing is the crop's.
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
  });
});

describe("the answers of the pages", () => {
  it("add the candidates of a page, which then needs no analysis of its own", async () => {
    const calls = await openPdfInCropTab({ detect_layout: () => [] });
    await startDetectAll();
    await answer(3, [chart, table]);

    expect(pages()).toEqual(["p.3", "p.3"]);
    expect(rowText(areaRows()[0])).toContain("グラフ");

    pickPage(3);
    await flush();
    expect(detectLayoutPages(calls)).toEqual([1]);
    expect(screen.getAllByTestId("crop-area")).toHaveLength(2);
    // The candidates are drawn too, as for any analyzed page.
    expect(screen.getAllByTestId("crop-candidate")).toHaveLength(2);
  });

  it("add the pages in the order they come, by page in the list", async () => {
    await openPdfInCropTab({ detect_layout: () => [] });
    await startDetectAll();
    await answer(4, [chart]);
    await answer(2, [table]);
    expect(pages()).toEqual(["p.2", "p.4"]);
  });

  it("do not add a candidate that repeats an area, and the page's own analysis does not add it again", async () => {
    await openPdfInCropTab({ detect_layout: () => [] });
    await startDetectAll();
    await answer(2, [chart]);
    await answer(2, [chart]);
    expect(areaRows()).toHaveLength(1);
  });

  it("show the error of a page where the page's candidates would be, and carry on", async () => {
    await openPdfInCropTab({ detect_layout: () => [] });
    await startDetectAll();
    await answer(2, [], TIMEOUT);
    await answer(3, [chart]);

    pickPage(2);
    await flush();
    expect(
      screen.getByText("PDF の処理に時間がかかりすぎたため中止しました"),
    ).toBeInTheDocument();
    expect(areaRows()).toHaveLength(1);
  });

  it("make the analysis unavailable for LayoutUnavailable", async () => {
    await openPdfInCropTab({ detect_layout: () => [] });
    await startDetectAll();
    await answer(2, [], UNAVAILABLE);

    expect(
      screen.getByText(
        "候補を表示できません（レイアウトの解析を使えません）。範囲は手で選べます",
      ),
    ).toBeInTheDocument();
    await emitJobFinished({
      succeeded: 0,
      failed: 1,
      noPages: 0,
      unprocessed: 3,
      cancelled: false,
    });
    fireEvent.click(screen.getByRole("button", { name: "範囲を追加 ▾" }));
    expect(
      screen.getByRole("menuitem", { name: "全ページの候補をすべて追加" }),
    ).toBeDisabled();
  });

  it("are ignored when they belong to another PDF", async () => {
    await openPdfInCropTab({ detect_layout: () => [] });
    await startDetectAll();
    await emitLayoutPage({ id: 99, page: 2, candidates: [chart], error: null });
    expect(areaRows()).toHaveLength(0);
  });

  it("add areas while the conversion runs, although the areas are locked", async () => {
    await openPdfInCropTab({ detect_layout: () => [] });
    await startDetectAll();
    expect(screen.getByRole("button", { name: "範囲を追加 ▾" })).toBeDisabled();
    await answer(2, [chart]);
    expect(areaRows()).toHaveLength(1);
  });
});

describe("a page whose analysis was still on its way", () => {
  async function pending() {
    const held = deferred<Candidate[]>();
    const handlers: Record<string, Handler> = {
      detect_layout: () => held.promise,
    };
    const calls = await openPdfInCropTab(handlers);
    await startDetectAll();
    return { held, calls };
  }

  it("has its candidates added when that answer arrives", async () => {
    const { held } = await pending();
    expect(areaRows()).toHaveLength(0);

    held.resolve([chart, table]);
    await flush();
    expect(areaRows()).toHaveLength(2);
  });

  it("has them added after the conversion has finished too, and the result stays", async () => {
    const { held } = await pending();
    await emitJobFinished({
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    expect(screen.getByRole("status")).toHaveTextContent(
      "全ページの候補を探しました：追加した範囲はありません",
    );

    held.resolve([chart]);
    await flush();
    expect(areaRows()).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent(
      "全ページの候補を探しました：範囲を 1 個追加",
    );
  });

  it("is not added twice when the search answers for it too", async () => {
    const { held } = await pending();
    await answer(1, [chart]);
    held.resolve([chart]);
    await flush();
    expect(areaRows()).toHaveLength(1);
  });

  it("adds nothing when its analysis failed", async () => {
    const held = deferred<Candidate[]>();
    await openPdfInCropTab({ detect_layout: () => held.promise });
    await startDetectAll();
    held.reject(TIMEOUT);
    await flush();
    expect(areaRows()).toHaveLength(0);
  });
});

describe("the result of the search", () => {
  async function finishWith(
    finished: Parameters<typeof emitJobFinished>[0],
    candidatesByPage: [number, Candidate[], IpcError | null][],
    language: "ja" | "en" = "ja",
  ) {
    await openPdfInCropTab({ detect_layout: () => [] }, cropPdf(), language);
    fireEvent.click(
      screen.getByRole("button", {
        name: language === "ja" ? "範囲を追加 ▾" : "Add areas ▾",
      }),
    );
    fireEvent.click(
      screen.getByRole("menuitem", {
        name:
          language === "ja"
            ? "全ページの候補をすべて追加"
            : "Add all suggestions on all pages",
      }),
    );
    await flush();
    for (const [page, candidates, error] of candidatesByPage) {
      await answer(page, candidates, error);
    }
    await emitJobFinished(finished);
  }

  it("counts the areas added and the pages that failed", async () => {
    await finishWith(
      {
        succeeded: 2,
        failed: 1,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
      [
        [2, [chart, table], null],
        [3, [], TIMEOUT],
        [4, [chart], null],
      ],
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "全ページの候補を探しました：範囲を 3 個追加 · 失敗 1 ページ",
    );
    expect(screen.getByText("失敗があります")).toBeInTheDocument();
  });

  it("says no area was added, leaving out the counts of zero", async () => {
    await finishWith(
      {
        succeeded: 4,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
      [],
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "全ページの候補を探しました：追加した範囲はありません",
    );
    expect(screen.getByRole("status").textContent).not.toMatch(/失敗|未処理/);
  });

  it("counts the pages not reached when it is cancelled", async () => {
    await finishWith(
      {
        succeeded: 1,
        failed: 0,
        noPages: 0,
        unprocessed: 3,
        cancelled: true,
      },
      [[2, [chart], null]],
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "キャンセルしました：範囲を 1 個追加 · 未処理 3 ページ",
    );
  });

  it("is colored by how it went", async () => {
    await finishWith(
      {
        succeeded: 4,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
      [[2, [chart], null]],
    );
    expect(screen.getByRole("status")).toHaveClass("is-success");
  });

  it("is dropped when an area is removed, but not when one is selected", async () => {
    await finishWith(
      {
        succeeded: 4,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
      [[2, [chart], null]],
    );
    pickPage(2);
    await flush();
    fireEvent.click(screen.getByTestId("crop-area"));
    expect(screen.getByRole("status")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "p.2 のグラフを消す" }));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("is said in English with the singular and the plural", async () => {
    await finishWith(
      {
        succeeded: 3,
        failed: 1,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
      [
        [2, [chart], null],
        [3, [], TIMEOUT],
      ],
      "en",
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "Searched all pages for suggestions: added 1 area · 1 page failed",
    );
  });
});
