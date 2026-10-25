import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { MenuList, type MenuItem } from "./MenuList";
import "./MenuButton.css";

/** How far the menu keeps from the window's edges, in CSS px. */
const CONTEXT_MENU_MARGIN_PX = 4;

export interface ContextMenuProps {
  /** Where it opens: the pointer, or a corner of the focused element. */
  x: number;
  y: number;
  /** Names the menu for assistive technology. */
  label: string;
  items: readonly MenuItem[];
  /**
   * The menu closes. `returnFocus` is true after Esc or a chosen item (before
   * its `onSelect`), when the focus should go back to what opened it.
   */
  onClose: (returnFocus: boolean) => void;
}

/**
 * A menu opened by a right click, Shift+F10 or the menu key (design §10.1):
 * the same items and keys as a menu button's (`MenuList`), at the pointer
 * and moved back inside the window if it would run over an edge. A press
 * outside it or a scroll closes it, since it stands for the place that was
 * pressed.
 */
export function ContextMenu({ x, y, label, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState({ left: x, top: y });
  // The listeners below are added once; they call the latest `onClose`.
  const closeRef = useRef(onClose);
  useEffect(() => {
    closeRef.current = onClose;
  });

  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (menu === null) {
      return;
    }
    const { width, height } = menu.getBoundingClientRect();
    setPlace({
      left: Math.max(
        CONTEXT_MENU_MARGIN_PX,
        Math.min(x, window.innerWidth - width - CONTEXT_MENU_MARGIN_PX),
      ),
      top: Math.max(
        CONTEXT_MENU_MARGIN_PX,
        Math.min(y, window.innerHeight - height - CONTEXT_MENU_MARGIN_PX),
      ),
    });
  }, [x, y]);

  useEffect(() => {
    function handlePress(event: MouseEvent) {
      if (
        event.target instanceof Node &&
        menuRef.current?.contains(event.target) !== true
      ) {
        closeRef.current(false);
      }
    }
    function handleScroll(event: Event) {
      if (
        event.target instanceof Node &&
        menuRef.current?.contains(event.target) !== true
      ) {
        closeRef.current(false);
      }
    }
    // Another window took the focus, as with a native menu.
    function handleBlur() {
      closeRef.current(false);
    }
    document.addEventListener("mousedown", handlePress);
    document.addEventListener("scroll", handleScroll, true);
    window.addEventListener("blur", handleBlur);
    return () => {
      document.removeEventListener("mousedown", handlePress);
      document.removeEventListener("scroll", handleScroll, true);
      window.removeEventListener("blur", handleBlur);
    };
  }, []);

  // Keys in the menu are its own: they go no further, to the components it
  // was opened from or to the page's keys on the document.
  return createPortal(
    <div
      className="context-menu-root"
      onKeyDown={(event) => event.stopPropagation()}
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <MenuList
        menuRef={menuRef}
        label={label}
        items={items}
        className="menu-button-menu context-menu"
        style={{ left: place.left, top: place.top }}
        onDismiss={() => onClose(true)}
        onLeave={() => onClose(false)}
      />
    </div>,
    document.body,
  );
}
