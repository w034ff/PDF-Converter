import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MenuButton, type MenuButtonItem } from "./MenuButton";

afterEach(cleanup);

function items(overrides: Partial<Record<string, boolean>> = {}) {
  const onSelect = {
    a: vi.fn(),
    b: vi.fn(),
    c: vi.fn(),
  };
  const list: MenuButtonItem[] = [
    { id: "a", label: "First", disabled: overrides.a, onSelect: onSelect.a },
    { id: "b", label: "Second", disabled: overrides.b, onSelect: onSelect.b },
    { id: "c", label: "Third", disabled: overrides.c, onSelect: onSelect.c },
  ];
  return { list, onSelect };
}

function renderMenu(
  list: MenuButtonItem[],
  disabled = false,
): HTMLButtonElement {
  render(
    <div>
      <button type="button">outside</button>
      <MenuButton menuLabel="Actions" items={list} disabled={disabled}>
        Open menu
      </MenuButton>
    </div>,
  );
  return screen.getByRole("button", { name: "Open menu" });
}

function focusedName(): string | null {
  return document.activeElement?.textContent ?? null;
}

describe("MenuButton", () => {
  it("is a menu button that starts closed", () => {
    const { list } = items();
    const button = renderMenu(list);
    expect(button).toHaveAttribute("aria-haspopup", "menu");
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens a labelled menu of menu items on a press and focuses the first", () => {
    const { list } = items();
    const button = renderMenu(list);
    fireEvent.click(button);

    expect(button).toHaveAttribute("aria-expanded", "true");
    const menu = screen.getByRole("menu", { name: "Actions" });
    expect(button).toHaveAttribute("aria-controls", menu.id);
    expect(
      screen.getAllByRole("menuitem").map((item) => item.textContent),
    ).toEqual(["First", "Second", "Third"]);
    expect(focusedName()).toBe("First");
  });

  it("closes on a second press of the button", () => {
    const { list } = items();
    const button = renderMenu(list);
    fireEvent.click(button);
    fireEvent.click(button);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(button).toHaveAttribute("aria-expanded", "false");
  });

  it("opens with ArrowDown on the first item and ArrowUp on the last", () => {
    const { list } = items();
    const button = renderMenu(list);
    fireEvent.keyDown(button, { key: "ArrowDown" });
    expect(focusedName()).toBe("First");
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    fireEvent.keyDown(button, { key: "ArrowUp" });
    expect(focusedName()).toBe("Third");
  });

  it("moves between the items with ↑↓, wrapping round, and with Home and End", () => {
    const { list } = items();
    fireEvent.click(renderMenu(list));
    const menu = screen.getByRole("menu");

    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(focusedName()).toBe("Second");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(focusedName()).toBe("Third");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(focusedName()).toBe("First");
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(focusedName()).toBe("Third");
    fireEvent.keyDown(menu, { key: "Home" });
    expect(focusedName()).toBe("First");
    fireEvent.keyDown(menu, { key: "End" });
    expect(focusedName()).toBe("Third");
  });

  it("skips the items that cannot be chosen", () => {
    const { list } = items({ a: true, b: true });
    fireEvent.click(renderMenu(list));
    const menu = screen.getByRole("menu");

    expect(focusedName()).toBe("Third");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(focusedName()).toBe("Third");
    expect(screen.getByRole("menuitem", { name: "First" })).toBeDisabled();
  });

  it("chooses an item, closes and gives the focus back to the button", () => {
    const { list, onSelect } = items();
    const button = renderMenu(list);
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("menuitem", { name: "Second" }));

    expect(onSelect.b).toHaveBeenCalledTimes(1);
    expect(onSelect.a).not.toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(button).toHaveFocus();
  });

  it("does not choose an item that cannot be chosen", () => {
    const { list, onSelect } = items({ b: true });
    fireEvent.click(renderMenu(list));
    fireEvent.click(screen.getByRole("menuitem", { name: "Second" }));
    expect(onSelect.b).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("closes on Esc and gives the focus back to the button", () => {
    const { list } = items();
    const button = renderMenu(list);
    fireEvent.click(button);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    expect(screen.queryByRole("menu")).toBeNull();
    expect(button).toHaveFocus();
  });

  it("closes on a press outside, without taking the focus", () => {
    const { list } = items();
    renderMenu(list);
    fireEvent.click(screen.getByRole("button", { name: "Open menu" }));
    screen.getByRole("button", { name: "outside" }).focus();
    fireEvent.mouseDown(screen.getByRole("button", { name: "outside" }));

    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: "outside" })).toHaveFocus();
  });

  it("stays open on a press inside the menu", () => {
    const { list } = items();
    fireEvent.click(renderMenu(list));
    fireEvent.mouseDown(screen.getByRole("menuitem", { name: "Second" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("closes when Tab leaves the menu", () => {
    const { list } = items();
    fireEvent.click(renderMenu(list));
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Tab" });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("cannot be opened while disabled", () => {
    const { list } = items();
    const button = renderMenu(list, true);
    expect(button).toBeDisabled();
    fireEvent.click(button);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes an open menu when the button becomes disabled", () => {
    const { list } = items();
    const { rerender } = render(
      <MenuButton menuLabel="Actions" items={list}>
        Open menu
      </MenuButton>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Open menu" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    rerender(
      <MenuButton menuLabel="Actions" items={list} disabled>
        Open menu
      </MenuButton>,
    );
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("names the button with its label when given one", () => {
    render(
      <MenuButton menuLabel="View" items={items().list} label="View 54%">
        54%
      </MenuButton>,
    );
    expect(
      screen.getByRole("button", { name: "View 54%" }),
    ).toBeInTheDocument();
  });

  it("opens from the button's right edge, or its left edge with align start", () => {
    const { list } = items();
    render(
      <div>
        <MenuButton menuLabel="End" items={list}>
          End
        </MenuButton>
        <MenuButton menuLabel="Start" items={list} align="start">
          Start
        </MenuButton>
      </div>,
    );
    fireEvent.click(screen.getByRole("button", { name: "End" }));
    expect(screen.getByRole("menu", { name: "End" })).toHaveClass("is-end");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    expect(screen.getByRole("menu", { name: "Start" })).toHaveClass("is-start");
  });

  it("checks the items of a choice or a toggle, and sets items apart with a rule", () => {
    const onSelect = vi.fn();
    renderMenu([
      { id: "a", label: "Fit", checked: true, onSelect },
      {
        id: "b",
        label: "50%",
        checked: false,
        separatorBefore: true,
        onSelect,
      },
      { id: "c", label: "Plain", onSelect },
      { id: "d", label: "Grid", checked: true, toggles: true, onSelect },
    ]);
    fireEvent.click(screen.getByRole("button", { name: "Open menu" }));

    const menu = screen.getByRole("menu");
    expect(
      [...menu.children].map((child) => child.getAttribute("role")),
    ).toEqual([
      "menuitemradio",
      "separator",
      "menuitemradio",
      "menuitem",
      "menuitemcheckbox",
    ]);
    expect(screen.getByRole("menuitemcheckbox")).toHaveAttribute(
      "aria-checked",
      "true",
    );
    const [fit, half] = screen.getAllByRole("menuitemradio");
    expect(fit).toHaveAttribute("aria-checked", "true");
    expect(fit).toHaveTextContent("✓Fit");
    expect(half).toHaveAttribute("aria-checked", "false");
    expect(half).toHaveTextContent(/^50%$/);
    // The rule is not an item: the arrows go from one item to the next.
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(focusedName()).toBe("50%");
  });
});
