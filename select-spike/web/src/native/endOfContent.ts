// pdf.js's fix for selections that jump when the pointer is over a gap
// between absolutely placed spans (text_layer_builder.js,
// #enableGlobalSelectionListener): while selecting, a full-size empty div sits
// behind the spans, and on every selection change it is moved in the DOM to
// just after the selection's moving end. A pointer over a gap then hits that
// div, whose DOM position is where the selection already is.
import { useEffect, type RefObject } from "react";

export function useEndOfContent(layer: RefObject<HTMLDivElement | null>, enabled: boolean) {
  useEffect(() => {
    const root = layer.current;
    if (!enabled || !root) return;
    const end = document.createElement("div");
    end.className = "endOfContent";
    root.append(end);
    let prev: Range | null = null;
    const down = () => root.classList.add("selecting");
    const up = () => {
      root.classList.remove("selecting");
      root.append(end);
    };
    const change = () => {
      const sel = document.getSelection();
      if (!sel || sel.rangeCount === 0) return;
      const range = sel.getRangeAt(0);
      if (!root.contains(range.commonAncestorContainer)) return;
      const modifyStart =
        prev !== null &&
        (range.compareBoundaryPoints(Range.END_TO_END, prev) === 0 ||
          range.compareBoundaryPoints(Range.START_TO_END, prev) === 0);
      let anchor: Node | null = modifyStart ? range.startContainer : range.endContainer;
      if (anchor.nodeType === Node.TEXT_NODE) anchor = anchor.parentNode;
      const parent = anchor?.parentElement;
      if (anchor && parent && anchor !== end && root.contains(parent)) {
        parent.insertBefore(end, modifyStart ? anchor : anchor.nextSibling);
      }
      prev = range.cloneRange();
    };
    root.addEventListener("pointerdown", down);
    document.addEventListener("pointerup", up);
    document.addEventListener("selectionchange", change);
    return () => {
      root.removeEventListener("pointerdown", down);
      document.removeEventListener("pointerup", up);
      document.removeEventListener("selectionchange", change);
      end.remove();
    };
  }, [layer, enabled]);
}
