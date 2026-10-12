import { describe, expect, it } from "vitest";
import type { CheckPageRangeResult, IpcError, PdfItem } from "../ipc";
import {
  DEFAULT_RENDER_COLOR_MODE,
  DEFAULT_RENDER_DPI,
  DEFAULT_RENDER_FORMAT,
  DPI_CHOICES,
  RENDER_DPI_MAX,
  RENDER_DPI_MIN,
  hasInvalidCustomDpi,
  initialPdfToImagesState,
  parseCustomDpi,
  pdfToImagesReducer,
} from "./pdfToImages";

const samplePdf1: PdfItem = {
  id: 1,
  name: "a.pdf",
  pageCount: 3,
  firstPageSizePt: { widthPt: 595.28, heightPt: 841.89 },
  bytes: 1234,
  error: null,
};

const samplePdf2: PdfItem = {
  id: 2,
  name: "b.pdf",
  pageCount: 10,
  firstPageSizePt: { widthPt: 612, heightPt: 792 },
  bytes: 5678,
  error: null,
};

describe("pdfToImagesReducer", () => {
  it("has expected initial state defaults", () => {
    expect(initialPdfToImagesState.items).toEqual([]);
    expect(initialPdfToImagesState.pageSelection).toBe("all");
    expect(initialPdfToImagesState.rangeText).toBe("");
    expect(initialPdfToImagesState.format).toBe(DEFAULT_RENDER_FORMAT);
    expect(initialPdfToImagesState.format).toBe("png");
    expect(initialPdfToImagesState.colorMode).toBe(DEFAULT_RENDER_COLOR_MODE);
    expect(initialPdfToImagesState.colorMode).toBe("color");
    expect(initialPdfToImagesState.dpi).toBe(DEFAULT_RENDER_DPI);
    expect(initialPdfToImagesState.dpi).toBe(150);
    expect(initialPdfToImagesState.isCustomDpi).toBe(false);
    expect(initialPdfToImagesState.customDpiText).toBe("150");
    expect(initialPdfToImagesState.outputDir).toBeNull();
    expect(initialPdfToImagesState.rangeResult).toBeNull();
    expect(initialPdfToImagesState.rangeError).toBeNull();
    expect(initialPdfToImagesState.rangeChecking).toBe(false);
    expect(DPI_CHOICES).toEqual([72, 150, 300]);
    expect(RENDER_DPI_MIN).toBe(36);
    expect(RENDER_DPI_MAX).toBe(1000);
  });

  it("handles ADD_PDF_ITEMS", () => {
    const next = pdfToImagesReducer(initialPdfToImagesState, {
      type: "ADD_PDF_ITEMS",
      items: [samplePdf1],
    });
    expect(next.items).toEqual([samplePdf1]);

    // Empty addition returns same state reference
    const unchanged = pdfToImagesReducer(next, {
      type: "ADD_PDF_ITEMS",
      items: [],
    });
    expect(unchanged).toBe(next);
  });

  it("handles REMOVE_PDF_ITEM", () => {
    const withItems = {
      ...initialPdfToImagesState,
      items: [samplePdf1, samplePdf2],
    };
    const removed = pdfToImagesReducer(withItems, {
      type: "REMOVE_PDF_ITEM",
      id: samplePdf1.id,
    });
    expect(removed.items).toEqual([samplePdf2]);

    // Unknown id returns same state reference
    const unchanged = pdfToImagesReducer(removed, {
      type: "REMOVE_PDF_ITEM",
      id: 999,
    });
    expect(unchanged).toBe(removed);
  });

  it("handles CLEAR_PDF_ITEMS", () => {
    const withItems = {
      ...initialPdfToImagesState,
      items: [samplePdf1],
    };
    const cleared = pdfToImagesReducer(withItems, {
      type: "CLEAR_PDF_ITEMS",
    });
    expect(cleared.items).toEqual([]);

    // Clearing empty list returns same state reference
    const unchanged = pdfToImagesReducer(cleared, {
      type: "CLEAR_PDF_ITEMS",
    });
    expect(unchanged).toBe(cleared);
  });

  it("handles SET_PAGE_SELECTION", () => {
    const next = pdfToImagesReducer(initialPdfToImagesState, {
      type: "SET_PAGE_SELECTION",
      selection: "range",
    });
    expect(next.pageSelection).toBe("range");

    const unchanged = pdfToImagesReducer(next, {
      type: "SET_PAGE_SELECTION",
      selection: "range",
    });
    expect(unchanged).toBe(next);
  });

  it("handles SET_RANGE_TEXT", () => {
    const next = pdfToImagesReducer(initialPdfToImagesState, {
      type: "SET_RANGE_TEXT",
      rangeText: "1-3, 5",
    });
    expect(next.rangeText).toBe("1-3, 5");

    const unchanged = pdfToImagesReducer(next, {
      type: "SET_RANGE_TEXT",
      rangeText: "1-3, 5",
    });
    expect(unchanged).toBe(next);
  });

  it("handles SET_RENDER_FORMAT", () => {
    const next = pdfToImagesReducer(initialPdfToImagesState, {
      type: "SET_RENDER_FORMAT",
      format: "jpeg",
    });
    expect(next.format).toBe("jpeg");

    const unchanged = pdfToImagesReducer(next, {
      type: "SET_RENDER_FORMAT",
      format: "jpeg",
    });
    expect(unchanged).toBe(next);
  });

  it("handles SET_RENDER_DPI", () => {
    const next = pdfToImagesReducer(initialPdfToImagesState, {
      type: "SET_RENDER_DPI",
      dpi: 300,
    });
    expect(next.dpi).toBe(300);

    const unchanged = pdfToImagesReducer(next, {
      type: "SET_RENDER_DPI",
      dpi: 300,
    });
    expect(unchanged).toBe(next);
  });

  it("handles SET_PDFS_OUTPUT_DIR", () => {
    const label = { dirLabel: "my-images" };
    const next = pdfToImagesReducer(initialPdfToImagesState, {
      type: "SET_PDFS_OUTPUT_DIR",
      outputDir: label,
    });
    expect(next.outputDir).toEqual(label);

    const cleared = pdfToImagesReducer(next, {
      type: "SET_PDFS_OUTPUT_DIR",
      outputDir: null,
    });
    expect(cleared.outputDir).toBeNull();
  });

  it("handles CHECK_RANGE actions (STARTED, SUCCESS, FAILURE, RESET)", () => {
    const started = pdfToImagesReducer(initialPdfToImagesState, {
      type: "CHECK_RANGE_STARTED",
    });
    expect(started.rangeChecking).toBe(true);

    const result: CheckPageRangeResult = {
      totalPages: 4,
      intervals: [
        [1, 3],
        [5, 5],
      ],
    };
    const success = pdfToImagesReducer(started, {
      type: "CHECK_RANGE_SUCCESS",
      result,
    });
    expect(success.rangeChecking).toBe(false);
    expect(success.rangeResult).toEqual(result);
    expect(success.rangeError).toBeNull();

    const error: IpcError = {
      code: "InvalidPageRange",
      detail: "bad range",
    };
    const failure = pdfToImagesReducer(success, {
      type: "CHECK_RANGE_FAILURE",
      error,
    });
    expect(failure.rangeChecking).toBe(false);
    expect(failure.rangeResult).toBeNull();
    expect(failure.rangeError).toEqual(error);

    const reset = pdfToImagesReducer(failure, {
      type: "CHECK_RANGE_RESET",
    });
    expect(reset.rangeChecking).toBe(false);
    expect(reset.rangeResult).toBeNull();
    expect(reset.rangeError).toBeNull();
  });

  it("handles SET_RENDER_COLOR_MODE", () => {
    const next = pdfToImagesReducer(initialPdfToImagesState, {
      type: "SET_RENDER_COLOR_MODE",
      colorMode: "gray",
    });
    expect(next.colorMode).toBe("gray");

    const unchanged = pdfToImagesReducer(next, {
      type: "SET_RENDER_COLOR_MODE",
      colorMode: "gray",
    });
    expect(unchanged).toBe(next);
  });

  it("handles SET_IS_CUSTOM_DPI and SET_CUSTOM_DPI_TEXT", () => {
    // Enabling custom DPI initializes text from state.dpi
    const customEnabled = pdfToImagesReducer(initialPdfToImagesState, {
      type: "SET_IS_CUSTOM_DPI",
      isCustom: true,
    });
    expect(customEnabled.isCustomDpi).toBe(true);
    expect(customEnabled.customDpiText).toBe("150");

    // Setting valid custom DPI text updates dpi and customDpiText
    const validDpi = pdfToImagesReducer(customEnabled, {
      type: "SET_CUSTOM_DPI_TEXT",
      text: "600",
    });
    expect(validDpi.customDpiText).toBe("600");
    expect(validDpi.dpi).toBe(600);

    // Setting invalid custom DPI text updates text but retains last valid dpi
    const invalidDpi = pdfToImagesReducer(validDpi, {
      type: "SET_CUSTOM_DPI_TEXT",
      text: "35",
    });
    expect(invalidDpi.customDpiText).toBe("35");
    expect(invalidDpi.dpi).toBe(600);

    // Setting preset DPI resets isCustomDpi and updates dpi & customDpiText
    const presetDpi = pdfToImagesReducer(invalidDpi, {
      type: "SET_RENDER_DPI",
      dpi: 72,
    });
    expect(presetDpi.isCustomDpi).toBe(false);
    expect(presetDpi.dpi).toBe(72);
    expect(presetDpi.customDpiText).toBe("72");
  });

  it("parses custom DPI correctly with whitespace and full-width numbers", () => {
    // Valid numbers
    expect(parseCustomDpi("36")).toBe(36);
    expect(parseCustomDpi("1000")).toBe(1000);
    expect(parseCustomDpi("600")).toBe(600);
    expect(parseCustomDpi(" 600 ")).toBe(600);
    expect(parseCustomDpi("\t150\n")).toBe(150);

    // Full-width numbers and whitespace
    expect(parseCustomDpi("６００")).toBe(600);
    expect(parseCustomDpi("　６００　")).toBe(600);
    expect(parseCustomDpi("０３６")).toBe(36);
    expect(parseCustomDpi("１０００")).toBe(1000);

    // Out of bounds
    expect(parseCustomDpi("35")).toBeNull();
    expect(parseCustomDpi("1001")).toBeNull();
    expect(parseCustomDpi("０")).toBeNull();
    expect(parseCustomDpi("０３５")).toBeNull();
    expect(parseCustomDpi("１００１")).toBeNull();

    // Invalid formatting
    expect(parseCustomDpi("")).toBeNull();
    expect(parseCustomDpi("   ")).toBeNull();
    expect(parseCustomDpi("　")).toBeNull();
    expect(parseCustomDpi("12.5")).toBeNull();
    expect(parseCustomDpi("+100")).toBeNull();
    expect(parseCustomDpi("-100")).toBeNull();
    expect(parseCustomDpi("abc")).toBeNull();
    expect(parseCustomDpi("600dpi")).toBeNull();
    expect(parseCustomDpi("6 00")).toBeNull();
  });

  it("checks whether state has invalid custom DPI", () => {
    // When isCustomDpi is false, hasInvalidCustomDpi is false even with invalid text
    expect(
      hasInvalidCustomDpi({ isCustomDpi: false, customDpiText: "bad" }),
    ).toBe(false);
    expect(
      hasInvalidCustomDpi({ isCustomDpi: false, customDpiText: "600" }),
    ).toBe(false);

    // When isCustomDpi is true, validates customDpiText
    expect(
      hasInvalidCustomDpi({ isCustomDpi: true, customDpiText: "600" }),
    ).toBe(false);
    expect(
      hasInvalidCustomDpi({ isCustomDpi: true, customDpiText: "36" }),
    ).toBe(false);
    expect(
      hasInvalidCustomDpi({ isCustomDpi: true, customDpiText: "1000" }),
    ).toBe(false);

    // Invalid values
    expect(hasInvalidCustomDpi({ isCustomDpi: true, customDpiText: "" })).toBe(
      true,
    );
    expect(
      hasInvalidCustomDpi({ isCustomDpi: true, customDpiText: "35" }),
    ).toBe(true);
    expect(
      hasInvalidCustomDpi({ isCustomDpi: true, customDpiText: "1001" }),
    ).toBe(true);
    expect(
      hasInvalidCustomDpi({ isCustomDpi: true, customDpiText: "abc" }),
    ).toBe(true);
  });
});
