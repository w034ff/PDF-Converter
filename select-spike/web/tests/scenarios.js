// Builds the operation script for one page, run inside the page (needs window.__spike).
// Every implementation (HTML x2, Slint) replays the same JSON. Points are page pt.
export function buildScenarios() {
  const S = window.__spike;
  const { flat, page } = S;
  const G = flat.glyphs;
  const vert = (k) => flat.vis[G[k].vis].vertical;
  // A point at fraction f along glyph k, at the middle across it.
  const at = (k, f) => {
    const b = G[k].bbox;
    return vert(k) ? [(b.x0 + b.x1) / 2, b.y0 + (b.y1 - b.y0) * f] : [b.x0 + (b.x1 - b.x0) * f, (b.y0 + b.y1) / 2];
  };
  const blockFirst = (bi) => G.findIndex((g) => g.block === bi);
  const blockEnd = (bi) => { let e = -1; G.forEach((g, k) => { if (g.block === bi) e = k + 1; }); return e; };
  const nb = page.blocks.length;
  // The block with the most lines.
  let big = 0;
  page.blocks.forEach((b, i) => { if (b.lines.length > page.blocks[big].lines.length) big = i; });
  const visOfBlock = flat.vis.filter((l) => G[l.first].block === big);
  const midLine = visOfBlock[Math.floor(visOfBlock.length / 2)];
  const line = (pts) => { const out = []; for (let i = 0; i < pts.length - 1; i++) for (let t = 0; t < 8; t++) { const f = t / 8; out.push([pts[i][0] + (pts[i + 1][0] - pts[i][0]) * f, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * f]); } out.push(pts[pts.length - 1]); return out; };
  const caretText = (a, p) => { const c = S.caretAt(p[0], p[1]); return S.textOf(Math.min(a, c), Math.max(a, c)); };
  const sc = [];
  sc.push({ name: "all", steps: [{ op: "click", pt: at(0, 0.5) }, { op: "key", key: "ctrl+a" }, { op: "key", key: "ctrl+c" }], copy: S.pageText });

  const s = blockFirst(nb > 2 ? 1 : 0) + 1;
  const tb = nb > 2 ? nb - 2 : nb - 1;
  const t = Math.floor((blockFirst(tb) + blockEnd(tb)) / 2);
  sc.push({ name: "cross-blocks", steps: [{ op: "drag", pts: line([at(s, 0.2), at(t, 0.8)]) }, { op: "key", key: "ctrl+c" }], copy: S.textOf(s, t + 1) });

  const s2 = blockFirst(big);
  const mb = midLine.bbox;
  const gapPt = midLine.vertical ? [(mb.x0 + mb.x1) / 2, mb.y1 + 6] : [mb.x1 + 6, (mb.y0 + mb.y1) / 2];
  sc.push({ name: "end-in-line-gap", steps: [{ op: "drag", pts: line([at(s2, 0.2), gapPt]) }, { op: "key", key: "ctrl+c" }], copy: caretText(s2, gapPt) });

  // Between two blocks: the middle of the gap between consecutive blocks' boxes.
  const bb = page.blocks.map((b) => b.bbox);
  const k = Math.max(1, Math.floor(nb / 2));
  const a = bb[k - 1], b = bb[k];
  const between = [(Math.max(a.x0, b.x0) + Math.min(a.x1, b.x1)) / 2, (a.y1 + b.y0) / 2];
  const betweenPt = a.y1 < b.y0 ? between : [(a.x0 + b.x1) / 2, (Math.max(a.y0, b.y0) + Math.min(a.y1, b.y1)) / 2];
  const s3 = blockFirst(k - 1);
  sc.push({ name: "end-between-blocks", steps: [{ op: "drag", pts: line([at(s3, 0.2), betweenPt]) }, { op: "key", key: "ctrl+c" }], copy: caretText(s3, betweenPt) });

  // Through gaps: leave the text, wander in the margin, come back to a glyph.
  const s4 = blockFirst(0);
  const t4 = blockEnd(big) - 2;
  const margin = [6, (mb.y0 + mb.y1) / 2];
  sc.push({ name: "through-margin", steps: [{ op: "drag", pts: line([at(s4, 0.2), betweenPt, margin, at(t4, 0.8)]) }, { op: "key", key: "ctrl+c" }], copy: S.textOf(s4, t4 + 1) });
  sc.push({ name: "end-in-margin", steps: [{ op: "drag", pts: line([at(s4, 0.2), margin]) }, { op: "key", key: "ctrl+c" }], copy: caretText(s4, margin) });

  const s5 = blockFirst(big) + 2;
  const t5 = blockEnd(big) - 3;
  sc.push({ name: "shift-click", steps: [{ op: "click", pt: at(s5, 0.2) }, { op: "click", pt: at(t5, 0.8), shift: true }, { op: "key", key: "ctrl+c" }], copy: S.textOf(s5, t5 + 1) });

  // A search hit across a line break: the last 2 glyphs of a line and the first 2 of the next.
  const ln = visOfBlock[Math.max(0, Math.floor(visOfBlock.length / 2) - 1)];
  const q = G.slice(ln.end - 2, ln.end + 2).map((g) => g.text).join("");
  const hits = S.search(q);
  sc.push({ name: "search-across-lines", steps: [{ op: "search", q }], query: q, hits, rects: hits.flatMap(([x, y]) => S.bands(x, y)) });

  const vb = page.blocks.findIndex((b) => b.orientation === "vertical" && b.lines.length > 1);
  if (vb >= 0) {
    const vl = page.blocks[vb].lines;
    const lt = (l) => l.words.map((w) => w.text).join(" ");
    const pick = [{ op: "click", pt: at(blockFirst(vb) + 1, 0.5) }];
    sc.push({ name: "panel-all", vblock: vb, steps: [...pick, { op: "panel-click", line: 0, char: 1 }, { op: "key", key: "ctrl+a" }, { op: "key", key: "ctrl+c" }], copy: vl.map(lt).join("\n") });
    const n1 = [...lt(vl[1])].length;
    sc.push({ name: "panel-drag", vblock: vb, steps: [...pick, { op: "panel-drag", from: [0, 2], to: [1, n1 - 1] }, { op: "key", key: "ctrl+c" }], copy: [...lt(vl[0])].slice(2).join("") + "\n" + lt(vl[1]) });
  }
  return sc;
}
