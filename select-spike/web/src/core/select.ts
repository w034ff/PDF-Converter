// Selection as a range of glyphs in reading order, built from the glyphs'
// quads alone: no text layout, no font metrics.
import type { Flat, Rect } from "./ocr";
import { quadBox } from "./ocr";

function dist(r: Rect, x: number, y: number): number {
  const dx = Math.max(r.x0 - x, 0, x - r.x1);
  const dy = Math.max(r.y0 - y, 0, y - r.y1);
  return Math.hypot(dx, dy);
}

/**
 * The caret (0..=glyphs.length, before glyph k) nearest to (x, y): the nearest
 * visual line, then the glyph boundary along it. Points in gaps and margins
 * go to the nearest line, so a drag never jumps to the start or end of the page.
 */
export function caretAt(flat: Flat, x: number, y: number): number {
  let best = -1;
  let bestD = Infinity;
  flat.vis.forEach((l, i) => {
    const d = dist(l.bbox, x, y);
    if (d < bestD) {
      bestD = d;
      best = i;
    }
  });
  if (best < 0) return 0;
  const l = flat.vis[best];
  for (let k = l.first; k < l.end; k++) {
    const b = flat.glyphs[k].bbox;
    const centre = l.vertical ? (b.y0 + b.y1) / 2 : (b.x0 + b.x1) / 2;
    if ((l.vertical ? y : x) < centre) return k;
  }
  return l.end;
}

/** One rectangle per visual line covered by glyphs [a, b). */
export function bands(flat: Flat, a: number, b: number): Rect[] {
  const out: Rect[] = [];
  for (const l of flat.vis) {
    const s = Math.max(a, l.first);
    const e = Math.min(b, l.end);
    if (s >= e) continue;
    const f = flat.glyphs[s].quad;
    const t = flat.glyphs[e - 1].quad;
    out.push(quadBox([f[0], t[1], t[2], f[3]]));
  }
  return out;
}

export interface Sel {
  anchor: number;
  focus: number;
}
export const selRange = (s: Sel): [number, number] => [Math.min(s.anchor, s.focus), Math.max(s.anchor, s.focus)];
