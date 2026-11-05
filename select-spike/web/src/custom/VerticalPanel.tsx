// The side panel: the browser lays the block out with writing-mode, each glyph
// in its own span; the spans' boxes are measured and fed to the same
// glyph-range selection as the page.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Flat, Glyph, OcrPage, Quad } from "../core/ocr";
import { textOf, withVis } from "../core/ocr";
import { bands, caretAt, selRange, type Sel } from "../core/select";
import { Bands } from "../shared/Bands";
import { useSelection } from "./useSelection";

interface Props {
  page: OcrPage;
  flat: Flat;
  block: number;
  sel: Sel;
  setSel: (f: (s: Sel) => Sel) => void;
  onActive: () => void;
  onModel: (text: (a: number, b: number) => string, len: number) => void;
}

export function VerticalPanel({ page, flat, block, sel, setSel, onActive, onModel }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const [panel, setPanel] = useState<Flat>({ glyphs: [], vis: [] });
  // Measure again once the webfont is in (it moves every glyph).
  const [fonts, setFonts] = useState(0);
  useEffect(() => void document.fonts.ready.then(() => setFonts((n) => n + 1)), []);
  const src = flat.glyphs.filter((g) => g.block === block);
  const lines: Glyph[][] = [];
  src.forEach((g, k) => (k > 0 && src[k - 1].line === g.line ? lines[lines.length - 1].push(g) : lines.push([g])));

  useLayoutEffect(() => {
    const root = wrap.current;
    if (!root) return;
    const o = root.getBoundingClientRect();
    let vis = -1;
    let lastX = NaN;
    const glyphs: Glyph[] = [...root.querySelectorAll<HTMLSpanElement>("span[data-pc]")].map((sp, k) => {
      const r = sp.getBoundingClientRect();
      const [x0, y0, x1, y1] = [r.left - o.left, r.top - o.top, r.right - o.left, r.bottom - o.top];
      if (Math.abs(x0 - lastX) > 1) vis++;
      lastX = x0;
      const quad: Quad = [[x1, y0], [x1, y1], [x0, y1], [x0, y0]];
      return { ...src[k], block: 0, quad, bbox: { x0, y0, x1, y1 }, vis };
    });
    const f = withVis(glyphs, () => true);
    setPanel(f);
    setSel(() => ({ anchor: 0, focus: 0 }));
    onModel((a, b) => textOf(f, a, b), f.glyphs.length);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [block, page, fonts]);

  const handlers = useSelection(
    setSel,
    (e) => {
      const o = wrap.current!.getBoundingClientRect();
      return caretAt(panel, e.clientX - o.left, e.clientY - o.top);
    },
    onActive,
  );
  const [a, b] = selRange(sel);
  if (block < 0) {
    return (
      <aside className="vpanel">
        <p className="hint">縦書きの塊を選ぶと、ここに縦書きで出る</p>
      </aside>
    );
  }
  return (
    <aside className="vpanel">
      <div className="vwrap" ref={wrap} {...handlers}>
        <div className="vtext">
          {lines.map((l, i) => (
            <div key={i}>
              {l.map((g, k) => (
                <span key={k}>
                  {k > 0 && l[k - 1].word !== g.word && " "}
                  <span data-pl={i} data-pc={k}>
                    {g.text}
                  </span>
                </span>
              ))}
            </div>
          ))}
        </div>
        <Bands rects={bands(panel, a, b)} scale={1} className="sel" />
      </div>
    </aside>
  );
}
