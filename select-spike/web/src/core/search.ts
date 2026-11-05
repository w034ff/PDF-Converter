import type { Flat } from "./ocr";

const norm = (s: string) => s.normalize("NFKC").toLowerCase();

/**
 * Finds `query` in the page's glyphs, ignoring spaces, width (NFKC) and case.
 * Lines inside a block are joined without a separator, so a match may run from
 * the end of one line into the next. Blocks are never joined. Returns glyph ranges [a, b).
 */
export function search(flat: Flat, query: string): Array<[number, number]> {
  const q = norm(query).replace(/\s+/g, "");
  if (!q) return [];
  let hay = "";
  const at: number[] = [];
  flat.glyphs.forEach((g, k) => {
    if (k > 0 && flat.glyphs[k - 1].block !== g.block) {
      hay += "\u0000";
      at.push(-1);
    }
    for (const c of norm(g.text).replace(/\s+/g, "")) {
      hay += c;
      at.push(k);
    }
  });
  const out: Array<[number, number]> = [];
  for (let i = hay.indexOf(q); i >= 0; i = hay.indexOf(q, i + q.length)) {
    out.push([at[i], at[i + q.length - 1] + 1]);
  }
  return out;
}
