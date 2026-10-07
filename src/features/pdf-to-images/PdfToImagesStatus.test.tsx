import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ja, en } from "../../i18n";
import type { CheckPageRangeResult, PdfItem } from "../../ipc";
import {
  AppStateProvider,
  createInitialAppState,
  type PdfToImagesState,
} from "../../state";
import { getPdfToImagesStatusText } from "./pdfToImagesStatusText";
import { PdfToImagesStatus } from "./PdfToImagesStatus";

const validPdf1: PdfItem = {
  id: 1,
  name: "doc1.pdf",
  pageCount: 5,
  firstPageSizePt: { widthPt: 595, heightPt: 842 },
  bytes: 10000,
  error: null,
};

const validPdf2: PdfItem = {
  id: 2,
  name: "doc2.pdf",
  pageCount: 19,
  firstPageSizePt: { widthPt: 595, heightPt: 842 },
  bytes: 20000,
  error: null,
};

const validPdfSinglePage: PdfItem = {
  id: 4,
  name: "single.pdf",
  pageCount: 1,
  firstPageSizePt: { widthPt: 595, heightPt: 842 },
  bytes: 5000,
  error: null,
};

const corruptPdf: PdfItem = {
  id: 3,
  name: "broken.pdf",
  pageCount: 0,
  firstPageSizePt: null,
  bytes: 100,
  error: { code: "DecodeFailed", detail: null },
};

function createBaseState(
  overrides: Partial<PdfToImagesState> = {},
): PdfToImagesState {
  return {
    items: [validPdf1],
    pageSelection: "all",
    rangeText: "",
    format: "png",
    dpi: 150,
    outputDir: null,
    rangeResult: null,
    rangeError: null,
    rangeChecking: false,
    ...overrides,
  };
}

describe("PdfToImagesStatus", () => {
  afterEach(() => {
    cleanup();
  });

  describe("getPdfToImagesStatusText (pure function)", () => {
    it("returns 'no PDFs selected' when list is empty", () => {
      const state = createBaseState({ items: [] });
      expect(getPdfToImagesStatusText(state, ja)).toBe(
        "PDF が選ばれていません",
      );
      expect(getPdfToImagesStatusText(state, en)).toBe("No PDFs selected");
    });

    it("returns 'no PDFs selected' when all PDFs have errors", () => {
      const state = createBaseState({ items: [corruptPdf] });
      expect(getPdfToImagesStatusText(state, ja)).toBe(
        "PDF が選ばれていません",
      );
      expect(getPdfToImagesStatusText(state, en)).toBe("No PDFs selected");
    });

    describe("when pageSelection is 'all'", () => {
      it("formats single PDF correctly with sum of pageCount", () => {
        const state = createBaseState({ items: [validPdf1] });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "5 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "Will save 5 pages as PNG",
        );
      });

      it("formats single PDF with 1 page using English singular", () => {
        const state = createBaseState({ items: [validPdfSinglePage] });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "1 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "Will save 1 page as PNG",
        );
      });

      it("formats multiple PDFs with 'From {count} PDFs' prefix", () => {
        const state = createBaseState({ items: [validPdf1, validPdf2] });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "PDF 2 件から 24 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "From 2 PDFs, will save 24 pages as PNG",
        );
      });

      it("formats multiple PDFs with 1 total page in English singular", () => {
        const zeroPagePdf: PdfItem = {
          id: 5,
          name: "zero.pdf",
          pageCount: 0,
          firstPageSizePt: { widthPt: 595, heightPt: 842 },
          bytes: 1000,
          error: null,
        };
        const state = createBaseState({
          items: [validPdfSinglePage, zeroPagePdf],
        });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "PDF 2 件から 1 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "From 2 PDFs, will save 1 page as PNG",
        );
      });

      it("excludes error PDFs from count and pageCount sum", () => {
        const state = createBaseState({ items: [validPdf1, corruptPdf] });
        // Only validPdf1 should be counted (1 PDF, 5 pages)
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "5 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "Will save 5 pages as PNG",
        );
      });

      it("changes format label when format is jpeg", () => {
        const state = createBaseState({ items: [validPdf1], format: "jpeg" });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "5 ページを JPEG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "Will save 5 pages as JPEG",
        );
      });
    });

    describe("when pageSelection is 'range'", () => {
      it("prompts to enter pages when rangeText is empty or whitespace", () => {
        const stateEmpty = createBaseState({
          pageSelection: "range",
          rangeText: "",
        });
        expect(getPdfToImagesStatusText(stateEmpty, ja)).toBe(
          "変換するページを指定してください",
        );
        expect(getPdfToImagesStatusText(stateEmpty, en)).toBe(
          "Enter the pages to convert",
        );

        const stateWhitespace = createBaseState({
          pageSelection: "range",
          rangeText: "   ",
        });
        expect(getPdfToImagesStatusText(stateWhitespace, ja)).toBe(
          "変換するページを指定してください",
        );
        expect(getPdfToImagesStatusText(stateWhitespace, en)).toBe(
          "Enter the pages to convert",
        );
      });

      it("shows nothing when rangeError is present", () => {
        const state = createBaseState({
          pageSelection: "range",
          rangeText: "invalid-range",
          rangeError: { code: "InvalidPageRange", detail: null },
        });
        expect(getPdfToImagesStatusText(state, ja)).toBeNull();
        expect(getPdfToImagesStatusText(state, en)).toBeNull();
      });

      it("shows 'no pages match' when rangeResult has totalPages = 0", () => {
        const rangeResult: CheckPageRangeResult = {
          totalPages: 0,
          intervals: [],
        };
        const state = createBaseState({
          pageSelection: "range",
          rangeText: "99",
          rangeResult,
        });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "範囲に当てはまるページがありません",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "No pages match the range",
        );
      });

      it("uses rangeResult.totalPages regardless of rangeText content", () => {
        // rangeText is "1-3", but mock returned totalPages = 42
        const rangeResult: CheckPageRangeResult = {
          totalPages: 42,
          intervals: [[1, 3]],
        };
        const state = createBaseState({
          pageSelection: "range",
          rangeText: "1-3",
          rangeResult,
        });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "42 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "Will save 42 pages as PNG",
        );
      });

      it("formats 1 page in English singular for range selection", () => {
        const rangeResult: CheckPageRangeResult = {
          totalPages: 1,
          intervals: [[1, 1]],
        };
        const state = createBaseState({
          pageSelection: "range",
          rangeText: "1",
          rangeResult,
        });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "1 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "Will save 1 page as PNG",
        );
      });

      it("adds 'From {count} PDFs' prefix when multiple valid PDFs are present", () => {
        const rangeResult: CheckPageRangeResult = {
          totalPages: 10,
          intervals: [[1, 5]],
        };
        const state = createBaseState({
          items: [validPdf1, validPdf2, corruptPdf], // 2 valid PDFs
          pageSelection: "range",
          rangeText: "1-5",
          rangeResult,
        });
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "PDF 2 件から 10 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "From 2 PDFs, will save 10 pages as PNG",
        );
      });

      it("retains previous rangeResult during range checking", () => {
        const prevResult: CheckPageRangeResult = {
          totalPages: 7,
          intervals: [[1, 7]],
        };
        const state = createBaseState({
          pageSelection: "range",
          rangeText: "1-8",
          rangeChecking: true,
          rangeResult: prevResult,
        });
        // Still shows previous count of 7 while checking
        expect(getPdfToImagesStatusText(state, ja)).toBe(
          "7 ページを PNG で保存します",
        );
        expect(getPdfToImagesStatusText(state, en)).toBe(
          "Will save 7 pages as PNG",
        );
      });

      it("shows nothing during range checking when there is no previous rangeResult", () => {
        const state = createBaseState({
          pageSelection: "range",
          rangeText: "1-5",
          rangeChecking: true,
          rangeResult: null,
        });
        expect(getPdfToImagesStatusText(state, ja)).toBeNull();
        expect(getPdfToImagesStatusText(state, en)).toBeNull();
      });
    });
  });

  describe("PdfToImagesStatus component", () => {
    it("renders hint class with text when text is available", () => {
      const initialState = createInitialAppState("ja-JP");
      initialState.language.activeTab = "pdfToImages";
      initialState.pdfToImages.items = [validPdf1];

      render(
        <AppStateProvider initialState={initialState}>
          <PdfToImagesStatus />
        </AppStateProvider>,
      );

      const span = screen.getByText("5 ページを PNG で保存します");
      expect(span).toBeInTheDocument();
      expect(span).toHaveClass("hint");
    });

    it("renders nothing when status is null (e.g. range error)", () => {
      const initialState = createInitialAppState("ja-JP");
      initialState.language.activeTab = "pdfToImages";
      initialState.pdfToImages.items = [validPdf1];
      initialState.pdfToImages.pageSelection = "range";
      initialState.pdfToImages.rangeText = "bad";
      initialState.pdfToImages.rangeError = {
        code: "InvalidPageRange",
        detail: null,
      };

      const { container } = render(
        <AppStateProvider initialState={initialState}>
          <PdfToImagesStatus />
        </AppStateProvider>,
      );

      expect(container).toBeEmptyDOMElement();
    });
  });
});
