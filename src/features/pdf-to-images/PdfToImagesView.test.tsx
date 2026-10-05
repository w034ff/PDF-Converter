import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { AddResult, JobFinishedPayload, PdfItem } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  type AppState,
} from "../../state";
import { PdfToImagesView } from "./PdfToImagesView";

const samplePdf1: PdfItem = {
  id: 1,
  name: "annual-report.pdf",
  pageCount: 4,
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
  const state = createInitialAppState("ja-JP");
  state.language.activeTab = "pdfToImages";
  stateModifier?.(state);
  return render(
    <AppStateProvider initialState={state}>
      <PdfToImagesView />
    </AppStateProvider>,
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

    it("calls remove_items and clears item on 'すべて外す'", async () => {
      const calls = mockCommands((cmd) =>
        cmd === "remove_items" ? undefined : undefined,
      );
      renderView((state) => {
        state.pdfToImages.items = [samplePdf1];
      });

      const clearAllBtn = screen.getByRole("button", { name: "すべて外す" });
      fireEvent.click(clearAllBtn);

      await waitFor(() => {
        expect(screen.getByTestId("drop-zone")).toBeInTheDocument();
      });

      expect(calls).toContainEqual({
        cmd: "remove_items",
        args: { ids: [1] },
      });
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
  });
});
