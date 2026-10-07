import { describe, expect, it } from "vitest";
import { ja } from "../../i18n/ja";
import {
  calculateRenderDimensions,
  clampIntervals,
  countPagesInIntervals,
  formatFileSize,
  formatPaperSize,
  formatSkippedSummary,
  intervalsToRangeText,
  isPageInIntervals,
  togglePageRangeInIntervals,
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

  describe("clampIntervals", () => {
    it("returns empty array for non-positive page count or empty intervals", () => {
      expect(clampIntervals([[1, 5]], 0)).toEqual([]);
      expect(clampIntervals([[1, 5]], -1)).toEqual([]);
      expect(clampIntervals([], 10)).toEqual([]);
    });

    it("clamps intervals exceeding pageCount without expanding pages", () => {
      // 1-4000000000 clamped to 10 pages without per-page iteration explosion
      expect(clampIntervals([[1, 4000000000]], 10)).toEqual([[1, 10]]);
      expect(
        clampIntervals(
          [
            [1, 5],
            [8, 20],
          ],
          10,
        ),
      ).toEqual([
        [1, 5],
        [8, 10],
      ]);
    });

    it("drops intervals completely outside 1..pageCount", () => {
      expect(clampIntervals([[15, 20]], 10)).toEqual([]);
      expect(clampIntervals([[-5, 0]], 10)).toEqual([]);
    });
  });

  describe("intervalsToRangeText", () => {
    it("formats empty intervals as empty string", () => {
      expect(intervalsToRangeText([])).toBe("");
    });

    it("formats single-page intervals and multi-page ranges correctly", () => {
      expect(intervalsToRangeText([[5, 5]])).toBe("5");
      expect(intervalsToRangeText([[1, 2]])).toBe("1-2");
      expect(
        intervalsToRangeText([
          [1, 3],
          [5, 5],
          [8, 10],
        ]),
      ).toBe("1-3, 5, 8-10");
    });
  });

  describe("togglePageRangeInIntervals", () => {
    it("adds a page to an empty selection", () => {
      const next = togglePageRangeInIntervals([], 10, 5, 5, true);
      expect(next).toEqual([[5, 5]]);
      expect(intervalsToRangeText(next)).toBe("5");
    });

    it("removes a page and results in empty intervals when all are removed", () => {
      const next = togglePageRangeInIntervals([[5, 5]], 10, 5, 5, false);
      expect(next).toEqual([]);
      expect(intervalsToRangeText(next)).toBe("");
    });

    it("merges adjacent intervals (1-3, 5 with 4 added becomes 1-5)", () => {
      const initial: Array<[number, number]> = [
        [1, 3],
        [5, 5],
      ];
      const next = togglePageRangeInIntervals(initial, 10, 4, 4, true);
      expect(next).toEqual([[1, 5]]);
      expect(intervalsToRangeText(next)).toBe("1-5");
    });

    it("splits an interval (1-5 with 3 removed becomes 1-2, 4-5)", () => {
      const initial: Array<[number, number]> = [[1, 5]];
      const next = togglePageRangeInIntervals(initial, 10, 3, 3, false);
      expect(next).toEqual([
        [1, 2],
        [4, 5],
      ]);
      expect(intervalsToRangeText(next)).toBe("1-2, 4-5");
    });

    it("clamps intervals exceeding pageCount (1-4000000000) and toggles within page count", () => {
      // PDF has 10 pages, range is 1-4000000000, remove page 4
      const initial: Array<[number, number]> = [[1, 4000000000]];
      const next = togglePageRangeInIntervals(initial, 10, 4, 4, false);
      expect(next).toEqual([
        [1, 3],
        [5, 10],
      ]);
      expect(intervalsToRangeText(next)).toBe("1-3, 5-10");
    });

    it("handles range selection when anchor is before target", () => {
      // Initially page 1 is selected, select 3 to 7
      const next = togglePageRangeInIntervals([[1, 1]], 10, 3, 7, true);
      expect(next).toEqual([
        [1, 1],
        [3, 7],
      ]);
      expect(intervalsToRangeText(next)).toBe("1, 3-7");
    });

    it("handles range selection when anchor is after target (e.g. anchor 7, target 3)", () => {
      // Remove 3..7 from 1-10 when anchor is 7 and target is 3
      const next = togglePageRangeInIntervals([[1, 10]], 10, 7, 3, false);
      expect(next).toEqual([
        [1, 2],
        [8, 10],
      ]);
      expect(intervalsToRangeText(next)).toBe("1-2, 8-10");

      // Add 3..7 to empty when anchor is 7 and target is 3
      const added = togglePageRangeInIntervals([], 10, 7, 3, true);
      expect(added).toEqual([[3, 7]]);
      expect(intervalsToRangeText(added)).toBe("3-7");
    });
  });
});
