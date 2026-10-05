import { describe, expect, it } from "vitest";
import { en, ja } from "../../i18n";
import { formatJobSummary } from "./jobSummary";

describe("formatJobSummary", () => {
  it("lists successes and only the other counts that are not zero", () => {
    expect(
      formatJobSummary(ja, {
        succeeded: 3,
        failed: 2,
        noPages: 1,
        unprocessed: 0,
        cancelled: false,
      }),
    ).toBe("変換が終わりました：成功 3 件 · 失敗 2 件 · 対象のページなし 1 件");
  });

  it("lists zero successes", () => {
    expect(
      formatJobSummary(ja, {
        succeeded: 0,
        failed: 1,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      }),
    ).toBe("変換が終わりました：成功 0 件 · 失敗 1 件");
  });

  it("lists partly failed PDFs on their own instead of as failures", () => {
    expect(
      formatJobSummary(
        ja,
        {
          succeeded: 1,
          failed: 3,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        },
        1,
      ),
    ).toBe("変換が終わりました：成功 1 件 · 失敗 2 件 · 一部失敗 1 件");
  });

  it("says a cancelled job was cancelled and counts what was left", () => {
    expect(
      formatJobSummary(en, {
        succeeded: 2,
        failed: 0,
        noPages: 0,
        unprocessed: 4,
        cancelled: true,
      }),
    ).toBe("Cancelled: 2 succeeded · 4 not processed");
  });
});
