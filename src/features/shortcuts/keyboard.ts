/** Which way a key moves through a history of edits, if it does (design §10.1). */
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

/** Inputs whose own keys the keys of a tab must leave alone: those that take text. */
export function takesText(target: EventTarget | null): boolean {
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

/** Whether a dialog is open, which the keys of a tab must not reach past. */
export function isModalOpen(): boolean {
  return document.querySelector("[aria-modal='true']") !== null;
}
