import {
  Fragment,
  useEffect,
  useRef,
  type CSSProperties,
  type KeyboardEvent,
  type RefObject,
} from "react";
import "./MenuButton.css";

export interface MenuItem {
  /** Names the item for `key`; unique within the menu. */
  id: string;
  label: string;
  /** An item that cannot be chosen now is shown but skipped by the arrow keys. */
  disabled?: boolean;
  /**
   * Set on the items of a menu that picks one of several states: the item is
   * a `menuitemradio` and shows a check while it is the state in use.
   */
  checked?: boolean;
  /** Draws a rule above the item, to set it apart from the ones before. */
  separatorBefore?: boolean;
  onSelect: () => void;
}

/** Which item takes the focus when the menu opens. */
export type MenuFirstFocus = "first" | "last";

export interface MenuListProps {
  id?: string;
  /** Names the menu for assistive technology. */
  label: string;
  items: readonly MenuItem[];
  className: string;
  style?: CSSProperties;
  menuRef?: RefObject<HTMLDivElement | null>;
  firstFocus?: MenuFirstFocus;
  /**
   * Esc, or an item chosen (called before the item's `onSelect`): the menu
   * closes and the focus goes back to what opened it.
   */
  onDismiss: () => void;
  /** Tab took the focus out of the menu, which closes with it. */
  onLeave: () => void;
}

/**
 * The open part of a menu (WAI-ARIA, design §10.1): `role="menu"` with
 * `menuitem`s, the first (or last) one that can be chosen focused when it
 * opens. ↑↓ move between the items that can be chosen (Home and End jump),
 * Enter or Space choose, Esc dismisses it, and Tab leaves it. Closing on a
 * press outside is left to the owner, which knows what counts as inside.
 */
export function MenuList({
  id,
  label,
  items,
  className,
  style,
  menuRef,
  firstFocus = "first",
  onDismiss,
  onLeave,
}: MenuListProps) {
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  // The checks keep a column of their own, so the labels line up.
  const hasChecks = items.some((item) => item.checked !== undefined);

  function enabledIndexes(): number[] {
    return items.flatMap((item, index) => (item.disabled ? [] : [index]));
  }

  const fallbackRef = useRef<HTMLDivElement | null>(null);
  const ownRef = menuRef ?? fallbackRef;

  // The focus moves into the menu once it is rendered: onto the menu itself
  // when no item can be chosen, so that Esc and Tab still reach it.
  useEffect(() => {
    const indexes = enabledIndexes();
    const target = firstFocus === "first" ? indexes[0] : indexes.at(-1);
    if (target !== undefined) {
      itemRefs.current[target]?.focus();
    } else {
      ownRef.current?.focus();
    }
    // The items may change while the menu is open; the focus is set once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function moveFocus(from: number, step: 1 | -1) {
    const indexes = enabledIndexes();
    if (indexes.length === 0) {
      return;
    }
    const position = indexes.indexOf(from);
    const next =
      position === -1
        ? indexes[0]
        : indexes[(position + step + indexes.length) % indexes.length];
    itemRefs.current[next]?.focus();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const focused = itemRefs.current.findIndex(
      (element) => element === document.activeElement,
    );
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveFocus(focused, 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveFocus(focused, -1);
        break;
      case "Home":
        event.preventDefault();
        itemRefs.current[enabledIndexes()[0]]?.focus();
        break;
      case "End":
        event.preventDefault();
        itemRefs.current[enabledIndexes().at(-1) ?? -1]?.focus();
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        onDismiss();
        break;
      case "Tab":
        onLeave();
        break;
    }
  }

  return (
    <div
      ref={ownRef}
      id={id}
      role="menu"
      tabIndex={-1}
      aria-label={label}
      className={className}
      style={style}
      onKeyDown={handleKeyDown}
    >
      {items.map((item, index) => (
        <Fragment key={item.id}>
          {item.separatorBefore === true && (
            <div role="separator" className="menu-button-separator" />
          )}
          <button
            ref={(element) => {
              itemRefs.current[index] = element;
            }}
            type="button"
            role={item.checked === undefined ? "menuitem" : "menuitemradio"}
            aria-checked={item.checked}
            tabIndex={-1}
            className="menu-button-item"
            disabled={item.disabled}
            onClick={() => {
              onDismiss();
              item.onSelect();
            }}
          >
            {hasChecks && (
              <span className="menu-button-check" aria-hidden="true">
                {item.checked === true ? "✓" : ""}
              </span>
            )}
            {item.label}
          </button>
        </Fragment>
      ))}
    </div>
  );
}
