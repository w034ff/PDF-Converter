import type { PointerEvent as RPointerEvent } from "react";
import type { Sel } from "../core/select";

/** Press sets the anchor (Shift+press keeps it), dragging moves the focus. */
export function useSelection(setSel: (f: (s: Sel) => Sel) => void, caret: (e: RPointerEvent) => number, onPress: () => void) {
  return {
    onPointerDown: (e: RPointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.currentTarget.setPointerCapture(e.pointerId);
      onPress();
      const c = caret(e);
      setSel((s) => (e.shiftKey ? { anchor: s.anchor, focus: c } : { anchor: c, focus: c }));
    },
    onPointerMove: (e: RPointerEvent<HTMLElement>) => {
      if (!(e.buttons & 1)) return;
      const c = caret(e);
      setSel((s) => (s.focus === c ? s : { anchor: s.anchor, focus: c }));
    },
  };
}
