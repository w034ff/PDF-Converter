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
  useAppState,
  type AppState,
} from "../../state";
import { JobFooter } from "../job/JobFooter";
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

function ActionHarness() {
  const { pdfToImages } = useAppState();
  return (
    <div>
      <span data-testid="output-dir-label">
        {pdfToImages.outputDir?.dirLabel ?? "not-chosen"}
      </span>
      <PdfToImagesAction />
      <footer data-testid="footer-pdf">
        <JobFooter
          tab="pdfToImages"
          idleStatus={<span>idle-pdf</span>}
          action={<span />}
        />
      </footer>
      <footer data-testid="footer-images">
        <JobFooter
          tab="imagesToPdf"
          idleStatus={<span>idle-images</span>}
          action={<span />}
        />
      </footer>
    </div>
  );
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
      <ActionHarness />
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

  it("is enabled when output directory is not chosen, and starts conversion after picking output folder", async () => {
    const calls = mockCommands((cmd) => {
      if (cmd === "pick_output_dir") {
        return { dirLabel: "picked-folder" };
      }
      return undefined;
    });

    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = null;
    });

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    expect(startBtn).toBeEnabled();

    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "pick_output_dir",
        args: { kind: "pdfToImages" },
      });
    });

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_pdfs_to_images",
        args: {
          ids: [1],
          range: "1-4",
          format: "png",
          dpi: 150,
        },
      });
    });

    expect(screen.getByTestId("output-dir-label")).toHaveTextContent(
      "picked-folder",
    );
  });

  it("does not start conversion when pick_output_dir is cancelled", async () => {
    const calls = mockCommands((cmd) => {
      if (cmd === "pick_output_dir") {
        return null;
      }
      return undefined;
    });

    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = null;
    });

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "pick_output_dir",
        args: { kind: "pdfToImages" },
      });
    });

    expect(calls.some((c) => c.cmd === "start_pdfs_to_images")).toBe(false);
    expect(screen.getByTestId("output-dir-label")).toHaveTextContent(
      "not-chosen",
    );
  });

  it("shows error in bottom bar when pick_output_dir fails and does not show on other tab", async () => {
    const calls = mockCommands((cmd) => {
      if (cmd === "pick_output_dir") {
        throw { code: "ReadFailed", detail: "folder read error" };
      }
      return undefined;
    });

    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = null;
    });

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "pick_output_dir",
        args: { kind: "pdfToImages" },
      });
    });

    expect(calls.some((c) => c.cmd === "start_pdfs_to_images")).toBe(false);

    await waitFor(() => {
      expect(screen.getByTestId("footer-pdf")).toHaveTextContent(
        "ファイルの読み込みに失敗しました",
      );
    });

    expect(screen.getByTestId("footer-images")).toHaveTextContent(
      "idle-images",
    );
  });

  it("disables button while waiting for pick_output_dir response", async () => {
    mockCommands((cmd) => {
      if (cmd === "pick_output_dir") {
        return new Promise(() => {}); // never resolves
      }
      return undefined;
    });

    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = null;
    });

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    expect(startBtn).toBeEnabled();

    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(startBtn).toBeDisabled();
    });
  });

  it("starts conversion directly without pick_output_dir when directory is already chosen", async () => {
    const calls = mockCommands(() => undefined);

    renderAction((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.outputDir = { dirLabel: "existing-dir" };
    });

    const startBtn = screen.getByRole("button", { name: "変換を開始" });
    fireEvent.click(startBtn);

    await waitFor(() => {
      expect(calls).toContainEqual({
        cmd: "start_pdfs_to_images",
        args: {
          ids: [1],
          range: "1-4",
          format: "png",
          dpi: 150,
        },
      });
    });

    expect(calls.some((c) => c.cmd === "pick_output_dir")).toBe(false);
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
