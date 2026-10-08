import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CheckPageRangeResult, PdfItem } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  type AppState,
} from "../../state";
import {
  PdfToImagesSettings,
  RANGE_ERROR_REVEAL_DELAY_MS,
} from "./PdfToImagesSettings";
import { PdfToImagesStatus } from "./PdfToImagesStatus";

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
      {/* The footer line, where the number of pages to convert is shown. */}
      <PdfToImagesStatus />
    </AppStateProvider>,
  );
}

describe("PdfToImagesSettings", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
    vi.useRealTimers();
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

  describe("switching to a range", () => {
    function pickRange() {
      fireEvent.click(screen.getByRole("button", { name: "範囲を指定" }));
      return screen.getByLabelText(/変換するページ/);
    }

    it("starts from every page of the longest PDF when the range is empty", () => {
      mockCommands(() => new Promise(() => {}));
      renderSettings((state) => {
        state.pdfToImages.items = [
          { ...samplePdf1, pageCount: 4 },
          { ...samplePdf2, pageCount: 12 },
        ];
      });

      expect(pickRange()).toHaveValue("1-12");
    });

    it("keeps a range typed before", () => {
      mockCommands(() => new Promise(() => {}));
      renderSettings((state) => {
        state.pdfToImages.items = [{ ...samplePdf1, pageCount: 4 }];
        state.pdfToImages.rangeText = "2, 4";
      });

      expect(pickRange()).toHaveValue("2, 4");
    });

    it("stays empty with no readable PDF", () => {
      mockCommands(() => new Promise(() => {}));
      renderSettings((state) => {
        state.pdfToImages.items = [
          {
            ...samplePdf1,
            pageCount: 0,
            error: { code: "PasswordProtected", detail: null },
          },
        ];
      });

      expect(pickRange()).toHaveValue("");
    });
  });

  it("renders range input with autocomplete='off'", () => {
    mockCommands(() => undefined);
    renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.pageSelection = "range";
    });

    const input = screen.getByLabelText(/変換するページ/);
    expect(input).toHaveAttribute("autocomplete", "off");
  });

  it("shows thumbnail pick hint only when exactly one error-free PDF is in the list", () => {
    mockCommands(() => undefined);
    const hintText = "サムネイルを押してもページを選べます（Shift で範囲）";

    // 0 PDFs: no hint
    const { unmount } = renderSettings((state) => {
      state.pdfToImages.items = [];
    });
    expect(screen.queryByText(hintText)).not.toBeInTheDocument();
    unmount();

    // 1 error-free PDF with "all": hint shown
    const renderedSingleAll = renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.pageSelection = "all";
    });
    expect(screen.getByText(hintText)).toBeInTheDocument();
    renderedSingleAll.unmount();

    // 1 error-free PDF with "range": hint shown
    const renderedSingleRange = renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.pageSelection = "range";
    });
    expect(screen.getByText(hintText)).toBeInTheDocument();
    renderedSingleRange.unmount();

    // 1 PDF with error: no hint
    const renderedErrorPdf = renderSettings((state) => {
      state.pdfToImages.items = [
        {
          id: 3,
          name: "corrupt.pdf",
          pageCount: 0,
          firstPageSizePt: null,
          bytes: 1000,
          error: { code: "PdfOpenFailed", detail: null },
        },
      ];
    });
    expect(screen.queryByText(hintText)).not.toBeInTheDocument();
    renderedErrorPdf.unmount();

    // 2 PDFs: no hint
    const renderedTwo = renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1, samplePdf2];
    });
    expect(screen.queryByText(hintText)).not.toBeInTheDocument();
    renderedTwo.unmount();
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
      expect(
        screen.getByText("4 ページを PNG で保存します"),
      ).toBeInTheDocument();
    });

    const checkCall = calls.find((c) => c.cmd === "check_page_range");
    expect(checkCall).toBeDefined();
    expect(checkCall?.args).toEqual({
      text: "1-3, 5",
      ids: [1],
    });
  });

  describe("showing a range error", () => {
    function mockInvalidRange() {
      mockCommands((cmd) => {
        if (cmd === "check_page_range") {
          return Promise.reject({ code: "InvalidPageRange", detail: "1-" });
        }
        return undefined;
      });
    }

    function renderRange() {
      renderSettings((state) => {
        state.pdfToImages.items = [samplePdf1];
        state.pdfToImages.pageSelection = "range";
      });
      return screen.getByLabelText(/変換するページ/);
    }

    it("waits until typing pauses", async () => {
      vi.useFakeTimers();
      mockInvalidRange();
      const input = renderRange();

      fireEvent.change(input, { target: { value: "1-" } });
      await act(async () => {});
      act(() => {
        vi.advanceTimersByTime(RANGE_ERROR_REVEAL_DELAY_MS - 1);
      });
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();

      act(() => {
        vi.advanceTimersByTime(1);
      });
      const alert = screen.getByRole("alert");
      expect(alert).toHaveTextContent(
        "ページの範囲の書き方が正しくありません（1-）",
      );
      expect(alert).toHaveClass("error-text");
    });

    it("shows at once when the field loses focus", async () => {
      vi.useFakeTimers();
      mockInvalidRange();
      const input = renderRange();

      fireEvent.change(input, { target: { value: "1-" } });
      await act(async () => {});
      fireEvent.blur(input);

      expect(screen.getByRole("alert")).toBeInTheDocument();
    });

    it("hides again while the next text is typed", async () => {
      vi.useFakeTimers();
      mockInvalidRange();
      const input = renderRange();

      fireEvent.change(input, { target: { value: "1-" } });
      await act(async () => {});
      act(() => {
        vi.advanceTimersByTime(RANGE_ERROR_REVEAL_DELAY_MS);
      });
      expect(screen.getByRole("alert")).toBeInTheDocument();

      fireEvent.change(input, { target: { value: "1-," } });
      await act(async () => {});
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  it("leaves the number of pages to the footer, not under the field", async () => {
    mockCommands((cmd) =>
      cmd === "check_page_range"
        ? { totalPages: 4, intervals: [[1, 4]] }
        : undefined,
    );
    const { container } = renderSettings((state) => {
      state.pdfToImages.items = [samplePdf1];
      state.pdfToImages.pageSelection = "range";
    });

    fireEvent.change(screen.getByLabelText(/変換するページ/), {
      target: { value: "1-4" },
    });
    await waitFor(() => {
      expect(
        screen.getByText("4 ページを PNG で保存します"),
      ).toBeInTheDocument();
    });
    const panel = container.querySelector(".pdf-settings");
    expect(panel).not.toBeNull();
    expect(panel).not.toHaveTextContent("4 ページ");
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

    expect(screen.getByText("2 ページを PNG で保存します")).toBeInTheDocument();

    // Slow one resolves later with 10 pages
    await act(async () => {
      resolveFirst({
        totalPages: 10,
        intervals: [[1, 10]],
      });
    });

    // It should STILL be 2 pages, not 10!
    expect(screen.getByText("2 ページを PNG で保存します")).toBeInTheDocument();
    expect(
      screen.queryByText("10 ページを PNG で保存します"),
    ).not.toBeInTheDocument();
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
