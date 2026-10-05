import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { CheckPageRangeResult, PdfItem } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  type AppState,
} from "../../state";
import { PdfToImagesSettings } from "./PdfToImagesSettings";

const samplePdf1: PdfItem = {
  id: 1,
  name: "report.pdf",
  pageCount: 8,
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

function renderSettings(stateModifier?: (state: AppState) => void) {
  const state = createInitialAppState("ja-JP");
  state.language.activeTab = "pdfToImages";
  stateModifier?.(state);
  return render(
    <AppStateProvider initialState={state}>
      <PdfToImagesSettings />
    </AppStateProvider>,
  );
}

describe("PdfToImagesSettings", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("renders page selection segmented control and toggles range input", () => {
    mockCommands(() => undefined);
    renderSettings();

    // Initially "all" (すべて) is selected, range input is not visible
    expect(screen.getByRole("button", { name: "すべて" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "範囲を指定" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.queryByLabelText(/変換するページ/)).not.toBeInTheDocument();

    // Click "範囲を指定"
    fireEvent.click(screen.getByRole("button", { name: "範囲を指定" }));
    expect(screen.getByRole("button", { name: "範囲を指定" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByLabelText(/変換するページ/)).toBeInTheDocument();
  });

  it("calls check_page_range on range input and displays total pages", async () => {
    const rangeResult: CheckPageRangeResult = {
      totalPages: 4,
      intervals: [
        [1, 3],
        [5, 5],
      ],
    };
    const calls = mockCommands((cmd) => {
      if (cmd === "check_page_range") {
        return rangeResult;
      }
      return undefined;
    });

    renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.pageSelection = "range";
    });

    const input = screen.getByLabelText(/変換するページ/);
    fireEvent.change(input, { target: { value: "1-3, 5" } });

    await waitFor(() => {
      expect(screen.getByText("4 ページを変換します")).toBeInTheDocument();
    });

    const checkCall = calls.find((c) => c.cmd === "check_page_range");
    expect(checkCall).toBeDefined();
    expect(checkCall?.args).toEqual({
      text: "1-3, 5",
      ids: [1],
    });
  });

  it("shows error when range format is invalid", async () => {
    mockCommands((cmd) => {
      if (cmd === "check_page_range") {
        return Promise.reject({
          code: "InvalidPageRange",
          detail: "abc",
        });
      }
      return undefined;
    });

    renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.pageSelection = "range";
    });

    const input = screen.getByLabelText(/変換するページ/);
    fireEvent.change(input, { target: { value: "abc" } });

    await waitFor(() => {
      const alert = screen.getByRole("alert");
      expect(alert).toHaveTextContent(
        "ページの範囲の書き方が正しくありません（abc）",
      );
    });
  });

  it("handles race condition: older slow answer does not overwrite newer answer", async () => {
    const resolvers: {
      first?: (res: CheckPageRangeResult) => void;
      second?: (res: CheckPageRangeResult) => void;
    } = {};

    mockCommands((_cmd, args) => {
      if (typeof args === "object" && args !== null && "text" in args) {
        if (args.text === "slow") {
          return new Promise<CheckPageRangeResult>((resolve) => {
            resolvers.first = resolve;
          });
        }
        if (args.text === "fast") {
          return new Promise<CheckPageRangeResult>((resolve) => {
            resolvers.second = resolve;
          });
        }
      }
      return undefined;
    });

    renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.pageSelection = "range";
    });

    const input = screen.getByLabelText(/変換するページ/);

    // 1st request (slow)
    fireEvent.change(input, { target: { value: "slow" } });

    // 2nd request (fast)
    fireEvent.change(input, { target: { value: "fast" } });

    // Wait until both requests have been dispatched to mock and both resolvers are ready
    await waitFor(() => {
      expect(resolvers.first).toBeDefined();
      expect(resolvers.second).toBeDefined();
    });

    const resolveFirst = resolvers.first;
    const resolveSecond = resolvers.second;
    if (resolveFirst === undefined || resolveSecond === undefined) {
      throw new Error("Both resolvers must be defined");
    }

    // Fast one resolves first with 2 pages
    await act(async () => {
      resolveSecond({
        totalPages: 2,
        intervals: [[2, 3]],
      });
    });

    expect(screen.getByText("2 ページを変換します")).toBeInTheDocument();

    // Slow one resolves later with 10 pages
    await act(async () => {
      resolveFirst({
        totalPages: 10,
        intervals: [[1, 10]],
      });
    });

    // It should STILL be 2 pages, not 10!
    expect(screen.getByText("2 ページを変換します")).toBeInTheDocument();
    expect(screen.queryByText("10 ページを変換します")).not.toBeInTheDocument();
  });

  it("displays rendered pixel dimension hint when 1 PDF is in the list", () => {
    mockCommands(() => undefined);
    renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.dpi = 150;
    });

    // 595.28 pt x 841.89 pt at 150 dpi is A4 -> 1240 x 1754 px
    expect(
      screen.getByText("A4 の 1 ページ → 1240 × 1754 px"),
    ).toBeInTheDocument();

    // Changing DPI to 300 updates pixel dimension
    const dpiSelect = screen.getByLabelText("解像度");
    fireEvent.change(dpiSelect, { target: { value: "300" } });

    expect(
      screen.getByText("A4 の 1 ページ → 2480 × 3508 px"),
    ).toBeInTheDocument();
  });

  it("does not show pixel dimension hint when multiple PDFs are in list", () => {
    mockCommands(() => undefined);
    renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1, samplePdf2];
    });

    expect(screen.queryByText(/の 1 ページ →/)).not.toBeInTheDocument();
  });

  it("disables all inputs when conversion is running", () => {
    mockCommands(() => undefined);
    renderSettings((state) => {
      state.pdfToImages.pageSelection = "all";
      state.job.phase = "running";
    });

    expect(screen.getByRole("button", { name: "すべて" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "範囲を指定" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "PNG" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "JPEG" })).toBeDisabled();
    expect(screen.getByLabelText("解像度")).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "フォルダを選ぶ" }),
    ).toBeDisabled();
  });
});
