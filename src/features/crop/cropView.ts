import type { PageSizePt } from "../../ipc";
import {
  CROP_PAGE_SIDE_STEPS,
  CROP_ZOOM_STEPS,
  PAGE_VIEW_MAX_SIDE,
  type CropViewSize,
} from "../../state";

/** CSS pixels in a point: the actual size (100 %) is 96 dpi (design §10.1). */
export const CSS_PX_PER_PT = 96 / 72;

const PERCENT = 100;

/** The room the page has: the content box of the scrolling view, in CSS pixels. */
export interface ViewportSize {
  width: number;
  height: number;
}

/** The scale at which the page fills the view's width, or all of the view. */
function fitScale(
  kind: "fitWidth" | "fitPage",
  page: PageSizePt,
  viewport: ViewportSize,
): number {
  const byWidth = viewport.width / (page.widthPt * CSS_PX_PER_PT);
  if (kind === "fitWidth") {
    return byWidth;
  }
  const byHeight = viewport.height / (page.heightPt * CSS_PX_PER_PT);
  return Math.min(byWidth, byHeight);
}

/**
 * The scale (1 is the actual size) the page is shown at in `view`, or `null`
 * while a fitted view waits for the size of the room (`viewport`).
 */
export function viewScale(
  view: CropViewSize,
  page: PageSizePt,
  viewport: ViewportSize | null,
): number | null {
  if (view.kind === "zoom") {
    return view.percent / PERCENT;
  }
  return viewport === null ? null : fitScale(view.kind, page, viewport);
}

/** A scale as the percentage the toolbar shows. */
export function scalePercent(scale: number): number {
  return Math.round(scale * PERCENT);
}

/**
 * The step `−` or `＋` moves to from `percent`: the next of
 * `CROP_ZOOM_STEPS` below or above it. `null` when there is no step that way.
 */
export function nextZoomPercent(
  percent: number,
  direction: "in" | "out",
): number | null {
  if (direction === "in") {
    return CROP_ZOOM_STEPS.find((step) => step > percent) ?? null;
  }
  return CROP_ZOOM_STEPS.filter((step) => step < percent).at(-1) ?? null;
}

/**
 * The `maxSide` to ask `get_thumbnail` for when the longer side of the page
 * is `longSideCssPx` on a screen with `devicePixelRatio`: the smallest of
 * `CROP_PAGE_SIDE_STEPS` that holds every device pixel, or the largest step
 * when none does (design §10.1).
 */
export function pageImageMaxSide(
  longSideCssPx: number,
  devicePixelRatio: number,
): number {
  const wanted = Math.ceil(longSideCssPx * devicePixelRatio);
  return (
    CROP_PAGE_SIDE_STEPS.find((side) => side >= wanted) ?? PAGE_VIEW_MAX_SIDE
  );
}

/** Where a rectangle of the page lies on the page as drawn, in CSS pixels. */
export interface CssBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Turns `[x0, y0, x1, y1]` in points into a box at `scale`. */
export function rectToCssBox(
  rectPt: readonly [number, number, number, number],
  scale: number,
): CssBox {
  const pxPerPt = CSS_PX_PER_PT * scale;
  const [x0, y0, x1, y1] = rectPt;
  return {
    left: x0 * pxPerPt,
    top: y0 * pxPerPt,
    width: (x1 - x0) * pxPerPt,
    height: (y1 - y0) * pxPerPt,
  };
}
