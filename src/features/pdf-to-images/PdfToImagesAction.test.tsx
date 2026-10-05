import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { PdfItem } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  initialJobState,
  initialPdfToImagesState,
  type AppState,
} from "../../state";
import { PdfToImagesAction } from "./PdfToImagesAction";

const samplePdf1: PdfItem = {
  id: 1,
  name: "a.pdf",
  pageCount: 4,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 1000,
  error: null,
};

const samplePdf2: PdfItem = {
  id: 2,
  name: "b.pdf",
  pageCount: 7,
  firstPageSizePt: { widthPt: 612, heightPt: 792 },
  bytes: 2000,
  error: null,
};

const errorPdf: PdfItem = {
  id: 3,
  name: "err.pdf",
  pageCount: 0,
  firstPageSizePt: null,
  bytes: 3000,
  error: { code: "PdfOpenFailed", detail: null },
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

function renderAction(stateModifier?: (state: AppState) => void) {
  const base = createInitialAppState("ja-JP");
  const state: AppState = {
    ...base,
    job: {
      ...base.job,
      targets: [],
      startedIds: [],
      results: {},
    },
    pdfToImages: {
      ...base.pdfToImages,
      items: [],
    },
  };
  state.language.activeTab = "pdfToImages";
  stateModifier?.(state);
  return render(
    <AppStateProvider initialState={state}>
      <PdfToImagesAction />
    </AppStateProvider>,
  );
}

describe("PdfToImagesAction", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
    initialJobState.phase = "idle";
    initialPdfToImagesState.items = [];
    initialPdfToImagesState.outputDir = null;
    initialPdfToImagesState.rangeResult = null;
    initialPdfToImagesState.rangeError = null;
    initialPdfToImagesState.rangeChecking = false;
  });

  it("is disabled when target list is empty", () => {
    mockCommands(() => undefined);
    renderAction((state) => {
      state.pdfToImages.items = [];
      state.pdfToImages.outputDir = { dirLabel: "output" };
    });

    expect(screen.getByRole("button", { name: "変換を開始" })).toBeDisabled();
  });

  it("is disabled when only error items are in list", () => {
    mockCommands(() => undefined);
    renderAction((state) => {
      state.pdfToImages.items = [errorPdf];
      state.pdfToImages.outputDir = { dirLabel: "output" };
    });

    expect(screen.getByRole("button", { name: "変換を開始" })).toBeDisabled();
  });

  it("is disabled when output directory is not chosen", () => {
    mockCommands(() => undefined);
    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = null;
    });

    expect(screen.getByRole("button", { name: "変換を開始" })).toBeDisabled();
  });

  it("is disabled when job is already running", () => {
    mockCommands(() => undefined);
    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = { dirLabel: "output" };
      state.job.phase = "running";
    });

    expect(screen.getByRole("button", { name: "変換を開始" })).toBeDisabled();
  });

  it("is disabled when page range check is checking or has error or has no result", () => {
    mockCommands(() => undefined);

    // Range checking in flight
    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = { dirLabel: "output" };
      state.pdfToImages.pageSelection = "range";
      state.pdfToImages.rangeChecking = true;
    });
    expect(screen.getByRole("button", { name: "変換を開始" })).toBeDisabled();

    cleanup();

    // Range has error
    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = { dirLabel: "output" };
      state.pdfToImages.pageSelection = "range";
      state.pdfToImages.rangeError = {
        code: "InvalidPageRange",
        detail: "bad",
      };
    });
    expect(screen.getByRole("button", { name: "変換を開始" })).toBeDisabled();

    cleanup();

    // Range has no result yet
    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = { dirLabel: "output" };
      state.pdfToImages.pageSelection = "range";
      state.pdfToImages.rangeResult = null;
    });
    expect(screen.getByRole("button", { name: "変換を開始" })).toBeDisabled();
  });

  it("calls start_pdfs_to_images with 1-{maxPages} when pageSelection is 'all'", async () => {
    const calls = mockCommands((cmd) =>
      cmd === "start_pdfs_to_images" ? undefined : undefined,
    );

    renderAction((state) => {
      // samplePdf1 has 4 pages, samplePdf2 has 7 pages -> maxPages = 7
      state.pdfToImages.items = [samplePdf1, samplePdf2, errorPdf];
      state.pdfToImages.outputDir = { dirLabel: "output" };
      state.pdfToImages.pageSelection = "all";
      state.pdfToImages.format = "png";
      state.pdfToImages.dpi = 150;
    });

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    expect(startBtn).toBeEnabled();

    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_pdfs_to_images",
        args: {
          ids: [1, 2], // errorPdf (id: 3) is excluded
          range: "1-7", // 1-{maxPages}
          format: "png",
          dpi: 150,
        },
      });
    });
  });

  it("calls start_pdfs_to_images with rangeText when pageSelection is 'range'", async () => {
    const calls = mockCommands((cmd) =>
      cmd === "start_pdfs_to_images" ? undefined : undefined,
    );

    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1, samplePdf2];
      state.pdfToImages.outputDir = { dirLabel: "output" };
      state.pdfToImages.pageSelection = "range";
      state.pdfToImages.rangeText = "1-3, 5";
      state.pdfToImages.rangeResult = {
        totalPages: 4,
        intervals: [
          [1, 3],
          [5, 5],
        ],
      };
      state.pdfToImages.format = "jpeg";
      state.pdfToImages.dpi = 300;
    });

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    expect(startBtn).toBeEnabled();

    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_pdfs_to_images",
        args: {
          ids: [1, 2],
          range: "1-3, 5",
          format: "jpeg",
          dpi: 300,
        },
      });
    });
  });
});
