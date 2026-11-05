// Item 2: the browser does the selection and the copy. Transparent text is
// laid over the page the way pdf.js's text layer does it.
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import type { OcrPage } from "../core/ocr";
import { flatten } from "../core/ocr";
import { PageFrame } from "../shared/PageFrame";
import { SearchBands } from "../shared/SearchBands";
import { useEndOfContent } from "./endOfContent";

interface Props {
  page: OcrPage;
  name: string;
  scale: number;
  query: string;
  /** Apply pdf.js's end-of-content trick against jumps in gaps. */
  fix: boolean;
}

const lineText = (l: OcrPage["blocks"][number]["lines"][number]) => l.words.map((w) => w.text).join(" ");

export function NativeView({ page, name, scale, query, fix }: Props) {
  const layer = useRef<HTMLDivElement>(null);
  const flat = useMemo(() => flatten(page), [page]);
  const panel = useRef<HTMLDivElement>(null);
  const [vblock, setVblock] = useState(-1);
  useEndOfContent(layer, fix);
  // The webfont changes the natural widths, so measure again once it is in.
  const [fonts, setFonts] = useState(0);
  useEffect(() => void document.fonts.ready.then(() => setFonts((n) => n + 1)), []);

  // Stretch each line's text to its box: measure the natural size, then scale (pdf.js does the same with canvas).
  useLayoutEffect(() => {
    for (const span of layer.current?.querySelectorAll<HTMLSpanElement>("span.line") ?? []) {
      const vertical = span.dataset.v === "1";
      span.style.transform = "none";
      const r = span.getBoundingClientRect();
      const want = Number(span.dataset.len) * scale;
      const s = want / (vertical ? r.height : r.width);
      span.style.transform = vertical ? `scaleY(${s})` : `scaleX(${s})`;
    }
  }, [page, scale, fonts]);

  // The vertical block shown in the side panel: the one holding the selection's focus.
  useEffect(() => {
    const onChange = () => {
      const n = document.getSelection()?.focusNode;
      const el = n instanceof Element ? n : n?.parentElement;
      const b = el?.closest<HTMLElement>("[data-b]");
      if (b && layer.current?.contains(b) && page.blocks[Number(b.dataset.b)].orientation === "vertical") {
        setVblock(Number(b.dataset.b));
      }
    };
    document.addEventListener("selectionchange", onChange);
    return () => document.removeEventListener("selectionchange", onChange);
  }, [page]);

  // Ctrl+A would select the whole document (toolbar, panel); limit it to the page or the panel.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.key !== "a" || e.target instanceof HTMLInputElement) return;
      const sel = document.getSelection();
      const n = sel?.focusNode;
      const inPanel = n && panel.current?.contains(n);
      const target = inPanel ? panel.current : layer.current;
      if (!sel || !target) return;
      e.preventDefault();
      const r = document.createRange();
      r.selectNodeContents(target);
      sel.removeAllRanges();
      sel.addRange(r);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      <PageFrame page={page} name={name} scale={scale}>
      <div className="native-layer" ref={layer}>
        {page.blocks.map((b, bi) => (
          <div key={bi} data-b={bi} className="block">
            {b.lines.map((l, li) => {
              const v = l.orientation === "vertical";
              const box = l.bbox;
              const thick = (v ? box.x1 - box.x0 : box.y1 - box.y0) * scale;
              return (
                <span key={li}>
                  <span
                    className="line"
                    data-v={v ? "1" : "0"}
                    data-len={v ? box.y1 - box.y0 : box.x1 - box.x0}
                    style={{
                      left: box.x0 * scale,
                      top: box.y0 * scale,
                      fontSize: thick,
                      writingMode: v ? "vertical-rl" : undefined,
                    }}
                  >
                    {lineText(l)}
                  </span>
                  <br />
                </span>
              );
            })}
            {bi + 1 < page.blocks.length && <br />}
          </div>
        ))}
      </div>
      <SearchBands flat={flat} query={query} scale={scale} />
      </PageFrame>
      <VerticalPanel page={page} block={vblock} panelRef={panel} />
    </>
  );
}

function VerticalPanel({ page, block, panelRef }: { page: OcrPage; block: number; panelRef: RefObject<HTMLDivElement | null> }) {
  const b = page.blocks[block];
  return (
    <aside className="vpanel">
      {b ? (
        <div className="vtext" ref={panelRef}>
          {b.lines.map((l, i) => (
            <div key={i}>{lineText(l)}</div>
          ))}
        </div>
      ) : (
        <p className="hint">縦書きの塊を選ぶと、ここに縦書きで出る</p>
      )}
    </aside>
  );
}
