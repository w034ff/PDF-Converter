import { describe, expect, it } from "vitest";
import type { RectPt } from "../../state";
import {
  HANDLE_IDS,
  arrowRect,
  drawRect,
  handleCursor,
  handlePoint,
  isDrawnAreaBigEnough,
  moveRect,
  resizeRect,
  type HandleId,
} from "./cropEdit";
import {
  CROP_KEY_STEP_LARGE_PT,
  CROP_KEY_STEP_PT,
  CROP_MIN_AREA_SIDE_PT,
} from "./cropView";

const PAGE = { widthPt: 600, heightPt: 800 };
const RECT: RectPt = [100, 200, 300, 400];

describe("the constants", () => {
  it("are those of the design", () => {
    expect(CROP_MIN_AREA_SIDE_PT).toBe(4);
    expect(CROP_KEY_STEP_PT).toBe(1);
    expect(CROP_KEY_STEP_LARGE_PT).toBe(10);
  });
});

describe("drawRect", () => {
  it("is the rectangle between the two points, whichever way the drag went", () => {
    expect(drawRect({ x: 100, y: 200 }, { x: 300, y: 400 }, PAGE)).toEqual(
      RECT,
    );
    expect(drawRect({ x: 300, y: 400 }, { x: 100, y: 200 }, PAGE)).toEqual(
      RECT,
    );
    expect(drawRect({ x: 100, y: 400 }, { x: 300, y: 200 }, PAGE)).toEqual(
      RECT,
    );
    expect(drawRect({ x: 300, y: 200 }, { x: 100, y: 400 }, PAGE)).toEqual(
      RECT,
    );
  });

  it("stops at every edge of the page", () => {
    expect(drawRect({ x: 100, y: 200 }, { x: 900, y: 1200 }, PAGE)).toEqual([
      100, 200, 600, 800,
    ]);
    expect(drawRect({ x: 100, y: 200 }, { x: -50, y: -70 }, PAGE)).toEqual([
      0, 0, 100, 200,
    ]);
  });

  it("holds a start outside the page inside it too", () => {
    expect(drawRect({ x: -20, y: 900 }, { x: 50, y: 700 }, PAGE)).toEqual([
      0, 700, 50, 800,
    ]);
  });

  it("does not round", () => {
    expect(
      drawRect({ x: 1.25, y: 2.5 }, { x: 10.75, y: 20.125 }, PAGE),
    ).toEqual([1.25, 2.5, 10.75, 20.125]);
  });
});

describe("isDrawnAreaBigEnough", () => {
  it("wants both sides to be at least the minimum", () => {
    expect(isDrawnAreaBigEnough([0, 0, 4, 4])).toBe(true);
    expect(isDrawnAreaBigEnough([0, 0, 3.9, 50])).toBe(false);
    expect(isDrawnAreaBigEnough([0, 0, 50, 3.9])).toBe(false);
    expect(isDrawnAreaBigEnough([10, 10, 10, 10])).toBe(false);
  });
});

describe("handlePoint", () => {
  it("sits on the corners and the middles of the edges", () => {
    const points = Object.fromEntries(
      HANDLE_IDS.map((handle) => [handle, handlePoint(RECT, handle)]),
    );
    expect(points).toEqual({
      nw: { x: 100, y: 200 },
      n: { x: 200, y: 200 },
      ne: { x: 300, y: 200 },
      e: { x: 300, y: 300 },
      se: { x: 300, y: 400 },
      s: { x: 200, y: 400 },
      sw: { x: 100, y: 400 },
      w: { x: 100, y: 300 },
    });
  });
});

describe("handleCursor", () => {
  it("points along the way the handle resizes", () => {
    expect(HANDLE_IDS.map(handleCursor)).toEqual([
      "nwse-resize",
      "ns-resize",
      "nesw-resize",
      "ew-resize",
      "nwse-resize",
      "ns-resize",
      "nesw-resize",
      "ew-resize",
    ]);
  });
});

describe("resizeRect", () => {
  const delta = { x: 30, y: 50 };

  it.each<[HandleId, RectPt]>([
    ["nw", [130, 250, 300, 400]],
    ["n", [100, 250, 300, 400]],
    ["ne", [100, 250, 330, 400]],
    ["e", [100, 200, 330, 400]],
    ["se", [100, 200, 330, 450]],
    ["s", [100, 200, 300, 450]],
    ["sw", [130, 200, 300, 450]],
    ["w", [130, 200, 300, 400]],
  ])("moves only the edges of the %s handle", (handle, expected) => {
    expect(resizeRect(RECT, handle, delta, PAGE)).toEqual(expected);
  });

  it.each<[HandleId, RectPt]>([
    ["nw", [0, 0, 300, 400]],
    ["n", [100, 0, 300, 400]],
    ["ne", [100, 0, 600, 400]],
    ["e", [100, 200, 600, 400]],
    ["se", [100, 200, 600, 800]],
    ["s", [100, 200, 300, 800]],
    ["sw", [0, 200, 300, 800]],
    ["w", [0, 200, 300, 400]],
  ])("stops the %s handle at the page's edges", (handle, expected) => {
    const far = {
      x: handle.includes("w") ? -1000 : 1000,
      y: handle.includes("n") ? -1000 : 1000,
    };
    expect(resizeRect(RECT, handle, far, PAGE)).toEqual(expected);
  });

  it("stops the minimum short of the opposite edge", () => {
    expect(resizeRect(RECT, "e", { x: -500, y: 0 }, PAGE)).toEqual([
      100, 200, 104, 400,
    ]);
    expect(resizeRect(RECT, "s", { x: 0, y: -500 }, PAGE)).toEqual([
      100, 200, 300, 204,
    ]);
    expect(resizeRect(RECT, "w", { x: 500, y: 0 }, PAGE)).toEqual([
      296, 200, 300, 400,
    ]);
    expect(resizeRect(RECT, "n", { x: 0, y: 500 }, PAGE)).toEqual([
      100, 396, 300, 400,
    ]);
    expect(resizeRect(RECT, "nw", { x: 500, y: 500 }, PAGE)).toEqual([
      296, 396, 300, 400,
    ]);
  });

  it("never turns the rectangle inside out", () => {
    for (const handle of HANDLE_IDS) {
      const [x0, y0, x1, y1] = resizeRect(
        RECT,
        handle,
        { x: -9000, y: 9000 },
        PAGE,
      );
      expect(x1 - x0).toBeGreaterThanOrEqual(CROP_MIN_AREA_SIDE_PT);
      expect(y1 - y0).toBeGreaterThanOrEqual(CROP_MIN_AREA_SIDE_PT);
    }
  });

  it("does not shrink an area that is smaller than the minimum, but lets it grow", () => {
    const small: RectPt = [100, 200, 102, 201];
    expect(resizeRect(small, "e", { x: -1, y: 0 }, PAGE)).toEqual(small);
    expect(resizeRect(small, "s", { x: 0, y: -1 }, PAGE)).toEqual(small);
    expect(resizeRect(small, "w", { x: 1, y: 0 }, PAGE)).toEqual(small);
    expect(resizeRect(small, "n", { x: 0, y: 1 }, PAGE)).toEqual(small);
    expect(resizeRect(small, "e", { x: 10, y: 0 }, PAGE)).toEqual([
      100, 200, 112, 201,
    ]);
    expect(resizeRect(small, "n", { x: 0, y: -10 }, PAGE)).toEqual([
      100, 190, 102, 201,
    ]);
  });

  it("does not round", () => {
    expect(resizeRect(RECT, "e", { x: 0.3, y: 0 }, PAGE)[2]).toBeCloseTo(300.3);
  });
});

describe("moveRect", () => {
  it("keeps the size", () => {
    expect(moveRect(RECT, { x: 25, y: -40 }, PAGE)).toEqual([
      125, 160, 325, 360,
    ]);
  });

  it.each<[string, { x: number; y: number }, RectPt]>([
    ["left", { x: -1000, y: 0 }, [0, 200, 200, 400]],
    ["right", { x: 1000, y: 0 }, [400, 200, 600, 400]],
    ["top", { x: 0, y: -1000 }, [100, 0, 300, 200]],
    ["bottom", { x: 0, y: 1000 }, [100, 600, 300, 800]],
  ])("stops at the %s edge of the page", (_edge, delta, expected) => {
    const moved = moveRect(RECT, delta, PAGE);
    expect(moved).toEqual(expected);
    expect(moved[2] - moved[0]).toBe(200);
    expect(moved[3] - moved[1]).toBe(200);
  });
});

describe("arrowRect", () => {
  const plain = { shift: false, alt: false };

  it("moves the area a point, or ten with Shift", () => {
    expect(arrowRect(RECT, "ArrowLeft", plain, PAGE)).toEqual([
      99, 200, 299, 400,
    ]);
    expect(arrowRect(RECT, "ArrowDown", plain, PAGE)).toEqual([
      100, 201, 300, 401,
    ]);
    expect(
      arrowRect(RECT, "ArrowRight", { shift: true, alt: false }, PAGE),
    ).toEqual([110, 200, 310, 400]);
    expect(
      arrowRect(RECT, "ArrowUp", { shift: true, alt: false }, PAGE),
    ).toEqual([100, 190, 300, 390]);
  });

  it("moves the right edge with Alt and ← →, and the bottom edge with Alt and ↑ ↓", () => {
    const alt = { shift: false, alt: true };
    expect(arrowRect(RECT, "ArrowLeft", alt, PAGE)).toEqual([
      100, 200, 299, 400,
    ]);
    expect(arrowRect(RECT, "ArrowRight", alt, PAGE)).toEqual([
      100, 200, 301, 400,
    ]);
    expect(arrowRect(RECT, "ArrowUp", alt, PAGE)).toEqual([100, 200, 300, 399]);
    expect(arrowRect(RECT, "ArrowDown", alt, PAGE)).toEqual([
      100, 200, 300, 401,
    ]);
    expect(
      arrowRect(RECT, "ArrowRight", { shift: true, alt: true }, PAGE),
    ).toEqual([100, 200, 310, 400]);
  });

  it("stops at the page's edge and at the minimum", () => {
    const atEdge: RectPt = [0, 0, 600, 800];
    expect(arrowRect(atEdge, "ArrowLeft", plain, PAGE)).toEqual(atEdge);
    expect(
      arrowRect(atEdge, "ArrowRight", { shift: false, alt: true }, PAGE),
    ).toEqual(atEdge);
    const thin: RectPt = [10, 10, 14, 50];
    expect(
      arrowRect(thin, "ArrowLeft", { shift: true, alt: true }, PAGE),
    ).toEqual(thin);
  });
});
