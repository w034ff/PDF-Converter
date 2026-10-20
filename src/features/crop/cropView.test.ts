import { describe, expect, it } from "vitest";
import {
  CSS_PX_PER_PT,
  nextZoomPercent,
  pageImageMaxSide,
  rectToCssBox,
  scalePercent,
  viewScale,
} from "./cropView";

const PAGE = { widthPt: 600, heightPt: 800 };

describe("pageImageMaxSide", () => {
  it("rounds the device pixels of the longer side up to a step", () => {
    expect(pageImageMaxSide(300, 1)).toBe(400);
    expect(pageImageMaxSide(400, 1)).toBe(400);
    expect(pageImageMaxSide(401, 1)).toBe(800);
    expect(pageImageMaxSide(1000, 1)).toBe(1200);
    expect(pageImageMaxSide(1600, 1)).toBe(1600);
  });

  it("counts device pixels, not CSS pixels", () => {
    expect(pageImageMaxSide(400, 2)).toBe(800);
    expect(pageImageMaxSide(600, 2)).toBe(1200);
    expect(pageImageMaxSide(1000, 1.5)).toBe(1600);
  });

  it("does not ask for more than the largest step", () => {
    expect(pageImageMaxSide(1200, 1)).toBe(1200);
    expect(pageImageMaxSide(1200, 2)).toBe(2400);
    expect(pageImageMaxSide(1300, 2)).toBe(2400);
    expect(pageImageMaxSide(5000, 2)).toBe(2400);
  });

  it("rounds a fraction of a device pixel up", () => {
    expect(pageImageMaxSide(399.9, 1)).toBe(400);
    expect(pageImageMaxSide(400.1, 1)).toBe(800);
  });
});

describe("nextZoomPercent", () => {
  it("moves to the next step above or below", () => {
    expect(nextZoomPercent(100, "in")).toBe(125);
    expect(nextZoomPercent(100, "out")).toBe(90);
  });

  it("moves from a percentage between two steps to the nearer step that way", () => {
    expect(nextZoomPercent(64, "in")).toBe(67);
    expect(nextZoomPercent(64, "out")).toBe(50);
    expect(nextZoomPercent(10, "in")).toBe(25);
    expect(nextZoomPercent(450, "out")).toBe(400);
  });

  it("stops at the ends", () => {
    expect(nextZoomPercent(400, "in")).toBeNull();
    expect(nextZoomPercent(25, "out")).toBeNull();
    expect(nextZoomPercent(10, "out")).toBeNull();
    expect(nextZoomPercent(450, "in")).toBeNull();
  });
});

describe("viewScale", () => {
  const room = { width: 960, height: 1000 };

  it("takes a fixed scale from its percentage", () => {
    expect(viewScale({ kind: "zoom", percent: 150 }, PAGE, null)).toBe(1.5);
  });

  it("fits the width", () => {
    expect(viewScale({ kind: "fitWidth" }, PAGE, room)).toBeCloseTo(
      960 / (600 * CSS_PX_PER_PT),
    );
  });

  it("fits the whole page by the tighter side", () => {
    // 960 wide leaves the page 1200 / 1000 too tall: the height decides.
    expect(viewScale({ kind: "fitPage" }, PAGE, room)).toBeCloseTo(
      1000 / (800 * CSS_PX_PER_PT),
    );
    expect(
      viewScale({ kind: "fitPage" }, PAGE, { width: 500, height: 5000 }),
    ).toBeCloseTo(500 / (600 * CSS_PX_PER_PT));
  });

  it("waits for the room when the view is fitted", () => {
    expect(viewScale({ kind: "fitWidth" }, PAGE, null)).toBeNull();
    expect(viewScale({ kind: "fitPage" }, PAGE, null)).toBeNull();
  });

  it("shows a scale as a rounded percentage", () => {
    expect(scalePercent(0.644)).toBe(64);
    expect(scalePercent(1)).toBe(100);
  });
});

describe("rectToCssBox", () => {
  it("puts a rectangle of points where the page is drawn", () => {
    const box = rectToCssBox([72, 144, 216, 288], 1);
    expect(box.left).toBeCloseTo(96);
    expect(box.top).toBeCloseTo(192);
    expect(box.width).toBeCloseTo(192);
    expect(box.height).toBeCloseTo(192);
  });

  it("follows the scale", () => {
    const box = rectToCssBox([72, 144, 216, 288], 0.5);
    expect(box.left).toBeCloseTo(48);
    expect(box.top).toBeCloseTo(96);
    expect(box.width).toBeCloseTo(96);
    expect(box.height).toBeCloseTo(96);
  });
});
