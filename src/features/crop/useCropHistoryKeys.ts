import { useEffect } from "react";
import { useAppDispatch } from "../../state";

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

/** Inputs whose own undo the keys belong to: those that take text. */
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

/**
 * Ctrl+Z undoes and Ctrl+Y or Ctrl+Shift+Z redoes a step of the areas while
 * the crop tab is shown, wherever the focus is (design §10.1), but not in a
 * field that takes text nor while a dialog is open. A drag in progress stops
 * the keys before they get here.
 */
export function useCropHistoryKeys() {
  const dispatch = useAppDispatch();
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const direction = historyKeyOf(event);
      if (
        direction === null ||
        event.defaultPrevented ||
        takesText(event.target) ||
        document.querySelector("[aria-modal='true']") !== null
      ) {
        return;
      }
      event.preventDefault();
      dispatch({ type: direction === "undo" ? "CROP_UNDO" : "CROP_REDO" });
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [dispatch]);
}
