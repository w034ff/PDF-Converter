import { describe, expect, it } from "vitest";
import type { OrganizePdf } from "../../ipc";
import type { OrganizePage } from "../../state";
import { bytesToMegabytes, estimateOutputBytes } from "./estimate";

/**
 * One row: `[fileBytes, usedPages, pageCount]` of each PDF in the output, and
 * the estimate. The same rows are in the test of `estimate_output_bytes` in
 * `src-tauri/src/organize.rs` (`the_estimates_of_the_table_the_screen_tests_too`);
 * a row changed here is changed there.
 */
const SHARED_TABLE: readonly {
  sources: readonly (readonly [number, number, number])[];
  estimate: bigint;
}[] = [
  { sources: [[1000, 1, 3]], estimate: 334n },
  { sources: [[1000, 3, 3]], estimate: 1000n },
  { sources: [[1000, 0, 3]], estimate: 0n },
  {
    sources: [
      [10, 1, 4],
      [7, 2, 3],
    ],
    estimate: 8n,
  },
  { sources: [[500, 2, 0]], estimate: 0n },
  {
    sources: [[9_007_199_254_740_991, 3, 4]],
    estimate: 6_755_399_441_055_744n,
  },
];

function build(sources: readonly (readonly [number, number, number])[]) {
  const pdfs: OrganizePdf[] = [];
  const pages: OrganizePage[] = [];
  sources.forEach(([fileBytes, used, pageCount], index) => {
    const id = index + 1;
    pdfs.push({
      id,
      name: `${id}.pdf`,
      pageCount,
      pages: [],
      fileBytes,
      encrypted: false,
      error: null,
    });
    for (let page = 1; page <= used; page += 1) {
      pages.push({ id, page, rotation: 0 });
    }
  });
  return { pdfs, pages };
}

describe("estimateOutputBytes", () => {
  it.each(SHARED_TABLE)(
    "is $estimate for $sources",
    ({ sources, estimate }) => {
      const { pdfs, pages } = build(sources);

      expect(estimateOutputBytes(pages, pdfs)).toBe(estimate);
    },
  );

  it("rounds up for each PDF on its own", () => {
    const { pdfs, pages } = build([
      [10, 1, 3],
      [10, 1, 3],
    ]);

    // 4 + 4, not ceil(6.66).
    expect(estimateOutputBytes(pages, pdfs)).toBe(8n);
  });

  it("counts the pages in the grid, whichever order they are in and however they are turned", () => {
    const { pdfs } = build([[300, 0, 3]]);
    const pages: OrganizePage[] = [
      { id: 1, page: 3, rotation: 90 },
      { id: 1, page: 1, rotation: 0 },
    ];

    expect(estimateOutputBytes(pages, pdfs)).toBe(200n);
  });

  it("leaves out a PDF none of whose pages is in the grid, and one with an error", () => {
    const { pdfs, pages } = build([
      [1000, 2, 4],
      [5000, 0, 5],
    ]);
    pdfs.push({
      id: 3,
      name: "waiting.pdf",
      pageCount: 0,
      pages: [],
      fileBytes: 9999,
      encrypted: true,
      error: { code: "PasswordProtected", detail: null },
    });

    expect(estimateOutputBytes(pages, pdfs)).toBe(500n);
  });

  it("is made in BigInt, where Number would lose bytes", () => {
    const { pdfs, pages } = build([[Number.MAX_SAFE_INTEGER, 3, 4]]);
    const asNumber = Math.ceil((Number.MAX_SAFE_INTEGER * 3) / 4);

    expect(estimateOutputBytes(pages, pdfs)).toBe(6_755_399_441_055_744n);
    // The product, 27021597764222973, is past what a Number holds exactly.
    expect(BigInt(asNumber)).not.toBe(6_755_399_441_055_744n);
  });

  it("is nothing for no pages", () => {
    expect(estimateOutputBytes([], [])).toBe(0n);
  });
});

describe("bytesToMegabytes", () => {
  const MB = 1024n * 1024n;

  it("rounds up to a whole MB of 1,048,576 bytes", () => {
    expect(bytesToMegabytes(0n)).toBe(0);
    expect(bytesToMegabytes(1n)).toBe(1);
    expect(bytesToMegabytes(MB - 1n)).toBe(1);
    expect(bytesToMegabytes(MB)).toBe(1);
    expect(bytesToMegabytes(MB + 1n)).toBe(2);
    expect(bytesToMegabytes(613n * MB)).toBe(613);
  });
});
