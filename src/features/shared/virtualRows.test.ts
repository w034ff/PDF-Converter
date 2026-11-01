import { describe, expect, it } from "vitest";
import { rowOffsets, visibleRows } from "./virtualRows";

describe("rowOffsets", () => {
  it("adds up rows of different heights and a gap after each", () => {
    expect([...rowOffsets(3, (row) => (row + 1) * 10, 2)]).toEqual([
      0, 12, 34, 66,
    ]);
  });

  it("has only the start when there are no rows", () => {
    expect([...rowOffsets(0, () => 10, 2)]).toEqual([0]);
  });
});

describe("visibleRows", () => {
  const offsets = rowOffsets(100, () => 100, 0);

  it("takes the rows in view and the overscan around them", () => {
    expect(visibleRows(offsets, 1000, 300, 2)).toEqual({ first: 8, last: 15 });
  });

  it("starts a row later exactly at its top", () => {
    expect(visibleRows(offsets, 1100, 0, 0)).toEqual({ first: 11, last: 11 });
    expect(visibleRows(offsets, 1099, 0, 0)).toEqual({ first: 10, last: 10 });
  });

  it("stays within the rows there are", () => {
    expect(visibleRows(offsets, -50, 100, 2)).toEqual({ first: 0, last: 3 });
    expect(visibleRows(offsets, 9900, 500, 2)).toEqual({
      first: 97,
      last: 99,
    });
  });

  it("finds rows of different heights", () => {
    const uneven = rowOffsets(4, (row) => (row === 1 ? 1000 : 10), 0);
    expect(visibleRows(uneven, 500, 0, 0)).toEqual({ first: 1, last: 1 });
    expect(visibleRows(uneven, 1015, 0, 0)).toEqual({ first: 2, last: 2 });
  });

  it("draws nothing when there are no rows", () => {
    const { first, last } = visibleRows(
      rowOffsets(0, () => 10, 0),
      0,
      100,
    );
    expect(last).toBeLessThan(first);
  });
});
