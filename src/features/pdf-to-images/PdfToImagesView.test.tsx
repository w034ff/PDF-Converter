import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type {
  AddResult,
  CheckPageRangeResult,
  JobFinishedPayload,
  PdfItem,
} from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  type AppState,
} from "../../state";
import { PdfToImagesSettings } from "./PdfToImagesSettings";
import { PdfToImagesView } from "./PdfToImagesView";

const samplePdf1: PdfItem = {
  id: 1,
  name: "annual-report.pdf",
  pageCount: 4,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 2000000,
  error: null,
};

const samplePdf10Pages: PdfItem = {
  id: 1,
  name: "annual-report.pdf",
  pageCount: 10,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 2000000,
  error: null,
};

const samplePdf2: PdfItem = {
  id: 2,
  name: "invoice.pdf",
  pageCount: 3,
  firstPageSizePt: { widthPt: 612, heightPt: 792 },
  bytes: 500000,
  error: null,
};

const samplePdfB10Pages: PdfItem = {
  id: 4,
  name: "catalog.pdf",
  pageCount: 10,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 2500000,
  error: null,
};

const errorPdf: PdfItem = {
  id: 3,
  name: "locked.pdf",
  pageCount: 0,
  firstPageSizePt: null,
  bytes: 100000,
  error: {
    code: "PasswordProtected",
    detail: null,
  },
};

interface Call {
  cmd: string;
  args: unknown;
}

function mockCommands(answer: (cmd: string, args: unknown) => unknown): Call[] {
  const calls: Call[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      return answer(cmd, args);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

function renderView(stateModifier?: (state: AppState) => void) {
  const base = createInitialAppState("ja-JP");
  const state: AppState = {
    ...base,
    job: { ...base.job, results: { ...base.job.results } },
    pdfToImages: { ...base.pdfToImages, items: [...base.pdfToImages.items] },
  };
  state.language.activeTab = "pdfToImages";
  stateModifier?.(state);
  return render(
    <AppStateProvider initialState={state}>
      <PdfToImagesView />
    </AppStateProvider>,
  );
}

function renderFeature(stateModifier?: (state: AppState) => void) {
  const base = createInitialAppState("ja-JP");
  const state: AppState = {
    ...base,
    job: { ...base.job, results: { ...base.job.results } },
    pdfToImages: { ...base.pdfToImages, items: [...base.pdfToImages.items] },
  };
  state.language.activeTab = "pdfToImages";
  stateModifier?.(state);
  return render(
    <AppStateProvider initialState={state}>
      <PdfToImagesSettings />
      <PdfToImagesView />
    </AppStateProvider>,
  );
}

function parseRangeToIntervals(text: string): CheckPageRangeResult {
  const intervals: Array<[number, number]> = [];
  if (text.trim().length > 0) {
    const parts = text.split(",").map((s) => s.trim());
    for (const part of parts) {
      if (part.includes("-")) {
        const [s, e] = part.split("-").map((n) => parseInt(n, 10));
        if (!isNaN(s) && !isNaN(e)) {
          intervals.push([s, e]);
        }
      } else {
        const n = parseInt(part, 10);
        if (!isNaN(n)) {
          intervals.push([n, n]);
        }
      }
    }
  }
  let totalPages = 0;
  for (const [s, e] of intervals) {
    totalPages += e - s + 1;
  }
  return { totalPages, intervals };
}

function isTextArgs(args: unknown): args is { text: string } {
  return (
    typeof args === "object" &&
    args !== null &&
    "text" in args &&
    typeof Reflect.get(args, "text") === "string"
  );
}

describe("PdfToImagesView", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  describe("empty state", () => {
    it("renders DropZone and calls add_pdfs for files and folder", async () => {
      const calls = mockCommands((cmd) => {
        if (cmd === "add_pdfs") {
          const res: AddResult<PdfItem> = {
            added: [samplePdf1],
            skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          };
          return res;
        }
        return undefined;
      });

      renderView();

      expect(screen.getByTestId("drop-zone")).toBeInTheDocument();

      // Click "PDF を追加"
      const addFilesBtn = screen.getByRole("button", { name: "PDF を追加" });
      fireEvent.click(addFilesBtn);

      await waitFor(() => {
        expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();
      });

      expect(calls).toContainEqual({
        cmd: "add_pdfs",
        args: { source: "files" },
      });
    });

    it("calls add_pdfs with folder when add folder is clicked", async () => {
      const calls = mockCommands((cmd) => {
        if (cmd === "add_pdfs") {
          const res: AddResult<PdfItem> = {
            added: [samplePdf1],
            skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          };
          return res;
        }
        return undefined;
      });

      renderView();
      const addFolderBtn = screen.getByRole("button", {
        name: "フォルダを追加",
      });
      fireEvent.click(addFolderBtn);

      await waitFor(() => {
        expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();
      });

      expect(calls).toContainEqual({
        cmd: "add_pdfs",
        args: { source: "folder" },
      });
    });

    it("displays error when add_pdfs rejects with an error", async () => {
      mockCommands((cmd) => {
        if (cmd === "add_pdfs") {
          throw { code: "ReadFailed", detail: "broken.pdf" };
        }
        return undefined;
      });

      renderView();
      const addFilesBtn = screen.getByRole("button", { name: "PDF を追加" });
      fireEvent.click(addFilesBtn);

      expect(await screen.findByRole("alert")).toBeInTheDocument();
      expect(
        screen.getByText("ファイルの読み込みに失敗しました"),
      ).toBeInTheDocument();
    });
  });

  describe("single PDF mode (PdfSingle)", () => {
    it("renders file header meta and thumbnail figures for all pages", async () => {
      mockCommands((cmd) =>
        cmd === "get_thumbnail" ? new Uint8Array() : undefined,
      );
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
      });

      expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();
      expect(screen.getByText(/4 ページ · A4 · 1.9 MB/)).toBeInTheDocument();

      await waitFor(() => {
        expect(screen.getAllByRole("img")).toHaveLength(4);
      });
    });

    it("highlights all pages when pageSelection is 'all'", async () => {
      mockCommands((cmd) =>
        cmd === "get_thumbnail" ? new Uint8Array() : undefined,
      );
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "all";
      });

      await waitFor(() => {
        expect(screen.getAllByRole("img")).toHaveLength(4);
      });

      for (let p = 1; p <= 4; p++) {
        expect(
          screen.getByText(new RegExp(`${p}\\s+✓ 変換する`)),
        ).toBeInTheDocument();
        const fig = screen.getByTestId(`page-thumbnail-${p}`);
        expect(fig).not.toHaveClass("is-dimmed");
      }
    });

    it("highlights only pages in intervals when pageSelection is 'range'", async () => {
      mockCommands((cmd) =>
        cmd === "get_thumbnail" ? new Uint8Array() : undefined,
      );
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeResult = {
          totalPages: 2,
          intervals: [
            [1, 1],
            [3, 3],
          ],
        };
      });

      await waitFor(() => {
        expect(screen.getAllByRole("img")).toHaveLength(4);
      });

      // Pages 1 and 3 are highlighted
      expect(
        screen.getByText(new RegExp("1\\s+✓ 変換する")),
      ).toBeInTheDocument();
      expect(
        screen.getByText(new RegExp("3\\s+✓ 変換する")),
      ).toBeInTheDocument();
      expect(screen.getByTestId("page-thumbnail-1")).not.toHaveClass(
        "is-dimmed",
      );
      expect(screen.getByTestId("page-thumbnail-3")).not.toHaveClass(
        "is-dimmed",
      );

      // Pages 2 and 4 are dimmed
      expect(screen.getByTestId("page-thumbnail-2")).toHaveClass("is-dimmed");
      expect(screen.getByTestId("page-thumbnail-4")).toHaveClass("is-dimmed");
    });

    it("calls remove_items and clears item on '外す'", async () => {
      const calls = mockCommands((cmd) =>
        cmd === "remove_items" ? undefined : undefined,
      );
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
      });

      const removeBtn = screen.getByRole("button", { name: "外す" });
      fireEvent.click(removeBtn);

      await waitFor(() => {
        expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
      });

      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [1] },
      });
    });

    it("does not render 'すべて外す' when exactly one PDF is in the list, but renders it when two or more PDFs are in the list", () => {
      mockCommands(() => undefined);

      // 1 PDF: only '外す' is rendered, 'すべて外す' is not
      const { unmount } = renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
      });
      expect(
        screen.queryByRole("button", { name: "すべて外す" }),
      ).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "外す" })).toBeInTheDocument();
      unmount();

      // 2 PDFs: 'すべて外す' is rendered in header
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1, samplePdf2];
      });
      expect(
        screen.getByRole("button", { name: "すべて外す" }),
      ).toBeInTheDocument();
    });

    it("keeps item in list and displays error when remove_items fails on '外す'", async () => {
      mockCommands((cmd) => {
        if (cmd === "remove_items") {
          throw { code: "InvalidParams", detail: "failed to remove" };
        }
        return undefined;
      });

      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
      });

      expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();

      const removeBtn = screen.getByRole("button", { name: "外す" });
      fireEvent.click(removeBtn);

      expect(await screen.findByRole("alert")).toBeInTheDocument();
      expect(screen.getByText("無効な設定です")).toBeInTheDocument();
      expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();
    });

    it("keeps items in list and displays error when remove_items fails on 'すべて外す' in batch view", async () => {
      mockCommands((cmd) => {
        if (cmd === "remove_items") {
          throw { code: "InvalidParams", detail: "cannot clear" };
        }
        return undefined;
      });

      renderView((state) => {
        state.pdfToImages.items = [samplePdf1, samplePdf2];
      });

      const clearAllBtn = screen.getByRole("button", { name: "すべて外す" });
      fireEvent.click(clearAllBtn);

      expect(await screen.findByRole("alert")).toBeInTheDocument();
      expect(screen.getByText("無効な設定です")).toBeInTheDocument();
      expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();
      expect(screen.getByText("invoice.pdf")).toBeInTheDocument();
    });

    it("displays error reason when single PDF has an error", () => {
      mockCommands(() => undefined);
      renderView((state) => {
        state.pdfToImages.items = [errorPdf];
      });

      expect(screen.getByText("locked.pdf")).toBeInTheDocument();
      expect(
        screen.getAllByText("パスワードで保護されているため開けません").length,
      ).toBeGreaterThanOrEqual(1);
    });
  });

  describe("multiple PDFs mode (PdfBatch)", () => {
    it("renders table with columns and items", () => {
      mockCommands(() => undefined);
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1, samplePdf2];
      });

      expect(screen.getByText("PDF 2 件")).toBeInTheDocument();
      expect(screen.getByRole("table")).toBeInTheDocument();
      expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();
      expect(screen.getByText("invoice.pdf")).toBeInTheDocument();
      expect(
        screen.getByRole("columnheader", { name: "ファイル名" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("columnheader", { name: "ページ / 全体" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("columnheader", { name: "保存したファイル" }),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("columnheader", { name: "状態" }),
      ).toBeInTheDocument();
    });

    it("displays various job row statuses with reasons and saved counts", () => {
      mockCommands(() => undefined);
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1, samplePdf2, errorPdf];
        state.job.kind = "pdfToImages";
        state.job.targets = [
          { id: 1, name: "annual-report.pdf" },
          { id: 2, name: "invoice.pdf" },
        ];
        state.job.results = {
          1: {
            id: 1,
            status: "partial",
            outputs: ["annual-report_p1.png"],
            failedPages: [2, 3],
          },
          2: {
            id: 2,
            status: "cancelled",
            outputs: ["invoice_p1.png", "invoice_p2.png"],
          },
        };
      });

      // Item 1: partial failure with failed pages
      expect(screen.getByText("✕ 一部失敗")).toBeInTheDocument();
      expect(screen.getByText("失敗したページ：2, 3")).toBeInTheDocument();

      // Item 2: cancelled with saved count
      expect(screen.getByText("キャンセル")).toBeInTheDocument();
      expect(screen.getByText("2 ページを保存済み")).toBeInTheDocument();

      // Item 3: initial error item excluded from targets
      expect(screen.getByText("✕ 失敗")).toBeInTheDocument();
      expect(
        screen.getByText("パスワードで保護されているため開けません"),
      ).toBeInTheDocument();
    });

    it("renders job summary banner above table on finish", () => {
      mockCommands(() => undefined);
      const finished: JobFinishedPayload = {
        succeeded: 1,
        failed: 1,
        noPages: 1,
        unprocessed: 0,
        cancelled: false,
      };

      renderView((state) => {
        state.pdfToImages.items = [samplePdf1, samplePdf2];
        state.job.kind = "pdfToImages";
        state.job.finished = finished;
      });

      expect(screen.getByRole("status")).toHaveTextContent(
        "変換が終わりました：成功 1 件 · 失敗 1 件 · 対象のページなし 1 件",
      );
      expect(
        screen.getByText("失敗した PDF の理由は、下の一覧に表示しています"),
      ).toBeInTheDocument();
    });

    it("does not show single-result when status is partial, but marks the failed page thumbnail with is-failed", () => {
      mockCommands(() => undefined);
      const finished: JobFinishedPayload = {
        succeeded: 0,
        failed: 1,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      };
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.job.phase = "finished";
        state.job.kind = "pdfToImages";
        state.job.targets = [{ id: 1, name: "annual-report.pdf" }];
        state.job.results = {
          1: {
            id: 1,
            status: "partial",
            outputs: [
              "annual-report_p1.png",
              "annual-report_p3.png",
              "annual-report_p4.png",
            ],
            failedPages: [2],
          },
        };
        state.job.finished = finished;
      });

      // Partial failure does not show single-result banner under header
      expect(screen.queryByTestId("single-result")).not.toBeInTheDocument();

      const thumb2 = screen.getByTestId("page-thumbnail-2");
      expect(thumb2.querySelector(".pdf-page-container")).toHaveClass(
        "is-failed",
      );
      expect(thumb2).toHaveTextContent("2 ✕ 失敗");
      expect(screen.getByTestId("page-thumbnail-1")).not.toHaveTextContent(
        "失敗",
      );
      // The mark tells a failed page from a selected one, whose border is a
      // similar colour; only the failed page has it.
      expect(thumb2.querySelector(".pdf-page-failed-badge")).not.toBeNull();
      expect(
        screen
          .getByTestId("page-thumbnail-1")
          .querySelector(".pdf-page-failed-badge"),
      ).toBeNull();

      // Top banner shows partial failure count
      expect(screen.getByRole("status")).toHaveTextContent(
        "変換が終わりました：成功 0 件 · 一部失敗 1 件",
      );
      expect(
        screen.queryByText("失敗した PDF の理由は、下の一覧に表示しています"),
      ).not.toBeInTheDocument();
    });

    it("shows single-result when status is ok, failed, or cancelled", () => {
      mockCommands(() => undefined);

      // OK status
      const okView = renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.job.phase = "finished";
        state.job.kind = "pdfToImages";
        state.job.targets = [{ id: 1, name: "annual-report.pdf" }];
        state.job.results = {
          1: {
            id: 1,
            status: "ok",
            outputs: ["annual-report_p1.png"],
          },
        };
        state.job.finished = {
          succeeded: 1,
          failed: 0,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        };
      });
      expect(screen.getByTestId("single-result")).toHaveTextContent("✓ 完了");
      okView.unmount();

      // Failed status
      const failedView = renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.job.phase = "finished";
        state.job.kind = "pdfToImages";
        state.job.targets = [{ id: 1, name: "annual-report.pdf" }];
        state.job.results = {
          1: {
            id: 1,
            status: "failed",
            outputs: [],
            error: { code: "RenderTooLarge", detail: null },
          },
        };
        state.job.finished = {
          succeeded: 0,
          failed: 1,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        };
      });
      const failedRes = screen.getByTestId("single-result");
      expect(failedRes).toHaveTextContent("✕ 失敗");
      expect(failedRes).toHaveTextContent(
        "この解像度では大きすぎて画像にできません",
      );
      failedView.unmount();

      // Cancelled status
      const cancelledView = renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.job.phase = "finished";
        state.job.kind = "pdfToImages";
        state.job.targets = [{ id: 1, name: "annual-report.pdf" }];
        state.job.results = {
          1: {
            id: 1,
            status: "cancelled",
            outputs: ["annual-report_p1.png"],
          },
        };
        state.job.finished = {
          succeeded: 0,
          failed: 0,
          noPages: 0,
          unprocessed: 0,
          cancelled: true,
        };
      });
      const cancelledRes = screen.getByTestId("single-result");
      expect(cancelledRes).toHaveTextContent("キャンセル");
      expect(cancelledRes).toHaveTextContent("1 ページを保存済み");
      cancelledView.unmount();
    });

    it("shows nothing under a single PDF before any conversion", () => {
      mockCommands(() => undefined);
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
      });

      expect(screen.queryByTestId("single-result")).not.toBeInTheDocument();
    });

    it("disables view action buttons during conversion", () => {
      mockCommands(() => undefined);
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1, samplePdf2];
        state.job.phase = "running";
      });

      expect(screen.getByRole("button", { name: "PDF を追加" })).toBeDisabled();
      expect(
        screen.getByRole("button", { name: "フォルダを追加" }),
      ).toBeDisabled();
      expect(screen.getByRole("button", { name: "すべて外す" })).toBeDisabled();

      const removeBtns = screen.getAllByRole("button", { name: "外す" });
      for (const btn of removeBtns) {
        expect(btn).toBeDisabled();
      }
    });

    it("keeps row in batch table and displays error when remove_items fails on row '外す'", async () => {
      mockCommands((cmd) => {
        if (cmd === "remove_items") {
          throw { code: "InvalidParams", detail: "cannot remove row" };
        }
        return undefined;
      });

      renderView((state) => {
        state.pdfToImages.items = [samplePdf1, samplePdf2];
      });

      const removeBtns = screen.getAllByRole("button", { name: "外す" });
      fireEvent.click(removeBtns[0]);

      expect(await screen.findByRole("alert")).toBeInTheDocument();
      expect(screen.getByText("無効な設定です")).toBeInTheDocument();
      expect(screen.getByText("annual-report.pdf")).toBeInTheDocument();
      expect(screen.getByText("invoice.pdf")).toBeInTheDocument();
    });
  });

  describe("thumbnail click page selection", () => {
    it("toggles page selection in range mode: rebuilds rangeText, calls check_page_range, and updates aria-pressed", async () => {
      const calls = mockCommands((cmd, args) => {
        if (cmd === "check_page_range" && isTextArgs(args)) {
          return parseRangeToIntervals(args.text);
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1]; // 4 pages
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "1";
        state.pdfToImages.rangeResult = { totalPages: 1, intervals: [[1, 1]] };
      });

      // Wait for initial check triggered on mount to finish so rangeChecking is false
      await waitFor(() => {
        expect(screen.getByText("1 ページを変換します")).toBeInTheDocument();
      });

      const thumb1Btn = screen.getByRole("button", { name: "1 ページ" });
      const thumb2Btn = screen.getByRole("button", { name: "2 ページ" });

      expect(thumb1Btn).toHaveAttribute("aria-pressed", "true");
      expect(thumb2Btn).toHaveAttribute("aria-pressed", "false");

      // Click unselected thumbnail (page 2) -> adds 2, text becomes "1-2"
      fireEvent.click(thumb2Btn);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1-2");
        expect(thumb2Btn).toHaveAttribute("aria-pressed", "true");
      });

      expect(calls).toContainEqual({
        cmd: "check_page_range",
        args: { text: "1-2", ids: [1] },
      });

      // Click selected thumbnail (page 1) -> removes 1, text becomes "2"
      fireEvent.click(thumb1Btn);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("2");
        expect(thumb1Btn).toHaveAttribute("aria-pressed", "false");
      });
    });

    it("creates single page text when clicking thumbnail while range text is empty", async () => {
      mockCommands((cmd, args) => {
        if (cmd === "check_page_range" && isTextArgs(args)) {
          return parseRangeToIntervals(args.text);
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "";
        state.pdfToImages.rangeResult = null;
      });

      const thumb3Btn = screen.getByRole("button", { name: "3 ページ" });
      fireEvent.click(thumb3Btn);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("3");
        expect(thumb3Btn).toHaveAttribute("aria-pressed", "true");
      });
    });

    it("switches from 'all' to 'range' and excludes only the clicked page", async () => {
      mockCommands((cmd, args) => {
        if (cmd === "check_page_range" && isTextArgs(args)) {
          return parseRangeToIntervals(args.text);
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf10Pages]; // 10 pages
        state.pdfToImages.pageSelection = "all";
      });

      // In 'all' mode, all pages have aria-pressed="true"
      const thumb4Btn = screen.getByRole("button", { name: "4 ページ" });
      expect(thumb4Btn).toHaveAttribute("aria-pressed", "true");

      // Clicking page 4 switches to 'range' mode with text "1-3, 5-10"
      fireEvent.click(thumb4Btn);

      await waitFor(() => {
        expect(
          screen.getByRole("button", { name: "範囲を指定" }),
        ).toHaveAttribute("aria-pressed", "true");
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue(
          "1-3, 5-10",
        );
        expect(thumb4Btn).toHaveAttribute("aria-pressed", "false");
      });
    });

    it("selects and deselects contiguous ranges when clicking with Shift", async () => {
      mockCommands((cmd, args) => {
        if (cmd === "check_page_range" && isTextArgs(args)) {
          return parseRangeToIntervals(args.text);
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf10Pages];
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "1";
        state.pdfToImages.rangeResult = { totalPages: 1, intervals: [[1, 1]] };
      });

      // Wait for initial check triggered on mount to finish so rangeChecking is false
      await waitFor(() => {
        expect(screen.getByText("1 ページを変換します")).toBeInTheDocument();
      });

      // Click page 3 without Shift (anchor becomes 3, adds 3 -> "1, 3")
      const thumb3 = screen.getByRole("button", { name: "3 ページ" });
      fireEvent.click(thumb3);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1, 3");
        expect(screen.getByText("2 ページを変換します")).toBeInTheDocument();
      });

      // Shift-click page 7 (target 7 is not selected -> selects 3..7 -> "1, 3-7")
      const thumb7 = screen.getByRole("button", { name: "7 ページ" });
      fireEvent.click(thumb7, { shiftKey: true });

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1, 3-7");
        expect(screen.getByText("6 ページを変換します")).toBeInTheDocument();
      });

      // Shift-click page 3 (target 3 is selected -> unselects 7..3 -> 3..7 excluded -> "1")
      fireEvent.click(thumb3, { shiftKey: true });

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1");
        expect(screen.getByText("1 ページを変換します")).toBeInTheDocument();
      });

      // Test anchor after target: Click page 7 (anchor becomes 7 -> "1, 7")
      fireEvent.click(thumb7);
      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1, 7");
        expect(screen.getByText("2 ページを変換します")).toBeInTheDocument();
      });

      // Shift-click page 3 (anchor 7 is after target 3; 3 is unselected -> selects 3..7 -> "1, 3-7")
      fireEvent.click(thumb3, { shiftKey: true });
      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1, 3-7");
      });
    });

    it("treats Shift-click without an anchor (first operation) as a single page click", async () => {
      mockCommands((cmd, args) => {
        if (cmd === "check_page_range" && isTextArgs(args)) {
          return parseRangeToIntervals(args.text);
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf10Pages]; // 10 pages
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "";
        state.pdfToImages.rangeResult = null;
      });

      // Shift-click page 5 as first operation: only page 5 is selected (not 1..5)
      const thumb5 = screen.getByRole("button", { name: "5 ページ" });
      fireEvent.click(thumb5, { shiftKey: true });

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("5");
        expect(thumb5).toHaveAttribute("aria-pressed", "true");
      });
    });

    it("resets anchor when PDF is replaced so first Shift-click on new PDF selects only that page", async () => {
      mockCommands((cmd, args) => {
        if (cmd === "check_page_range" && isTextArgs(args)) {
          return parseRangeToIntervals(args.text);
        }
        if (cmd === "remove_items") {
          return undefined;
        }
        if (cmd === "add_pdfs") {
          const res: AddResult<PdfItem> = {
            added: [samplePdfB10Pages],
            skipped: { unsupported: 0, folders: 0, duplicates: 0 },
          };
          return res;
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1]; // PDF A (4 pages)
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "";
        state.pdfToImages.rangeResult = null;
      });

      // 1. Click page 2 on PDF A (anchor becomes 2, rangeText becomes "2")
      const thumb2OnPdfA = screen.getByRole("button", { name: "2 ページ" });
      fireEvent.click(thumb2OnPdfA);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("2");
      });

      // Deselect page 2 so rangeText becomes empty, while anchor remains 2
      fireEvent.click(thumb2OnPdfA);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("");
      });

      // 2. Remove PDF A by clicking "外す"
      const removeBtn = screen.getByRole("button", { name: "外す" });
      fireEvent.click(removeBtn);

      await waitFor(() => {
        expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
      });

      // 3. Add PDF B (10 pages) via DropZone
      const addBtn = screen.getByRole("button", { name: "PDF を追加" });
      fireEvent.click(addBtn);

      await waitFor(() => {
        expect(screen.getByText("catalog.pdf")).toBeInTheDocument();
      });

      // 4. Shift-click page 6 on PDF B: anchor should be reset, so only page 6 is selected
      const thumb6OnPdfB = screen.getByRole("button", { name: "6 ページ" });
      fireEvent.click(thumb6OnPdfB, { shiftKey: true });

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("6");
        expect(thumb6OnPdfB).toHaveAttribute("aria-pressed", "true");
      });
    });

    it("disables thumbnail buttons and ignores clicks during job conversion and range errors", () => {
      mockCommands(() => undefined);

      // During active conversion: buttons disabled
      const runningFeature = renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.job.phase = "running";
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "1";
        state.pdfToImages.rangeResult = { totalPages: 1, intervals: [[1, 1]] };
      });
      const btnRunning = screen.getByRole("button", { name: "2 ページ" });
      expect(btnRunning).toBeDisabled();
      fireEvent.click(btnRunning);
      expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1");
      runningFeature.unmount();

      // When range error is present: buttons disabled
      const errorFeature = renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "abc";
        state.pdfToImages.rangeError = {
          code: "InvalidPageRange",
          detail: "abc",
        };
      });
      const btnError = screen.getByRole("button", { name: "2 ページ" });
      expect(btnError).toBeDisabled();
      fireEvent.click(btnError);
      expect(screen.getByLabelText(/変換するページ/)).toHaveValue("abc");
      errorFeature.unmount();
    });

    it("does not change range text when clicked while range check is in progress", () => {
      mockCommands(() => undefined);

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "1";
        state.pdfToImages.rangeChecking = true;
        state.pdfToImages.rangeResult = { totalPages: 1, intervals: [[1, 1]] };
      });

      const btn = screen.getByRole("button", { name: "2 ページ" });
      // Not disabled to prevent UI flicker
      expect(btn).not.toBeDisabled();

      // Click should be ignored
      fireEvent.click(btn);
      expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1");
    });

    it("rebuilds range text strictly from intervals returned by IPC without parsing range text on the client", async () => {
      // Mock returns intervals [2, 2] regardless of text "something-arbitrary"
      mockCommands((cmd) => {
        if (cmd === "check_page_range") {
          return { totalPages: 1, intervals: [[2, 2]] };
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "something-arbitrary";
        state.pdfToImages.rangeResult = {
          totalPages: 1,
          intervals: [[2, 2]],
        };
      });

      // Wait for initial check to complete so rangeChecking becomes false
      await waitFor(() => {
        expect(screen.getByText("1 ページを変換します")).toBeInTheDocument();
      });

      // Currently intervals says only page 2 is selected (despite rangeText being "something-arbitrary")
      const btn2 = screen.getByRole("button", { name: "2 ページ" });
      const btn3 = screen.getByRole("button", { name: "3 ページ" });
      expect(btn2).toHaveAttribute("aria-pressed", "true");
      expect(btn3).toHaveAttribute("aria-pressed", "false");

      // Click unselected page 3 -> toggles [2, 2] + 3 -> "2-3"
      fireEvent.click(btn3);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("2-3");
      });
    });

    it("has accessible aria-label on thumbnail buttons and can be activated via Enter key", async () => {
      mockCommands((cmd, args) => {
        if (cmd === "check_page_range" && isTextArgs(args)) {
          return parseRangeToIntervals(args.text);
        }
        return undefined;
      });

      renderFeature((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "range";
        state.pdfToImages.rangeText = "1";
        state.pdfToImages.rangeResult = { totalPages: 1, intervals: [[1, 1]] };
      });

      // Wait for initial check to finish so rangeChecking is false
      await waitFor(() => {
        expect(screen.getByText("1 ページを変換します")).toBeInTheDocument();
      });

      const thumb2Btn = screen.getByRole("button", { name: "2 ページ" });
      expect(thumb2Btn).toHaveAttribute("aria-label", "2 ページ");

      // Press Enter key on button
      fireEvent.keyDown(thumb2Btn, { key: "Enter", code: "Enter" });
      fireEvent.click(thumb2Btn);

      await waitFor(() => {
        expect(screen.getByLabelText(/変換するページ/)).toHaveValue("1-2");
      });
    });
  });
});
