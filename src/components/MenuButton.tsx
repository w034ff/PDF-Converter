import {
  Fragment,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import "./MenuButton.css";

export interface MenuButtonItem {
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

export interface MenuButtonProps {
  /** What the button says. */
  children: ReactNode;
  /** Names the menu for assistive technology. */
  menuLabel: string;
  items: readonly MenuButtonItem[];
  /** A disabled button cannot open the menu, and closes it if it is open. */
  disabled?: boolean;
  className?: string;
  /** Names the button when what it says does not (e.g. a bare number). */
  label?: string;
  /**
   * Which edge of the button the menu lines up with: `end` for a button at
   * the right of its toolbar, `start` for one further left, whose menu would
   * otherwise run off to the left.
   */
  align?: "start" | "end";
}

/** Which item takes the focus when the menu opens. */
type FirstFocus = "first" | "last";

/**
 * A button that opens a menu of actions (the WAI-ARIA menu button pattern,
 * design §10.1): `aria-haspopup` and `aria-expanded` on the button,
 * `role="menu"` with `menuitem`s under it. ↑↓ move between the items that can
 * be chosen (Home and End jump), Enter or Space choose, Esc closes and gives
 * the focus back to the button, and a press outside closes the menu.
 */
export function MenuButton({
  children,
  menuLabel,
  items,
  disabled = false,
  className,
  label,
  align = "end",
}: MenuButtonProps) {
  const [open, setOpen] = useState(false);
  // A menu does not wait to come back when the button is enabled again.
  if (open && disabled) {
    setOpen(false);
  }
  const [firstFocus, setFirstFocus] = useState<FirstFocus>("first");
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const menuId = useId();
  // The checks keep a column of their own, so the labels line up.
  const hasChecks = items.some((item) => item.checked !== undefined);

  function enabledIndexes(): number[] {
    return items.flatMap((item, index) => (item.disabled ? [] : [index]));
  }

  // The focus moves into the menu once it is rendered.
  useEffect(() => {
    if (!open) {
      return;
    }
    const indexes = enabledIndexes();
    const target = firstFocus === "first" ? indexes[0] : indexes.at(-1);
    if (target !== undefined) {
      itemRefs.current[target]?.focus();
    }
    // The items may change while the menu is open; the focus is set once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, firstFocus]);

  useEffect(() => {
    if (!open) {
      return;
    }
    function handlePress(event: MouseEvent) {
      if (
        event.target instanceof Node &&
        rootRef.current?.contains(event.target) !== true
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handlePress);
    return () => document.removeEventListener("mousedown", handlePress);
  }, [open]);

  function openMenu(which: FirstFocus) {
    setFirstFocus(which);
    setOpen(true);
  }

  function closeAndFocusButton() {
    setOpen(false);
    buttonRef.current?.focus();
  }

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

  function handleButtonKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      openMenu("first");
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      openMenu("last");
    }
  }

  function handleMenuKeyDown(event: KeyboardEvent<HTMLDivElement>) {
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
        closeAndFocusButton();
        break;
      case "Tab":
        // The focus leaves the menu with the key; the menu goes with it.
        setOpen(false);
        break;
    }
  }

  return (
    <div ref={rootRef} className="menu-button">
      <button
        ref={buttonRef}
        type="button"
        className={className}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        disabled={disabled}
        onClick={() => (open ? setOpen(false) : openMenu("first"))}
        onKeyDown={handleButtonKeyDown}
      >
        {children}
      </button>
      {open && (
        <div
          id={menuId}
          role="menu"
          aria-label={menuLabel}
          className={`menu-button-menu is-${align}`}
          onKeyDown={handleMenuKeyDown}
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
                  closeAndFocusButton();
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
      )}
    </div>
  );
}
