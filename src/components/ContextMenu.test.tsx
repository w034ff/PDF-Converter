import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ContextMenu } from "./ContextMenu";
import type { MenuButtonItem } from "./MenuButton";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const MENU_WIDTH = 200;
const MENU_HEIGHT = 80;

function items(): { list: MenuButtonItem[]; onSelect: () => void } {
  const onSelect = vi.fn();
  return {
    list: [
      { id: "a", label: "First", onSelect },
      { id: "b", label: "Second", disabled: true, onSelect: vi.fn() },
    ],
    onSelect,
  };
}

/** The menu measures this big; jsdom lays nothing out. */
function sizeMenus() {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(
    new DOMRect(0, 0, MENU_WIDTH, MENU_HEIGHT),
  );
}

describe("ContextMenu", () => {
  it("opens at the point with the first item focused", () => {
    sizeMenus();
    const { list } = items();
    render(
      <ContextMenu
        x={30}
        y={40}
        label="Actions"
        items={list}
        onClose={vi.fn()}
      />,
    );
    const menu = screen.getByRole("menu", { name: "Actions" });
    expect(menu).toHaveStyle({ left: "30px", top: "40px" });
    expect(screen.getByRole("menuitem", { name: "First" })).toHaveFocus();
  });

  it("moves back inside the window near its edges", () => {
    sizeMenus();
    const { list } = items();
    render(
      <ContextMenu
        x={window.innerWidth - 10}
        y={window.innerHeight - 10}
        label="Actions"
        items={list}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByRole("menu")).toHaveStyle({
      left: `${window.innerWidth - MENU_WIDTH - 4}px`,
      top: `${window.innerHeight - MENU_HEIGHT - 4}px`,
    });
  });

  it("closes with the focus to give back after Esc or a choice, and not after a press outside or a scroll", () => {
    const { list, onSelect } = items();
    const onClose = vi.fn();
    render(
      <ContextMenu
        x={0}
        y={0}
        label="Actions"
        items={list}
        onClose={onClose}
      />,
    );

    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(onClose).toHaveBeenLastCalledWith(true);
    fireEvent.click(screen.getByRole("menuitem", { name: "First" }));
    expect(onClose).toHaveBeenLastCalledWith(true);
    expect(onSelect).toHaveBeenCalledOnce();

    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenLastCalledWith(false);
    onClose.mockClear();
    fireEvent.mouseDown(screen.getByRole("menuitem", { name: "First" }));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.scroll(document);
    expect(onClose).toHaveBeenLastCalledWith(false);
  });
});
