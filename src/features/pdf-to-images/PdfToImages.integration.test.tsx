import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "../../App";
import {
  JOB_FINISHED_EVENT,
  JOB_ITEM_EVENT,
  JOB_PROGRESS_EVENT,
  type AddResult,
  type CheckPageRangeResult,
  type JobFinishedPayload,
  type JobItemPayload,
  type JobProgressPayload,
  type OutputDirLabel,
  type PdfItem,
  type Skipped,
} from "../../ipc";

const NO_SKIPPED: Skipped = {
  unsupported: 0,
  folders: 0,
  duplicates: 0,
};

interface CommandCall {
  cmd: string;
  args: unknown;
}

const SAMPLE_PDF_1: PdfItem = {
  id: 1,
  name: "sample1.pdf",
  pageCount: 3,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 1048576,
  error: null,
};

const SAMPLE_PDF_2: PdfItem = {
  id: 2,
  name: "sample2.pdf",
  pageCount: 5,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 2097152,
  error: null,
};

describe("PdfToImages integration workflow", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  function setupIpc(customHandlers?: (cmd: string, args: unknown) => unknown) {
    const calls: CommandCall[] = [];
    mockIPC(
      (cmd, args) => {
        calls.push({ cmd, args });
        if (customHandlers) {
          const res = customHandlers(cmd, args);
          if (res !== undefined) {
            return res;
          }
        }
        if (cmd === "get_about") {
          return {
            version: "0.1.0",
            pdfiumVersion: "chromium/8076",
            pdfiumReady: true,
            pdfiumError: null,
          };
        }
        if (cmd === "render_thumbnail") {
          return [137, 80, 78, 71];
        }
        return undefined;
      },
      { shouldMockEvents: true },
    );
    return calls;
  }

  it("handles full lifecycle: single PDF, page range checking, settings, conversion, progress and summary", async () => {
    let nextAddResult: AddResult<PdfItem> = {
      added: [SAMPLE_PDF_1],
      skipped: NO_SKIPPED,
    };

    const calls = setupIpc((cmd, args) => {
      if (cmd === "add_pdfs") {
        return nextAddResult;
      }
      if (
        cmd === "check_page_range" &&
        typeof args === "object" &&
        args !== null &&
        "text" in args &&
        args.text === "1-2"
      ) {
        const res: CheckPageRangeResult = {
          totalPages: 2,
          intervals: [[1, 2]],
        };
        return res;
      }
      if (cmd === "pick_output_dir") {
        const res: OutputDirLabel = { dirLabel: "converted_images" };
        return res;
      }
      if (cmd === "start_pdfs_to_images") {
        return undefined;
      }
      return undefined;
    });

    render(<App initialNavLang="ja" />);

    // Switch tab to PDF → 画像
    const pdfsTab = screen.getByRole("tab", { name: "PDF → 画像" });
    fireEvent.click(pdfsTab);

    // Initial state: DropZone shown, "変換を開始" disabled
    expect(screen.getByText("PDF を追加")).toBeInTheDocument();
    const startButton = screen.getByRole("button", { name: "変換を開始" });
    expect(startButton).toBeDisabled();

    // Click "PDF を追加" to add 1 PDF
    const addButtons = screen.getAllByRole("button", { name: "PDF を追加" });
    fireEvent.click(addButtons[0]);

    // Single PDF view appears
    expect(await screen.findByText("sample1.pdf")).toBeInTheDocument();
    expect(screen.getByText("3 ページ · A4 · 1.0 MB")).toBeInTheDocument();

    // 3 thumbnail cards are rendered for 3 pages
    const thumb1 = screen.getByTestId("page-thumbnail-1");
    const thumb2 = screen.getByTestId("page-thumbnail-2");
    const thumb3 = screen.getByTestId("page-thumbnail-3");
    expect(thumb1).toBeInTheDocument();
    expect(thumb2).toBeInTheDocument();
    expect(thumb3).toBeInTheDocument();

    // In "all" mode, all pages are highlighted
    expect(thumb1.querySelector(".pdf-page-container")).toHaveClass(
      "is-highlighted",
    );
    expect(thumb2.querySelector(".pdf-page-container")).toHaveClass(
      "is-highlighted",
    );
    expect(thumb3.querySelector(".pdf-page-container")).toHaveClass(
      "is-highlighted",
    );

    // Switch page mode to "範囲を指定"
    const rangeModeButton = screen.getByRole("button", { name: "範囲を指定" });
    fireEvent.click(rangeModeButton);

    // Input range "1-2"
    const rangeInput = screen.getByLabelText(/変換するページ/);
    fireEvent.change(rangeInput, { target: { value: "1-2" } });

    // check_page_range is called, and page count appears
    expect(await screen.findByText("2 ページを変換します")).toBeInTheDocument();
    expect(thumb1.querySelector(".pdf-page-container")).toHaveClass(
      "is-highlighted",
    );
    expect(thumb2.querySelector(".pdf-page-container")).toHaveClass(
      "is-highlighted",
    );
    expect(thumb3.querySelector(".pdf-page-container")).not.toHaveClass(
      "is-highlighted",
    );

    // Change resolution to 300 dpi
    const dpiSelect = screen.getByLabelText("解像度");
    fireEvent.change(dpiSelect, { target: { value: "300" } });
    expect(dpiSelect).toHaveValue("300");

    // "変換を開始" is still disabled because output directory is not selected
    expect(startButton).toBeDisabled();

    // Pick output directory
    const pickDirButton = screen.getByRole("button", {
      name: "フォルダを選ぶ",
    });
    fireEvent.click(pickDirButton);

    // Output dir is set and "変換を開始" is now enabled!
    await waitFor(() => {
      expect(screen.getByTestId("output-dir-name")).toHaveTextContent(
        "converted_images",
      );
    });
    expect(startButton).toBeEnabled();

    // Click "変換を開始"
    fireEvent.click(startButton);

    // Verify start_pdfs_to_images command called with expected ids, range, format, dpi
    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_pdfs_to_images",
        args: {
          ids: [1],
          range: "1-2",
          format: "png",
          dpi: 300,
        },
      });
    });

    // During active job, settings and drop buttons are disabled
    expect(rangeInput).toBeDisabled();
    expect(pickDirButton).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "キャンセル" }),
    ).toBeInTheDocument();

    // Emit progress event
    const progress: JobProgressPayload = {
      done: 0,
      total: 2,
      current: "sample1.pdf",
    };
    await act(() => emit(JOB_PROGRESS_EVENT, progress));
    expect(screen.getByText("変換中：sample1.pdf")).toBeInTheDocument();

    // Emit item completion
    const itemPayload: JobItemPayload = {
      id: 1,
      status: "ok",
      outputs: ["sample1_p1.png", "sample1_p2.png"],
    };
    await act(() => emit(JOB_ITEM_EVENT, itemPayload));

    // Emit finished event
    const finishedPayload: JobFinishedPayload = {
      succeeded: 1,
      failed: 0,
      noPages: 0,
      unprocessed: 0,
      cancelled: false,
    };
    await act(() => emit(JOB_FINISHED_EVENT, finishedPayload));

    // Summary banner appears on top of the list
    expect(
      await screen.findByText("変換が終わりました：成功 1 件"),
    ).toBeInTheDocument();

    // Add a second PDF to switch to batch view table
    nextAddResult = {
      added: [SAMPLE_PDF_2],
      skipped: NO_SKIPPED,
    };
    const addPdfHeaderBtn = screen.getByRole("button", { name: "PDF を追加" });
    fireEvent.click(addPdfHeaderBtn);

    // Multi-PDF table view is now active
    expect(await screen.findByText("PDF 2 件")).toBeInTheDocument();
    expect(screen.getByRole("table")).toBeInTheDocument();

    const rows = screen.getAllByRole("row");
    // Header row + 2 data rows
    expect(rows).toHaveLength(3);
    expect(within(rows[1]).getByText("sample1.pdf")).toBeInTheDocument();
    expect(within(rows[2]).getByText("sample2.pdf")).toBeInTheDocument();

    // Test removing all items
    const removeAllButton = screen.getByRole("button", { name: "すべて外す" });
    fireEvent.click(removeAllButton);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [1, 2] },
      });
    });

    // View returns to empty DropZone
    expect(await screen.findByText("PDF を追加")).toBeInTheDocument();
  });

  it("batch view displays partial failure details for failed pages", async () => {
    setupIpc((cmd) => {
      if (cmd === "add_pdfs") {
        return {
          added: [SAMPLE_PDF_1, SAMPLE_PDF_2],
          skipped: NO_SKIPPED,
        };
      }
      if (cmd === "pick_output_dir") {
        return { dirLabel: "out" };
      }
      if (cmd === "start_pdfs_to_images") {
        return undefined;
      }
      return undefined;
    });

    render(<App initialNavLang="ja" />);
    fireEvent.click(screen.getByRole("tab", { name: "PDF → 画像" }));

    // Add 2 PDFs
    fireEvent.click(screen.getAllByRole("button", { name: "PDF を追加" })[0]);
    expect(await screen.findByRole("table")).toBeInTheDocument();

    // Pick output directory
    fireEvent.click(screen.getByRole("button", { name: "フォルダを選ぶ" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "変換を開始" })).toBeEnabled();
    });

    // Start conversion
    fireEvent.click(screen.getByRole("button", { name: "変換を開始" }));

    // Emit partial failure on item 1: failed on page 2
    const item1: JobItemPayload = {
      id: 1,
      status: "partial",
      outputs: ["sample1_p1.png"],
      failedPages: [2],
    };
    await act(() => emit(JOB_ITEM_EVENT, item1));

    // Emit finished
    const finished: JobFinishedPayload = {
      succeeded: 0,
      failed: 1,
      noPages: 0,
      unprocessed: 1,
      cancelled: true,
    };
    await act(() => emit(JOB_FINISHED_EVENT, finished));

    // Summary and fail hint banner appear
    expect(
      await screen.findByText(
        "失敗した PDF の理由は、下の一覧に表示しています",
      ),
    ).toBeInTheDocument();

    // Check row status: 一部失敗 and reason 失敗したページ：2
    expect(screen.getByText("✕ 一部失敗")).toBeInTheDocument();
    // The summary lists it as a partial failure, not a failure.
    expect(screen.getByRole("status")).toHaveTextContent(
      "キャンセルしました：成功 0 件 · 一部失敗 1 件 · 未処理 1 件",
    );
    expect(screen.getByText("失敗したページ：2")).toBeInTheDocument();
  });
});
