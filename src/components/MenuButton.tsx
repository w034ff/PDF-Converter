import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { MenuList, type MenuFirstFocus, type MenuItem } from "./MenuList";
import "./MenuButton.css";

export type MenuButtonItem = MenuItem;

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

/**
 * A button that opens a menu of actions (the WAI-ARIA menu button pattern,
 * design §10.1): `aria-haspopup` and `aria-expanded` on the button, and the
 * menu (`MenuList`) under it. ↓ on the button opens it at the first item and
 * ↑ at the last, Esc or a chosen item closes it and gives the focus back to
 * the button, and a press outside closes it.
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
  const [firstFocus, setFirstFocus] = useState<MenuFirstFocus>("first");
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const menuId = useId();

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

  function openMenu(which: MenuFirstFocus) {
    setFirstFocus(which);
    setOpen(true);
  }

  function closeAndFocusButton() {
    setOpen(false);
    buttonRef.current?.focus();
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
        <MenuList
          id={menuId}
          label={menuLabel}
          items={items}
          className={`menu-button-menu is-${align}`}
          firstFocus={firstFocus}
          onDismiss={closeAndFocusButton}
          onLeave={() => setOpen(false)}
        />
      )}
    </div>
  );
}
