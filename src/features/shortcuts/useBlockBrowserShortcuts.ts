import { useEffect } from "react";
import { isBlockedBrowserShortcut } from "./blockedShortcuts";

/**
 * Stops the browser's own shortcuts for find, print, reload, downloads and
 * caret browsing from reaching
 * the WebView (design §10.1). Listens on `window`, so a key pressed while an
 * input has focus is stopped too.
 */
export function useBlockBrowserShortcuts(): void {
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (isBlockedBrowserShortcut(event)) {
        event.preventDefault();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);
}
