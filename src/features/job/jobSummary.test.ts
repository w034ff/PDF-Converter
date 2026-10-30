import { describe, expect, it } from "vitest";
import { en, ja } from "../../i18n";
import { initialJobState, type JobState } from "../../state";
import {
  formatJobSummary,
  jobOutcome,
  organizeSummaryShape,
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

  describe("a merged PDF that was not saved", () => {
    const CANCELLED = { ...NOTHING, unprocessed: 2, cancelled: true };

    it("says no PDF was saved, with no counts", () => {
      expect(
        formatJobSummary(ja, CANCELLED, { kind: "mergedPdfNotSaved" }),
      ).toBe("キャンセルしました：PDF は保存していません");
    });

    it("reads in English", () => {
      expect(
        formatJobSummary(en, CANCELLED, { kind: "mergedPdfNotSaved" }),
      ).toBe("Cancelled: the PDF was not saved");
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

  it("counts a cancel after a failed page as reached", () => {
    // Pages 1-5 selected: 1 saved, 2 failed, 3 saved, then cancelled.
    expect(
      singlePdfPageCounts(
        {
          id: 1,
          status: "cancelled",
          outputs: ["a", "b"],
          failedPages: [2],
        },
        5,
      ),
    ).toEqual({ kind: "pages", saved: 2, failed: 1, unprocessed: 2 });
  });

  it("counts all selected pages as failed when the PDF failed as a whole", () => {
    expect(
      singlePdfPageCounts(
        {
          id: 1,
          status: "failed",
          outputs: [],
          error: { code: "PdfOpenFailed", detail: null },
        },
        200,
      ),
    ).toEqual({ kind: "pages", saved: 0, failed: 200, unprocessed: 0 });
  });

  it("does not take a cancel for a failure of the whole PDF", () => {
    expect(
      singlePdfPageCounts({ id: 1, status: "cancelled", outputs: [] }, 4),
    ).toEqual({ kind: "pages", saved: 0, failed: 0, unprocessed: 4 });
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

describe("formatJobSummary for all pages' candidates", () => {
  const detectAll = (added: number, failed = 0, unprocessed = 0) =>
    ({ kind: "detectAll", added, failed, unprocessed }) as const;

  it("counts the areas added and the pages that failed", () => {
    expect(
      formatJobSummary(
        ja,
        { ...NOTHING, succeeded: 7, failed: 1 },
        detectAll(8, 1),
      ),
    ).toBe("全ページの候補を探しました：範囲を 8 個追加 · 失敗 1 ページ");
  });

  it("counts the pages not reached when it was cancelled", () => {
    expect(
      formatJobSummary(
        ja,
        { ...NOTHING, succeeded: 2, unprocessed: 40, cancelled: true },
        detectAll(3, 0, 40),
      ),
    ).toBe("キャンセルしました：範囲を 3 個追加 · 未処理 40 ページ");
  });

  it("says no area was added, and leaves out a count of zero", () => {
    expect(
      formatJobSummary(ja, { ...NOTHING, succeeded: 4 }, detectAll(0)),
    ).toBe("全ページの候補を探しました：追加した範囲はありません");
    expect(
      formatJobSummary(ja, { ...NOTHING, cancelled: true }, detectAll(0, 0, 5)),
    ).toBe("キャンセルしました：追加した範囲はありません · 未処理 5 ページ");
  });

  it("says the counts in the singular and the plural in English", () => {
    expect(
      formatJobSummary(
        en,
        { ...NOTHING, succeeded: 1, failed: 1 },
        detectAll(1, 1),
      ),
    ).toBe("Searched all pages for suggestions: added 1 area · 1 page failed");
    expect(
      formatJobSummary(
        en,
        { ...NOTHING, succeeded: 3, failed: 2 },
        detectAll(5, 2),
      ),
    ).toBe(
      "Searched all pages for suggestions: added 5 areas · 2 pages failed",
    );
    expect(
      formatJobSummary(
        en,
        { ...NOTHING, cancelled: true, unprocessed: 4 },
        detectAll(0, 0, 4),
      ),
    ).toBe("Cancelled: no areas were added · 4 pages not processed");
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

describe("the summary of the organize tab's save in parts", () => {
  const PDFS = [
    {
      id: 7,
      name: "book.pdf",
      pageCount: 2,
      pages: [],
      fileBytes: 10,
      encrypted: false,
      error: null,
      colorIndex: 0,
    },
  ];
  const FAILED = { code: "PdfOpenFailed", detail: null } as const;

  function split(overrides: Partial<JobState>): JobState {
    return {
      ...initialJobState,
      kind: "organize",
      organizeOutput: "split",
      phase: "finished",
      ...overrides,
    };
  }

  it("counts the PDFs saved when all of them were", () => {
    const finished = { ...NOTHING, succeeded: 3 };
    const shape = organizeSummaryShape(split({ finished }), PDFS, "ja");

    expect(shape).toEqual({ kind: "organizedParts", saved: 3 });
    expect(formatJobSummary(ja, finished, shape)).toBe(
      "3 個の PDF を保存しました",
    );
    expect(formatJobSummary(en, finished, shape)).toBe("Saved 3 PDFs");
    expect(
      formatJobSummary(
        en,
        { ...NOTHING, succeeded: 1 },
        { kind: "organizedParts", saved: 1 },
      ),
    ).toBe("Saved 1 PDF");
  });

  it("tells the first output that failed, by its number and the PDF to blame", () => {
    const finished = { ...NOTHING, succeeded: 1, failed: 2 };
    const job = split({
      finished,
      results: {
        1: { id: 1, status: "ok", outputs: ["a.pdf"] },
        3: { id: 3, status: "failed", outputs: [], error: FAILED, sourceId: 7 },
        2: { id: 2, status: "failed", outputs: [], error: FAILED },
      },
    });

    const shape = organizeSummaryShape(job, PDFS, "ja");

    expect(shape).toMatchObject({
      kind: "items",
      firstFailure: { part: 2, name: null },
    });
    expect(formatJobSummary(ja, finished, shape)).toMatch(
      /^変換が終わりました：成功 1 件 · 失敗 2 件（2 番目の PDF: .+）$/,
    );
    expect(
      formatJobSummary(
        ja,
        finished,
        organizeSummaryShape(
          split({
            finished,
            results: {
              3: {
                id: 3,
                status: "failed",
                outputs: [],
                error: FAILED,
                sourceId: 7,
              },
            },
          }),
          PDFS,
          "ja",
        ),
      ),
    ).toMatch(/（3 番目の PDF: book\.pdf: .+）$/);
    expect(formatJobSummary(en, finished, shape)).toMatch(
      /^Conversion finished: 1 succeeded · 2 failed \(PDF 2: .+\)$/,
    );
  });

  it("counts what was saved and left after Cancel, with no claim that nothing was saved", () => {
    const finished = {
      ...NOTHING,
      succeeded: 1,
      unprocessed: 2,
      cancelled: true,
    };
    const shape = organizeSummaryShape(split({ finished }), PDFS, "ja");

    expect(shape).toEqual({ kind: "items", partial: 0 });
    expect(formatJobSummary(ja, finished, shape)).toBe(
      "キャンセルしました：成功 1 件 · 未処理 2 件",
    );
  });

  it("is told by how the save started: one PDF is told as one PDF", () => {
    const finished = { ...NOTHING, succeeded: 1 };
    const shape = organizeSummaryShape(
      { ...split({ finished }), organizeOutput: "single" },
      PDFS,
      "ja",
    );

    expect(shape.kind).toBe("organizedPdf");
  });
});
