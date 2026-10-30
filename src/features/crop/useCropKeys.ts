import { useEffect } from "react";
import { useAppDispatch, type CropTool } from "../../state";
import { historyKeyOf, isModalOpen, takesText } from "../shortcuts/keyboard";

function hasModifier(event: KeyboardEvent): boolean {
  return event.ctrlKey || event.altKey || event.metaKey || event.shiftKey;
}

/** Whether the key is R alone, which starts drawing an area (design §10.1). */
function isDrawKey(event: KeyboardEvent): boolean {
  return !hasModifier(event) && event.key.toLowerCase() === "r";
}

export interface CropKeysOptions {
  /** The tool in use: Esc stops drawing. */
  tool: CropTool;
  /** Whether R can start drawing; it cannot while a conversion runs. */
  canDraw: boolean;
  /** Opens the list of the keys (`?`). */
  onHelp: () => void;
  /**
   * Draws an area over the whole page shown (Enter while drawing): the way
   * to make an area of one's own without a pointer, to move and resize with
   * the arrows after.
   */
  onDrawWholePage: () => void;
}

/**
 * The keys of the crop tab that work wherever the focus is while a PDF is
 * open (design §10.1): Ctrl+Z undoes and Ctrl+Y or Ctrl+Shift+Z redoes, R
 * starts drawing an area, Enter then draws one over the whole page, Esc
 * stops drawing, and `?` lists the keys. None of them
 * work in a field that takes text nor while a dialog is open, and a drag in
 * progress stops them before they get here.
 */
export function useCropKeys({
  tool,
  canDraw,
  onHelp,
  onDrawWholePage,
}: CropKeysOptions) {
  const dispatch = useAppDispatch();
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.defaultPrevented || takesText(event.target) || isModalOpen()) {
        return;
      }
      const direction = historyKeyOf(event);
      if (direction !== null) {
        event.preventDefault();
        dispatch({ type: direction === "undo" ? "CROP_UNDO" : "CROP_REDO" });
        return;
      }
      if (isDrawKey(event)) {
        if (canDraw) {
          event.preventDefault();
          dispatch({ type: "CROP_TOOL_SET", tool: "draw" });
        }
        return;
      }
      if (event.key === "Escape" && tool === "draw") {
        event.preventDefault();
        dispatch({ type: "CROP_TOOL_SET", tool: "select" });
        return;
      }
      // Before the control with the focus takes it: the menu's button, which
      // has the focus after "範囲を描く", would open the menu again.
      if (event.key === "Enter" && tool === "draw" && !hasModifier(event)) {
        event.preventDefault();
        onDrawWholePage();
        return;
      }
      if (event.key === "?" && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        onHelp();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [dispatch, tool, canDraw, onHelp, onDrawWholePage]);
}
