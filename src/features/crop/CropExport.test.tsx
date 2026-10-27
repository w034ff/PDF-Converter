import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  areaRows,
  callsOf,
  candidate,
  emitJobFinished,
  emitJobItem,
  emitJobProgress,
  drawWholePage,
  expectPageShown,
  toggleCandidates,
  viewMenuButton,
  flush,
  installFakeObjectUrls,
  installFakeResizeObserver,
  openPdfInCropTab,
  openSettings,
  pageImage,
  rowPage,
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

const OUTPUT_DIR = { dirLabel: "figures" };

function pickPage(page: number) {
  fireEvent.click(screen.getByRole("button", { name: `ページ ${page}` }));
}

async function addWholePage() {
  await drawWholePage();
}

/** Opens a PDF and adds the whole of pages 3, 1 and 2, which get keys 1, 2 and 3. */
async function withThreeAreas(
  handlers: Record<string, Handler> = {},
): Promise<Call[]> {
  const calls = await openPdfInCropTab({
    pick_output_dir: () => OUTPUT_DIR,
    ...handlers,
  });
  for (const page of [3, 1, 2]) {
    pickPage(page);
    await flush();
    await addWholePage();
  }
  openSettings();
  return calls;
}

function exportButton(): HTMLElement {
  return screen.getByRole("button", { name: "書き出す" });
}

async function startExport() {
  fireEvent.click(exportButton());
  await flush();
}

function statusOf(page: number): string {
  const row = areaRows().find((r) => rowPage(r) === `p.${page}`);
  return row === undefined ? "" : rowText(row);
}

function outputDirText(): string | null {
  return screen.getByTestId("output-dir-name").textContent;
}

describe("the button that writes", () => {
  it("cannot be pressed with no PDF", async () => {
    await openPdfInCropTab();
    expect(exportButton()).toBeDisabled();
  });

  it("cannot be pressed with no area, nor with none checked", async () => {
    await openPdfInCropTab();
    expect(exportButton()).toBeDisabled();
    await addWholePage();
    expect(exportButton()).toBeEnabled();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "p.1 の手で選んだ範囲を書き出す" }),
    );
    expect(exportButton()).toBeDisabled();
  });

  it("cannot be pressed while the resolution typed is wrong", async () => {
    await openPdfInCropTab();
    await addWholePage();
    openSettings();
    fireEvent.change(screen.getByLabelText("解像度"), {
      target: { value: "custom" },
    });
    fireEvent.change(screen.getByLabelText("解像度（dpi）"), {
      target: { value: "5" },
    });
    expect(exportButton()).toBeDisabled();
    fireEvent.change(screen.getByLabelText("解像度（dpi）"), {
      target: { value: "200" },
    });
    expect(exportButton()).toBeEnabled();
  });

  it("cannot be pressed while a conversion runs, and gives way to Cancel", async () => {
    await withThreeAreas();
    await startExport();
    expect(screen.queryByRole("button", { name: "書き出す" })).toBeNull();
    expect(screen.getByRole("button", { name: "キャンセル" })).toBeEnabled();
  });
});

describe("starting a write", () => {
  it("asks for the folder first, and keeps it for the crop tab only", async () => {
    const calls = await withThreeAreas();
    expect(outputDirText()).toBe("未選択");

    await startExport();

    expect(callsOf(calls, "pick_output_dir").map((c) => c.args)).toEqual([
      { kind: "crop" },
    ]);
    expect(outputDirText()).toBe("figures");
    expect(callsOf(calls, "start_crop_export")).toHaveLength(1);

    fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));
    expect(screen.getByTestId("output-dir-name")).toHaveTextContent("未選択");
  });

  it("starts nothing when the folder dialog is cancelled", async () => {
    const calls = await withThreeAreas({ pick_output_dir: () => null });
    await startExport();
    expect(callsOf(calls, "start_crop_export")).toHaveLength(0);
    expect(exportButton()).toBeEnabled();
  });

  it("hands over the checked areas in the order of the list, with the settings", async () => {
    const calls = await withThreeAreas();
    fireEvent.click(
      screen.getByRole("checkbox", { name: "p.2 の手で選んだ範囲を書き出す" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "WebP" }));
    fireEvent.click(screen.getByRole("button", { name: "グレースケール" }));
    fireEvent.click(
      screen.getByRole("checkbox", { name: "写真だけを書き出す" }),
    );
    await startExport();

    const [call] = callsOf(calls, "start_crop_export");
    expect(call.args).toEqual({
      id: 7,
      regions: [
        { key: 2, page: 1, rectPt: [0, 0, 595, 842], imageIndex: null },
        { key: 1, page: 3, rectPt: [0, 0, 595, 842], imageIndex: null },
      ],
      format: "webp",
      colorMode: "gray",
      dpi: 150,
      photoNative: true,
      photoOnly: true,
    });
  });

  it("hands over the image of a photo", async () => {
    const calls = await openPdfInCropTab({
      pick_output_dir: () => OUTPUT_DIR,
      list_page_images: () => [pageImage([100, 200, 300, 400], 4)],
      detect_layout: () => [candidate("chart", [10, 10, 60, 60])],
    });
    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    fireEvent.click(screen.getByTestId("crop-candidate"));
    await startExport();

    const [call] = callsOf(calls, "start_crop_export");
    expect(call.args.regions).toEqual([
      { key: 1, page: 1, rectPt: [100, 200, 300, 400], imageIndex: 4 },
      { key: 2, page: 1, rectPt: [10, 10, 60, 60], imageIndex: null },
    ]);
  });

  it("does not ask for the folder again once it has one", async () => {
    const calls = await withThreeAreas();
    await startExport();
    await emitJobFinished({
      succeeded: 3,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    await startExport();
    expect(callsOf(calls, "pick_output_dir")).toHaveLength(1);
    expect(callsOf(calls, "start_crop_export")).toHaveLength(2);
  });

  it("forgets a folder Rust has forgotten, in the crop tab only", async () => {
    const calls = await withThreeAreas({
      start_crop_export: () => {
        throw { code: "OutputDirMissing", detail: null };
      },
    });
    await startExport();
    expect(callsOf(calls, "start_crop_export")).toHaveLength(1);
    expect(outputDirText()).toBe("未選択");
  });

  it("shows an error a refused start leaves", async () => {
    await withThreeAreas({
      start_crop_export: () => {
        throw { code: "ConversionRunning", detail: null };
      },
    });
    await startExport();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(exportButton()).toBeEnabled();
  });
});

describe("the rows while and after a write", () => {
  async function started() {
    const calls = await withThreeAreas();
    await startExport();
    return calls;
  }

  it("read waiting for every row of the write, and do not point at the one being written", async () => {
    await started();
    expect(statusOf(1)).toContain("待機");
    expect(statusOf(2)).toContain("待機");
    expect(statusOf(3)).toContain("待機");

    await emitJobProgress({ done: 0, total: 3, current: "report.pdf" });
    expect(areaRows().every((row) => rowText(row).includes("待機"))).toBe(true);
    expect(areaRows().some((row) => rowText(row).includes("変換中"))).toBe(
      false,
    );
  });

  it("say exported for a row that succeeded, and the reason for one that failed", async () => {
    await started();
    // The keys are 2, 3 and 1 for pages 1, 2 and 3.
    await emitJobItem({ id: 2, status: "ok", outputs: ["report_p1.png"] });
    await emitJobItem({
      id: 3,
      status: "failed",
      outputs: [],
      error: { code: "RenderTooLarge", detail: null },
    });

    expect(statusOf(1)).toContain("✓ 書き出し済み");
    expect(statusOf(2)).toContain("✕ 失敗");
    expect(statusOf(2)).toContain(
      "この解像度では大きすぎて画像にできません。解像度を下げてください",
    );
    expect(statusOf(3)).toContain("待機");
  });

  it("say not processed for the rows a finished write did not reach, and summarize by items", async () => {
    await started();
    await emitJobItem({ id: 2, status: "ok", outputs: ["a.png"] });
    await emitJobItem({
      id: 3,
      status: "failed",
      outputs: [],
      error: { code: "RenderTooLarge", detail: null },
    });
    await emitJobFinished({
      succeeded: 1,
      failed: 1,
      noPages: 0,
      unprocessed: 1,
      cancelled: false,
    });

    expect(statusOf(3)).toContain("未処理");
    const banner = screen.getByRole("status");
    expect(banner).toHaveTextContent(
      "変換が終わりました：成功 1 件 · 失敗 1 件 · 未処理 1 件",
    );
    expect(banner).toHaveClass("error-text");
  });

  it("tell the summary in the bottom bar, not above the list", async () => {
    await started();
    await emitJobFinished({
      succeeded: 3,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    const side = screen.getByRole("region", { name: "範囲と書き出しの設定" });
    expect(within(side).queryByRole("status")).toBeNull();
    const summary = screen.getByRole("status");
    expect(summary.closest("footer")).not.toBeNull();
    expect(summary).toHaveTextContent("変換が終わりました：成功 3 件");
    expect(summary).toHaveClass("success-text");
  });

  it("say cancelled for the row a cancel stopped, and how the cancel is summarized", async () => {
    await started();
    await emitJobItem({ id: 2, status: "ok", outputs: ["a.png"] });
    await emitJobItem({ id: 3, status: "cancelled", outputs: [] });
    await emitJobFinished({
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 1,
      cancelled: true,
    });
    expect(statusOf(2)).toContain("キャンセル");
    expect(statusOf(3)).toContain("未処理");
    expect(screen.getByRole("status")).toHaveTextContent(
      "キャンセルしました：成功 1 件 · 未処理 1 件",
    );
  });

  it("keep saying exported after the summary is dropped by an edit", async () => {
    await started();
    await emitJobItem({ id: 2, status: "ok", outputs: ["a.png"] });
    await emitJobItem({ id: 3, status: "ok", outputs: ["b.png"] });
    await emitJobItem({ id: 1, status: "ok", outputs: ["c.png"] });
    await emitJobFinished({
      succeeded: 3,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    fireEvent.click(
      screen.getByRole("checkbox", { name: "p.1 の手で選んだ範囲を書き出す" }),
    );

    expect(screen.queryByRole("status")).toBeNull();
    expect(statusOf(1)).toContain("✓ 書き出し済み");
    expect(statusOf(2)).toContain("✓ 書き出し済み");
    expect(statusOf(3)).toContain("✓ 書き出し済み");
  });

  it("show a row written before as exported while a later write has other rows", async () => {
    await started();
    await emitJobItem({ id: 2, status: "ok", outputs: ["a.png"] });
    await emitJobFinished({
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 2,
      cancelled: true,
    });
    fireEvent.click(
      screen.getByRole("checkbox", { name: "p.1 の手で選んだ範囲を書き出す" }),
    );
    await startExport();

    expect(statusOf(1)).toContain("✓ 書き出し済み");
    expect(statusOf(2)).toContain("待機");
  });

  it("are cleared of a failure when a new write starts", async () => {
    await started();
    await emitJobItem({
      id: 2,
      status: "failed",
      outputs: [],
      error: { code: "RenderTooLarge", detail: null },
    });
    await emitJobFinished({
      succeeded: 0,
      failed: 1,
      noPages: 0,
      unprocessed: 2,
      cancelled: false,
    });
    await startExport();
    expect(statusOf(1)).not.toContain("失敗");
    expect(statusOf(1)).toContain("待機");
  });
});

describe("what a running conversion locks", () => {
  async function running() {
    const calls = await openPdfInCropTab({
      pick_output_dir: () => OUTPUT_DIR,
      list_page_images: () => [pageImage([100, 200, 300, 400], 2)],
      detect_layout: () => [candidate("chart", [10, 10, 60, 60])],
    });
    await addWholePage();
    openSettings();
    await startExport();
    return calls;
  }

  it("keeps an area from being added, removed or checked", async () => {
    await running();
    const rows = areaRows().length;

    fireEvent.click(screen.getByTestId("crop-photo-frame"));
    fireEvent.click(screen.getByTestId("crop-candidate"));
    expect(areaRows()).toHaveLength(rows);
    expect(
      screen.getByRole("checkbox", { name: "p.1 の手で選んだ範囲を書き出す" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "p.1 の手で選んだ範囲を消す" }),
    ).toBeDisabled();
    expect(screen.getByRole("button", { name: "範囲を追加" })).toBeDisabled();
  });

  it("keeps the settings and the opening of another PDF from being changed", async () => {
    await running();
    expect(screen.getByLabelText("解像度")).toBeDisabled();
    expect(screen.getByRole("button", { name: "WebP" })).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "写真は元の画質で書き出す" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("checkbox", { name: "写真だけを書き出す" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "別の PDF を開く" }),
    ).toBeDisabled();
  });

  it("leaves the pages, the view, the candidates and the selection free", async () => {
    await running();
    expect(screen.getByRole("button", { name: "ページ 2" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "ページ 2" }));
    expectPageShown(2, 4);
    fireEvent.click(
      screen.getByRole("button", { name: "ページ 1、範囲 1 個" }),
    );

    expect(screen.getByRole("button", { name: "拡大" })).toBeEnabled();
    expect(viewMenuButton()).toBeEnabled();
    toggleCandidates();
    expect(screen.queryAllByTestId("crop-candidate")).toHaveLength(0);

    fireEvent.click(screen.getByTestId("crop-area"));
    expect(screen.getByTestId("crop-area")).toHaveAttribute(
      "data-selected",
      "true",
    );
  });

  it("lets a row be pressed to show its page", async () => {
    await running();
    pickPage(3);
    await flush();
    fireEvent.click(
      screen.getByRole("button", { name: "p.1 の手で選んだ範囲を表示" }),
    );
    expectPageShown(1, 4);
  });

  it("lets the settings be opened and closed", async () => {
    await running();
    const toggle = screen.getByRole("button", { name: /書き出しの設定/ });
    expect(toggle).toBeEnabled();
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("frees everything once the write has finished", async () => {
    await running();
    await emitJobFinished({
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    });
    expect(screen.getByRole("button", { name: "範囲を追加" })).toBeEnabled();
    expect(
      screen.getByRole("button", { name: "別の PDF を開く" }),
    ).toBeEnabled();
    expect(screen.getByLabelText("解像度")).toBeEnabled();
  });
});
