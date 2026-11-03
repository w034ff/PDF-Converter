import { describe, expect, it } from "vitest";
import { en } from "../../i18n/en";
import { ja } from "../../i18n/ja";
import { formatSkippedDetails, formatSkippedSummary } from "./skipped";

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
    ).toBe("（対象外：非対応 2 件）");

    expect(
      formatSkippedSummary(ja, { unsupported: 1, folders: 2, duplicates: 3 }),
    ).toBe("（対象外：非対応 1 件、フォルダ 2 件、重複 3 件）");
  });
});

describe("formatSkippedDetails", () => {
  it("says one folder and one duplicate in the singular", () => {
    expect(
      formatSkippedDetails(en, { unsupported: 1, folders: 1, duplicates: 1 }),
    ).toBe("1 unsupported, 1 folder, 1 duplicate");
  });
});
