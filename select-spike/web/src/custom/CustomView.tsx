// Item 4: the selection is built from the glyphs' quads (core/select.ts), the
// same design as the Slint version. The browser's selection is not used.
import { useEffect, useMemo, useRef, useState } from "react";
import type { OcrPage } from "../core/ocr";
import { flatten, textOf } from "../core/ocr";
import { bands, caretAt, selRange, type Sel } from "../core/select";
import { Bands } from "../shared/Bands";
import { PageFrame } from "../shared/PageFrame";
import { SearchBands } from "../shared/SearchBands";
import { useSelection } from "./useSelection";
import { VerticalPanel } from "./VerticalPanel";

interface Props {
  page: OcrPage;
  name: string;
  scale: number;
  query: string;
}

export function CustomView({ page, name, scale, query }: Props) {
  const flat = useMemo(() => flatten(page), [page]);
  const pageRef = useRef<HTMLDivElement>(null);
  const [sel, setSel] = useState<Sel>({ anchor: 0, focus: 0 });
  const [panelSel, setPanelSel] = useState<Sel>({ anchor: 0, focus: 0 });
  const [panelText, setPanelText] = useState<(a: number, b: number) => string>(() => () => "");
  const [panelLen, setPanelLen] = useState(0);
  const active = useRef<"page" | "panel">("page");
  const [vblock, setVblock] = useState(-1);

  const toPt = (e: React.PointerEvent) => {
    const r = pageRef.current!.getBoundingClientRect();
    return caretAt(flat, (e.clientX - r.left) / scale, (e.clientY - r.top) / scale);
  };
  const handlers = useSelection(setSel, toPt, () => (active.current = "page"));

  // The vertical block under the selection's focus goes to the side panel.
  useEffect(() => {
    const g = flat.glyphs[Math.min(sel.focus, flat.glyphs.length - 1)];
    if (g && page.blocks[g.block].orientation === "vertical") setVblock(g.block);
  }, [sel.focus, flat, page]);

  const [a, b] = selRange(sel);
  // Ctrl+A picks the page or the panel, whichever was pressed last; Ctrl+C fills the copy event.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.key !== "a" || e.target instanceof HTMLInputElement) return;
      e.preventDefault();
      if (active.current === "page") setSel({ anchor: 0, focus: flat.glyphs.length });
      else setPanelSel({ anchor: 0, focus: panelLen });
    };
    const copy = (e: ClipboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      const [pa, pb] = selRange(panelSel);
      const text = active.current === "page" ? textOf(flat, a, b) : panelText(pa, pb);
      if (!text) return;
      e.preventDefault();
      e.clipboardData?.setData("text/plain", text);
    };
    document.addEventListener("keydown", key);
    document.addEventListener("copy", copy);
    return () => {
      document.removeEventListener("keydown", key);
      document.removeEventListener("copy", copy);
    };
  }, [flat, a, b, panelSel, panelText, panelLen]);

  return (
    <>
      <PageFrame page={page} name={name} scale={scale} pageRef={pageRef}>
        <SearchBands flat={flat} query={query} scale={scale} />
        <Bands rects={bands(flat, a, b)} scale={scale} className="sel" />
        <div className="custom-layer" {...handlers} />
      </PageFrame>
      <VerticalPanel
        page={page}
        flat={flat}
        block={vblock}
        sel={panelSel}
        setSel={setPanelSel}
        onActive={() => (active.current = "panel")}
        onModel={(text, len) => {
          setPanelText(() => text);
          setPanelLen(len);
        }}
      />
    </>
  );
}
