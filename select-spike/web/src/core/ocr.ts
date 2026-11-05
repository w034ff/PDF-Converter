// OcrPage as written by ocr-spike's `page-json` (ocr-spike/src/types.rs), and
// the flat list of glyphs in reading order that the selection works on.

export type Pt = [number, number];
/** Horizontal: TL, TR, BR, BL. Vertical: TR, BR, BL, TL (start of the text first). */
export type Quad = [Pt, Pt, Pt, Pt];
export type Orientation = "horizontal" | "vertical";
export interface Rect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}
export interface OcrChar {
  quad: Quad;
  text: string;
  confidence: number;
}
export interface OcrWord {
  quad: Quad;
  text: string;
  confidence: number;
  chars: OcrChar[];
}
export interface OcrLine {
  quad: Quad;
  bbox: Rect;
  orientation: Orientation;
  text: string;
  confidence: number;
  words: OcrWord[];
}
export interface OcrBlock {
  bbox: Rect;
  orientation: Orientation;
  lines: OcrLine[];
}
export interface OcrPage {
  widthPt: number;
  heightPt: number;
  engine: string;
  blocks: OcrBlock[];
}

export interface Glyph {
  text: string;
  quad: Quad;
  bbox: Rect;
  /** Logical position, for the copied text. */
  block: number;
  line: number;
  word: number;
  /** Visual row (a line on the page, a column in the side panel), for hit tests and bands. */
  vis: number;
}
export interface VisLine {
  first: number;
  /** One past the last glyph. */
  end: number;
  bbox: Rect;
  vertical: boolean;
}
export interface Flat {
  glyphs: Glyph[];
  vis: VisLine[];
}

export function quadBox(q: Quad): Rect {
  const xs = q.map((p) => p[0]);
  const ys = q.map((p) => p[1]);
  return { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
}

export function union(a: Rect, b: Rect): Rect {
  return { x0: Math.min(a.x0, b.x0), y0: Math.min(a.y0, b.y0), x1: Math.max(a.x1, b.x1), y1: Math.max(a.y1, b.y1) };
}

/** Groups glyphs (already in reading order, `vis` set) into visual lines. */
export function withVis(glyphs: Glyph[], vertical: (vis: number) => boolean): Flat {
  const vis: VisLine[] = [];
  glyphs.forEach((g, k) => {
    const last = vis[vis.length - 1];
    if (last && glyphs[k - 1].vis === g.vis) {
      last.end = k + 1;
      last.bbox = union(last.bbox, g.bbox);
    } else {
      vis.push({ first: k, end: k + 1, bbox: g.bbox, vertical: vertical(g.vis) });
    }
  });
  return { glyphs, vis };
}

export function flatten(page: OcrPage): Flat {
  const glyphs: Glyph[] = [];
  const vertical: boolean[] = [];
  let line = 0;
  let word = 0;
  page.blocks.forEach((b, bi) => {
    for (const l of b.lines) {
      for (const w of l.words) {
        for (const c of w.chars) {
          glyphs.push({ text: c.text, quad: c.quad, bbox: quadBox(c.quad), block: bi, line, word, vis: line });
        }
        word++;
      }
      vertical[line] = l.orientation === "vertical";
      line++;
    }
  });
  return withVis(glyphs, (v) => vertical[v]);
}

/** The text of glyphs [a, b): a space between words, a newline between lines, an empty line between blocks. */
export function textOf(flat: Flat, a: number, b: number): string {
  let s = "";
  for (let k = a; k < b; k++) {
    const g = flat.glyphs[k];
    if (k > a) {
      const p = flat.glyphs[k - 1];
      if (p.block !== g.block) s += "\n\n";
      else if (p.line !== g.line) s += "\n";
      else if (p.word !== g.word) s += " ";
    }
    s += g.text;
  }
  return s;
}

/** The expected copy of the whole page. */
export function pageText(page: OcrPage): string {
  return page.blocks.map((b) => b.lines.map((l) => l.words.map((w) => w.text).join(" ")).join("\n")).join("\n\n");
}
