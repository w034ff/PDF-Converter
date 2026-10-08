import { describe, expect, it } from "vitest";
import { en, ja } from "../../i18n";
import {
  formatJobSummary,
  jobOutcome,
  singlePdfPageCounts,
} from "./jobSummary";

const NOTHING = {
  succeeded: 0,
  failed: 0,
  noPages: 0,
  unprocessed: 0,
  cancelled: false,
};

describe("formatJobSummary", () => {
  describe("counting items", () => {
    it("lists the counts that are not zero", () => {
      expect(
        formatJobSummary(ja, {
          ...NOTHING,
          succeeded: 3,
          failed: 2,
          noPages: 1,
        }),
      ).toBe(
        "変換が終わりました：成功 3 件 · 失敗 2 件 · 対象のページなし 1 件",
      );
    });

    it("does not list zero successes", () => {
      expect(formatJobSummary(ja, { ...NOTHING, failed: 1 })).toBe(
        "変換が終わりました：失敗 1 件",
      );
      expect(
        formatJobSummary(
          ja,
          { ...NOTHING, failed: 2 },
          { kind: "items", partial: 2 },
        ),
      ).toBe("変換が終わりました：一部失敗 2 件");
    });

    it("lists partly failed PDFs on their own instead of as failures", () => {
      expect(
        formatJobSummary(
          ja,
          { ...NOTHING, succeeded: 1, failed: 3 },
          { kind: "items", partial: 1 },
        ),
      ).toBe("変換が終わりました：成功 1 件 · 失敗 2 件 · 一部失敗 1 件");
    });

    it("says a cancelled job was cancelled and counts what was left", () => {
      expect(
        formatJobSummary(en, {
          ...NOTHING,
          succeeded: 2,
          unprocessed: 4,
          cancelled: true,
        }),
      ).toBe("Cancelled: 2 succeeded · 4 not processed");
    });

    it("does not list zero successes of a cancelled job either", () => {
      expect(
        formatJobSummary(ja, { ...NOTHING, unprocessed: 3, cancelled: true }),
      ).toBe("キャンセルしました：未処理 3 件");
    });
  });

  describe("counting the pages of one PDF", () => {
    const pages = (saved: number, failed: number, unprocessed: number) =>
      ({ kind: "pages", saved, failed, unprocessed }) as const;

    it("says how many pages were saved", () => {
      expect(formatJobSummary(ja, NOTHING, pages(3, 0, 0))).toBe(
        "変換が終わりました：3 ページ保存しました",
      );
    });

    it("adds the pages that failed", () => {
      expect(formatJobSummary(ja, NOTHING, pages(2, 1, 0))).toBe(
        "変換が終わりました：2 ページ保存しました · 失敗 1 ページ",
      );
    });

    it("adds the pages that were not reached, for a cancel or a stop", () => {
      expect(
        formatJobSummary(
          ja,
          { ...NOTHING, cancelled: true },
          pages(18, 0, 182),
        ),
      ).toBe("キャンセルしました：18 ページ保存しました · 未処理 182 ページ");
      expect(formatJobSummary(ja, NOTHING, pages(5, 1, 94))).toBe(
        "変換が終わりました：5 ページ保存しました · 失敗 1 ページ · 未処理 94 ページ",
      );
    });

    it("lists only the failure when no page was saved", () => {
      expect(formatJobSummary(ja, NOTHING, pages(0, 3, 0))).toBe(
        "変換が終わりました：失敗 3 ページ",
      );
    });

    it("uses the singular for one page in English", () => {
      expect(formatJobSummary(en, NOTHING, pages(1, 1, 1))).toBe(
        "Conversion finished: 1 page saved · 1 page failed · 1 page not processed",
      );
      expect(formatJobSummary(en, NOTHING, pages(2, 3, 4))).toBe(
        "Conversion finished: 2 pages saved · 3 pages failed · 4 pages not processed",
      );
    });
  });

  describe("counting the pages of the merged PDF", () => {
    it("says how many pages the PDF has, without its name", () => {
      expect(
        formatJobSummary(
          ja,
          { ...NOTHING, succeeded: 10 },
          { kind: "mergedPdf", pages: 10, failedImages: 0 },
        ),
      ).toBe("変換が終わりました：10 ページの PDF を保存しました");
    });

    it("adds the images that failed", () => {
      expect(
        formatJobSummary(
          ja,
          { ...NOTHING, succeeded: 10, failed: 1 },
          { kind: "mergedPdf", pages: 10, failedImages: 1 },
        ),
      ).toBe("変換が終わりました：10 ページの PDF を保存しました · 失敗 1 枚");
    });

    it("reads in English, with one page too", () => {
      expect(
        formatJobSummary(
          en,
          { ...NOTHING, succeeded: 1, failed: 2 },
          { kind: "mergedPdf", pages: 1, failedImages: 2 },
        ),
      ).toBe("Conversion finished: Saved a 1-page PDF · 2 failed");
      expect(
        formatJobSummary(
          en,
          { ...NOTHING, succeeded: 10 },
          { kind: "mergedPdf", pages: 10, failedImages: 0 },
        ),
      ).toBe("Conversion finished: Saved a 10-page PDF");
    });
  });
});

describe("singlePdfPageCounts", () => {
  it("counts the selected pages neither saved nor failed as not processed", () => {
    expect(
      singlePdfPageCounts(
        { id: 1, status: "cancelled", outputs: ["a", "b"] },
        200,
      ),
    ).toEqual({ kind: "pages", saved: 2, failed: 0, unprocessed: 198 });
    expect(
      singlePdfPageCounts(
        {
          id: 1,
          status: "partial",
          outputs: ["a", "b"],
          failedPages: [3],
        },
        5,
      ),
    ).toEqual({ kind: "pages", saved: 2, failed: 1, unprocessed: 2 });
  });

  it("counts every selected page as not processed before any result", () => {
    expect(singlePdfPageCounts(undefined, 7)).toEqual({
      kind: "pages",
      saved: 0,
      failed: 0,
      unprocessed: 7,
    });
  });
});

describe("jobOutcome", () => {
  const base = {
    succeeded: 0,
    failed: 0,
    noPages: 0,
    unprocessed: 0,
    cancelled: false,
  };

  it("is a success when every item converted", () => {
    expect(jobOutcome({ ...base, succeeded: 3 })).toBe("success");
  });

  it("is a failure when any item failed, even when the run was cancelled", () => {
    expect(jobOutcome({ ...base, succeeded: 2, failed: 1 })).toBe("failure");
    expect(jobOutcome({ ...base, failed: 1, cancelled: true })).toBe("failure");
  });

  it("is neutral for a cancel or a run that converted nothing", () => {
    expect(jobOutcome({ ...base, succeeded: 1, cancelled: true })).toBe(
      "neutral",
    );
    expect(jobOutcome({ ...base, succeeded: 1, noPages: 1 })).toBe("neutral");
    expect(jobOutcome({ ...base, unprocessed: 2 })).toBe("neutral");
  });
});
