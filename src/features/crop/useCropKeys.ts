import { useEffect } from "react";
import { useAppDispatch, type CropTool } from "../../state";

/** Which way a key moves through the history of the areas, if it does (design §10.1). */
export function historyKeyOf(
  event: Pick<
    KeyboardEvent,
    "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey"
  >,
): "undo" | "redo" | null {
  if (!event.ctrlKey || event.altKey || event.metaKey) {
    return null;
  }
  const key = event.key.toLowerCase();
  if (key === "z") {
    return event.shiftKey ? "redo" : "undo";
  }
  return key === "y" && !event.shiftKey ? "redo" : null;
}

/** The tool a plain key picks: V selects and R draws (design §10.1). */
function toolKeyOf(event: KeyboardEvent): CropTool | null {
  if (event.ctrlKey || event.altKey || event.metaKey || event.shiftKey) {
    return null;
  }
  const key = event.key.toLowerCase();
  return key === "v" ? "select" : key === "r" ? "draw" : null;
}

/** Inputs whose own keys these would take: those that take text. */
function takesText(target: EventTarget | null): boolean {
  if (target instanceof HTMLTextAreaElement) {
    return true;
  }
  if (target instanceof HTMLInputElement) {
    return !["checkbox", "radio", "button", "submit", "reset"].includes(
      target.type,
    );
  }
  return target instanceof HTMLElement && target.isContentEditable;
}

export interface CropKeysOptions {
  /** The tool in use, which Esc takes back to "select". */
  tool: CropTool;
  /** Whether "描く" can be picked; it cannot while a conversion runs. */
  canDraw: boolean;
  /** Opens the list of the keys (`?`). */
  onHelp: () => void;
}

/**
 * The keys of the crop tab that work wherever the focus is while a PDF is
 * open (design §10.1): Ctrl+Z undoes and Ctrl+Y or Ctrl+Shift+Z redoes, V and
 * R pick the tool, Esc leaves "描く", and `?` lists the keys. None of them
 * work in a field that takes text nor while a dialog is open, and a drag in
 * progress stops them before they get here.
 */
export function useCropKeys({ tool, canDraw, onHelp }: CropKeysOptions) {
  const dispatch = useAppDispatch();
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (
        event.defaultPrevented ||
        takesText(event.target) ||
        document.querySelector("[aria-modal='true']") !== null
      ) {
        return;
      }
      const direction = historyKeyOf(event);
      if (direction !== null) {
        event.preventDefault();
        dispatch({ type: direction === "undo" ? "CROP_UNDO" : "CROP_REDO" });
        return;
      }
      const picked = toolKeyOf(event);
      if (picked !== null) {
        if (picked === "select" || canDraw) {
          event.preventDefault();
          dispatch({ type: "CROP_TOOL_SET", tool: picked });
        }
        return;
      }
      if (event.key === "Escape" && tool === "draw") {
        event.preventDefault();
        dispatch({ type: "CROP_TOOL_SET", tool: "select" });
        return;
      }
      if (event.key === "?" && !event.ctrlKey && !event.altKey) {
        event.preventDefault();
        onHelp();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [dispatch, tool, canDraw, onHelp]);
}
