import { describe, expect, it } from "vitest";
import { ja } from "../../i18n/ja";
import {
  calculateRenderDimensions,
  countPagesInIntervals,
  formatFileSize,
  formatPaperSize,
  formatSkippedSummary,
  isPageInIntervals,
} from "./pdfUtils";

describe("pdfUtils", () => {
  describe("formatPaperSize", () => {
    it("returns empty string for null", () => {
      expect(formatPaperSize(null)).toBe("");
    });

    it("identifies A4 portrait and landscape", () => {
      expect(formatPaperSize({ widthPt: 595.28, heightPt: 841.89 })).toBe("A4");
      expect(formatPaperSize({ widthPt: 841.89, heightPt: 595.28 })).toBe("A4");
    });

    it("identifies Letter portrait and landscape", () => {
      expect(formatPaperSize({ widthPt: 612, heightPt: 792 })).toBe("Letter");
      expect(formatPaperSize({ widthPt: 792, heightPt: 612 })).toBe("Letter");
    });

    it("formats custom point dimensions", () => {
      expect(formatPaperSize({ widthPt: 200.4, heightPt: 300.6 })).toBe(
        "200 × 301 pt",
      );
    });
  });

  describe("calculateRenderDimensions", () => {
    it("computes dimensions according to design §4.4 (pt ÷ 72 × dpi rounded)", () => {
      // A4 at 150 dpi: 595.28 / 72 * 150 = 1240.17 -> 1240, 841.89 / 72 * 150 = 1753.94 -> 1754
      const a4 = { widthPt: 595.28, heightPt: 841.89 };
      const dims150 = calculateRenderDimensions(a4, 150);
      expect(dims150).toEqual({ width: 1240, height: 1754 });

      // A4 at 72 dpi: 595, 842
      const dims72 = calculateRenderDimensions(a4, 72);
      expect(dims72).toEqual({ width: 595, height: 842 });

      // A4 at 300 dpi: 595.28 / 72 * 300 = 2480.33 -> 2480, 841.89 / 72 * 300 = 3507.88 -> 3508
      const dims300 = calculateRenderDimensions(a4, 300);
      expect(dims300).toEqual({ width: 2480, height: 3508 });
    });
  });

  describe("formatFileSize", () => {
    it("formats bytes, kilobytes, and megabytes", () => {
      expect(formatFileSize(500)).toBe("500 B");
      expect(formatFileSize(2048)).toBe("2.0 KB");
      expect(formatFileSize(2.4 * 1024 * 1024)).toBe("2.4 MB");
    });
  });

  describe("countPagesInIntervals", () => {
    it("returns 0 for zero or negative page count", () => {
      expect(countPagesInIntervals([[1, 5]], 0)).toBe(0);
      expect(countPagesInIntervals([[1, 5]], -1)).toBe(0);
    });

    it("counts matching pages up to pageCount", () => {
      // Intervals 1..3 and 5..6 in an 8-page PDF: 3 + 2 = 5
      expect(
        countPagesInIntervals(
          [
            [1, 3],
            [5, 6],
          ],
          8,
        ),
      ).toBe(5);

      // Intervals extending beyond pageCount are capped
      // Intervals 1..3 and 5..10 in an 8-page PDF: 3 + (8 - 5 + 1) = 7
      expect(
        countPagesInIntervals(
          [
            [1, 3],
            [5, 10],
          ],
          8,
        ),
      ).toBe(7);

      // Intervals starting beyond pageCount are ignored
      expect(countPagesInIntervals([[10, 15]], 8)).toBe(0);
    });
  });

  describe("isPageInIntervals", () => {
    it("determines whether a page number is within intervals", () => {
      const intervals: Array<[number, number]> = [
        [1, 3],
        [6, 6],
      ];
      expect(isPageInIntervals(intervals, 1)).toBe(true);
      expect(isPageInIntervals(intervals, 2)).toBe(true);
      expect(isPageInIntervals(intervals, 3)).toBe(true);
      expect(isPageInIntervals(intervals, 4)).toBe(false);
      expect(isPageInIntervals(intervals, 5)).toBe(false);
      expect(isPageInIntervals(intervals, 6)).toBe(true);
      expect(isPageInIntervals(intervals, 7)).toBe(false);
    });
  });

  describe("formatSkippedSummary", () => {
    it("returns null when skipped is null or all zeros", () => {
      expect(formatSkippedSummary(ja, null)).toBeNull();
      expect(
        formatSkippedSummary(ja, { unsupported: 0, folders: 0, duplicates: 0 }),
      ).toBeNull();
    });

    it("formats non-zero skip counts", () => {
      expect(
        formatSkippedSummary(ja, { unsupported: 2, folders: 0, duplicates: 0 }),
      ).toBe("（スキップ: 非対応 2 件）");

      expect(
        formatSkippedSummary(ja, { unsupported: 1, folders: 2, duplicates: 3 }),
      ).toBe("（スキップ: 非対応 1 件、フォルダ 2 件、重複 3 件）");
    });
  });
});
