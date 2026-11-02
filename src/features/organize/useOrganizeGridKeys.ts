import {
  useEffect,
  useRef,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import {
  keyOfPage,
  useAppDispatch,
  type OrganizePage,
  type PageKey,
} from "../../state";
import { isModalOpen, takesText } from "../shortcuts/keyboard";
import { focusIndexAfter, isFocusKey, keyInsertIndex } from "./organizeGrid";
import { FOCUS_LANDING, type RevealTarget } from "./useGridReveal";

export interface OrganizeGridKeysOptions {
  pages: readonly OrganizePage[];
  selected: ReadonlySet<PageKey>;
  /** The page the keys work on when nothing is selected: the one Tab enters. */
  tabStopKey: PageKey | null;
  indexByKey: ReadonlyMap<PageKey, number>;
  columns: number;
  locked: boolean;
  isSplit: boolean;
  reveal: (target: RevealTarget) => void;
}

/** The keys a page operation by key or button works on: the selection, else the focused page. */
function operatedKeys(
  selected: ReadonlySet<PageKey>,
  focused: PageKey | null,
): PageKey[] {
  if (selected.size > 0) {
    return [...selected];
  }
  return focused === null ? [] : [focused];
}

/** `Ctrl+A`, which selects every page wherever the focus is. */
function isSelectAll(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "metaKey">,
): boolean {
  return (
    event.ctrlKey &&
    !event.altKey &&
    !event.metaKey &&
    event.key.toLowerCase() === "a"
  );
}

/** The way `R` or `L` turns the pages, if the key is one of them. */
function rotationOf(key: string): "left" | "right" | null {
  const lower = key.toLowerCase();
  return lower === "r" ? "right" : lower === "l" ? "left" : null;
}

/**
 * The keys of the grid (organize §4, design §10.1): the handler for the key
 * presses in the grid, and the listener on the window for the ones that work
 * on the selection with the focus outside it.
 */
export function useOrganizeGridKeys({
  pages,
  selected,
  tabStopKey,
  indexByKey,
  columns,
  locked,
  isSplit,
  reveal,
}: OrganizeGridKeysOptions): {
  handleKeyDown: (event: ReactKeyboardEvent<HTMLElement>) => void;
} {
  const dispatch = useAppDispatch();

  function rotate(keys: PageKey[], direction: "left" | "right") {
    dispatch({ type: "ORGANIZE_PAGES_ROTATED", keys, direction });
  }

  function remove(keys: PageKey[]) {
    // The cell that has the focus is the one going, and the focus must not be lost with it.
    reveal(FOCUS_LANDING);
    dispatch({ type: "ORGANIZE_PAGES_DELETED", keys });
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLElement>) {
    const current = tabStopKey;
    if (current === null) {
      return;
    }
    const { key } = event;
    if (isFocusKey(key) && !event.altKey && !event.ctrlKey && !event.metaKey) {
      const from = indexByKey.get(current) ?? 0;
      const to = focusIndexAfter(key, from, columns, pages.length);
      event.preventDefault();
      const target = keyOfPage(pages[to]);
      reveal(target);
      dispatch({
        type: "ORGANIZE_FOCUSED",
        key: target,
        extend: event.shiftKey,
      });
      return;
    }
    if (event.altKey && !event.ctrlKey && !event.metaKey) {
      const direction =
        key === "ArrowLeft" || key === "ArrowUp"
          ? "before"
          : key === "ArrowRight" || key === "ArrowDown"
            ? "after"
            : null;
      if (direction !== null) {
        event.preventDefault();
        if (locked) {
          return;
        }
        const keys = operatedKeys(selected, current);
        const vertical = key === "ArrowUp" || key === "ArrowDown";
        const insertIndex = keyInsertIndex(
          pages,
          new Set(keys),
          direction,
          vertical ? columns : 1,
        );
        reveal(current);
        dispatch({ type: "ORGANIZE_PAGES_MOVED", keys, insertIndex });
      }
      return;
    }
    if (isSelectAll(event)) {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_SELECTED_ALL" });
      return;
    }
    if (event.ctrlKey || event.altKey || event.metaKey) {
      return;
    }
    const rotation = rotationOf(key);
    if (key === " ") {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_SELECTION_TOGGLED", key: current });
    } else if (key === "Escape") {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_SELECTION_CLEARED" });
    } else if (!locked && isSplit && (key === "b" || key === "B")) {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_BREAK_TOGGLED", key: current });
    } else if (!locked && rotation !== null) {
      event.preventDefault();
      rotate(operatedKeys(selected, current), rotation);
    } else if (!locked && (key === "Delete" || key === "Backspace")) {
      event.preventDefault();
      remove(operatedKeys(selected, current));
    }
  }

  /**
   * The keys that work on the selection also work with the focus on the band,
   * where the mouse leaves it, or nowhere, where deleting leaves it (design
   * §10.1). `Ctrl+A` works anywhere text is not typed: it would select the
   * words of the screen otherwise. Keys in the grid have been dealt with, and
   * the other controls keep their own.
   */
  function handleKeyOutsideGrid(event: KeyboardEvent) {
    if (event.defaultPrevented || takesText(event.target) || isModalOpen()) {
      return;
    }
    if (isSelectAll(event)) {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_SELECTED_ALL" });
      return;
    }
    const { target } = event;
    const onBand =
      target === document.body ||
      (target instanceof Element &&
        target.closest(".organize-band") !== null &&
        target.closest(".organize-band-actions") === null);
    if (
      !onBand ||
      selected.size === 0 ||
      event.ctrlKey ||
      event.altKey ||
      event.metaKey
    ) {
      return;
    }
    const keys = [...selected];
    const key = event.key.toLowerCase();
    const rotation = rotationOf(key);
    if (key === "escape") {
      event.preventDefault();
      // The band's buttons are about to go out of use, and the focus with them.
      if (tabStopKey !== null) {
        reveal(tabStopKey);
      }
      dispatch({ type: "ORGANIZE_SELECTION_CLEARED" });
    } else if (!locked && rotation !== null) {
      event.preventDefault();
      rotate(keys, rotation);
    } else if (!locked && (key === "delete" || key === "backspace")) {
      event.preventDefault();
      remove(keys);
    }
  }
  const keyOutsideGrid = useRef(handleKeyOutsideGrid);
  useEffect(() => {
    keyOutsideGrid.current = handleKeyOutsideGrid;
  });
  useEffect(() => {
    function handle(event: KeyboardEvent) {
      keyOutsideGrid.current(event);
    }
    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, []);

  return { handleKeyDown };
}
