import type { PageSizePt } from "../../ipc";
import type { RectPt } from "../../state";
import {
  CROP_EDGE_HANDLE_MIN_SIDE_PX,
  CROP_KEY_STEP_LARGE_PT,
  CROP_KEY_STEP_PT,
  CROP_MIN_AREA_SIDE_PT,
} from "./cropView";

/** A point of the page, in points from its top left. */
export interface PointPt {
  x: number;
  y: number;
}

/** The way a pointer or an arrow key moved, in points. */
export interface DeltaPt {
  x: number;
  y: number;
}

/** The handles of an area: four corners and four edge midpoints, named by compass point. */
export type HandleId = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

export const HANDLE_IDS: readonly HandleId[] = [
  "nw",
  "n",
  "ne",
  "e",
  "se",
  "s",
  "sw",
  "w",
];

/**
 * The handles an area of `widthPx` × `heightPx` on screen shows: the corners
 * always, and the middle of the top and bottom edges or of the left and right
 * ones only while the area is wide or tall enough for them.
 */
export function shownHandles(widthPx: number, heightPx: number): HandleId[] {
  return HANDLE_IDS.filter((handle) => {
    const edges = movedEdges(handle);
    if (edges.x === 0) {
      return widthPx >= CROP_EDGE_HANDLE_MIN_SIDE_PX;
    }
    if (edges.y === 0) {
      return heightPx >= CROP_EDGE_HANDLE_MIN_SIDE_PX;
    }
    return true;
  });
}

/** Which edges a handle moves: the sign says which side of the area it is on. */
function movedEdges(handle: HandleId): { x: -1 | 0 | 1; y: -1 | 0 | 1 } {
  return {
    x: handle.includes("w") ? -1 : handle.includes("e") ? 1 : 0,
    y: handle.includes("n") ? -1 : handle.includes("s") ? 1 : 0,
  };
}

/** Where a handle sits on `rect`. */
export function handlePoint(rect: RectPt, handle: HandleId): PointPt {
  const edges = movedEdges(handle);
  const [x0, y0, x1, y1] = rect;
  return {
    x: edges.x < 0 ? x0 : edges.x > 0 ? x1 : (x0 + x1) / 2,
    y: edges.y < 0 ? y0 : edges.y > 0 ? y1 : (y0 + y1) / 2,
  };
}

/** The cursor that tells how a handle resizes. */
export function handleCursor(handle: HandleId): string {
  switch (handle) {
    case "nw":
    case "se":
      return "nwse-resize";
    case "ne":
    case "sw":
      return "nesw-resize";
    case "n":
    case "s":
      return "ns-resize";
    case "e":
    case "w":
      return "ew-resize";
  }
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(high, Math.max(low, value));
}

function clampToPage(point: PointPt, page: PageSizePt): PointPt {
  return {
    x: clamp(point.x, 0, page.widthPt),
    y: clamp(point.y, 0, page.heightPt),
  };
}

/**
 * The rectangle between two points of a drag, each held inside the page, with
 * its corners in order wherever the drag went.
 */
export function drawRect(
  start: PointPt,
  current: PointPt,
  page: PageSizePt,
): RectPt {
  const a = clampToPage(start, page);
  const b = clampToPage(current, page);
  return [
    Math.min(a.x, b.x),
    Math.min(a.y, b.y),
    Math.max(a.x, b.x),
    Math.max(a.y, b.y),
  ];
}

/** Whether a drawn rectangle is big enough to be an area. */
export function isDrawnAreaBigEnough(rect: RectPt): boolean {
  return (
    rect[2] - rect[0] >= CROP_MIN_AREA_SIDE_PT &&
    rect[3] - rect[1] >= CROP_MIN_AREA_SIDE_PT
  );
}

/**
 * Where an edge ends up when it is moved by `delta`. The edge stops at the
 * page and at `limit`, which is as near as it may come to the opposite edge;
 * an edge that is already past `limit` does not move further that way but can
 * move back.
 */
function moveLowEdge(
  edge: number,
  opposite: number,
  delta: number,
  pageEdge: number,
): number {
  const limit = Math.max(opposite - CROP_MIN_AREA_SIDE_PT, edge);
  return clamp(edge + delta, pageEdge, limit);
}

function moveHighEdge(
  edge: number,
  opposite: number,
  delta: number,
  pageEdge: number,
): number {
  const limit = Math.min(opposite + CROP_MIN_AREA_SIDE_PT, edge);
  return clamp(edge + delta, limit, pageEdge);
}

/**
 * `rect` with the edges `handle` moves shifted by `delta`: at the page's edge
 * and `CROP_MIN_AREA_SIDE_PT` short of the opposite edge they stop, and they
 * never pass the opposite edge.
 */
export function resizeRect(
  rect: RectPt,
  handle: HandleId,
  delta: DeltaPt,
  page: PageSizePt,
): RectPt {
  const edges = movedEdges(handle);
  const [x0, y0, x1, y1] = rect;
  return [
    edges.x < 0 ? moveLowEdge(x0, x1, delta.x, 0) : x0,
    edges.y < 0 ? moveLowEdge(y0, y1, delta.y, 0) : y0,
    edges.x > 0 ? moveHighEdge(x1, x0, delta.x, page.widthPt) : x1,
    edges.y > 0 ? moveHighEdge(y1, y0, delta.y, page.heightPt) : y1,
  ];
}

/** `rect` shifted by `delta`, keeping its size and stopping at the page's edges. */
export function moveRect(
  rect: RectPt,
  delta: DeltaPt,
  page: PageSizePt,
): RectPt {
  const [x0, y0, x1, y1] = rect;
  // An area that sticks out of the page moves back in rather than further out.
  const dx = clamp(delta.x, -x0, Math.max(page.widthPt - x1, -x0));
  const dy = clamp(delta.y, -y0, Math.max(page.heightPt - y1, -y0));
  return [x0 + dx, y0 + dy, x1 + dx, y1 + dy];
}

/** The arrow keys that move or resize an area. */
export type ArrowKey = "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown";

export function isArrowKey(key: string): key is ArrowKey {
  return (
    key === "ArrowLeft" ||
    key === "ArrowRight" ||
    key === "ArrowUp" ||
    key === "ArrowDown"
  );
}

export interface ArrowModifiers {
  /** Moves by `CROP_KEY_STEP_LARGE_PT` instead of `CROP_KEY_STEP_PT`. */
  shift: boolean;
  /** Moves the right or bottom edge instead of the whole area. */
  alt: boolean;
}

const ARROW_DIRECTIONS: Record<ArrowKey, DeltaPt> = {
  ArrowLeft: { x: -1, y: 0 },
  ArrowRight: { x: 1, y: 0 },
  ArrowUp: { x: 0, y: -1 },
  ArrowDown: { x: 0, y: 1 },
};

/**
 * `rect` after an arrow key (design §10.1): the whole area moves, or with Alt
 * the right edge (← and →) or the bottom edge (↑ and ↓), by the same rules as
 * a drag.
 */
export function arrowRect(
  rect: RectPt,
  key: ArrowKey,
  modifiers: ArrowModifiers,
  page: PageSizePt,
): RectPt {
  const step = modifiers.shift ? CROP_KEY_STEP_LARGE_PT : CROP_KEY_STEP_PT;
  const direction = ARROW_DIRECTIONS[key];
  const delta = { x: direction.x * step, y: direction.y * step };
  if (!modifiers.alt) {
    return moveRect(rect, delta, page);
  }
  return resizeRect(rect, direction.x === 0 ? "s" : "e", delta, page);
}
