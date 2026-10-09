/** A key combination, with `ctrl` meaning Ctrl (or Command on macOS). */
export interface ShortcutKey {
  /** `KeyboardEvent.key` in lower case. */
  key: string;
  ctrl: boolean;
  shift: boolean;
}

/**
 * The shortcuts of the WebView2 browser that do nothing useful in this app
 * and show browser chrome: find, print and reload. Zoom, copy, paste, select
 * all and undo are not in the list; they work as in any app.
 */
export const BLOCKED_BROWSER_SHORTCUTS: readonly ShortcutKey[] = [
  // Find
  { key: "f", ctrl: true, shift: false },
  { key: "f3", ctrl: false, shift: false },
  { key: "g", ctrl: true, shift: false },
  { key: "g", ctrl: true, shift: true },
  // Print
  { key: "p", ctrl: true, shift: false },
  // Reload
  { key: "r", ctrl: true, shift: false },
  { key: "r", ctrl: true, shift: true },
  { key: "f5", ctrl: false, shift: false },
  { key: "f5", ctrl: true, shift: false },
];

/**
 * Whether `event` is one of [`BLOCKED_BROWSER_SHORTCUTS`]. Alt must not be
 * held: AltGr reports Ctrl and Alt together and types a character on some
 * keyboard layouts.
 */
export function isBlockedBrowserShortcut(event: KeyboardEvent): boolean {
  if (event.altKey) {
    return false;
  }
  const key = event.key.toLowerCase();
  const ctrl = event.ctrlKey || event.metaKey;
  return BLOCKED_BROWSER_SHORTCUTS.some(
    (blocked) =>
      blocked.key === key &&
      blocked.ctrl === ctrl &&
      blocked.shift === event.shiftKey,
  );
}
