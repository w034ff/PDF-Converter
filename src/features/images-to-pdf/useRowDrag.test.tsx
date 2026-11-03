import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { useRef } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { rowDropClass, useRowDrag } from "./useRowDrag";

function List({
  onMove,
  disabled = false,
}: {
  onMove: (fromIndex: number, toIndex: number) => void;
  disabled?: boolean;
}) {
  const listRef = useRef<HTMLOListElement>(null);
  const { dragIndex, overIndex, handlers } = useRowDrag(
    listRef,
    disabled,
    onMove,
  );
  return (
    <ol
      ref={listRef}
      data-testid="list"
      data-drag={dragIndex ?? "none"}
      data-over={overIndex ?? "none"}
      {...handlers}
    >
      {[0, 1, 2].map((index) => (
        <li key={index} className="list-row" data-testid={`row-${index}`}>
          <button type="button">remove {index}</button>
        </li>
      ))}
    </ol>
  );
}

/** Makes `document.elementFromPoint` answer `element`, as a browser would under the pointer. */
function pointAt(element: Element | null) {
  Object.defineProperty(document, "elementFromPoint", {
    configurable: true,
    value: () => element,
  });
}

describe("useRowDrag", () => {
  afterEach(() => {
    cleanup();
    Reflect.deleteProperty(document, "elementFromPoint");
  });

  it("follows the pointer by its position and drops on the row under it", () => {
    const onMove = vi.fn();
    render(<List onMove={onMove} />);
    const list = screen.getByTestId("list");

    fireEvent.pointerDown(screen.getByTestId("row-0"), { button: 0 });
    expect(list.dataset.drag).toBe("0");

    pointAt(screen.getByTestId("row-1"));
    act(() => {
      window.dispatchEvent(new MouseEvent("pointermove"));
    });
    expect(list.dataset.over).toBe("1");

    pointAt(screen.getByTestId("row-2"));
    act(() => {
      window.dispatchEvent(new MouseEvent("pointerup"));
    });
    expect(onMove).toHaveBeenCalledWith(0, 2);
    expect(list.dataset.drag).toBe("none");
    expect(list.dataset.over).toBe("none");
  });

  it("drops on the last row hovered when released outside the list", () => {
    const onMove = vi.fn();
    render(<List onMove={onMove} />);

    fireEvent.pointerDown(screen.getByTestId("row-2"), { button: 0 });
    pointAt(screen.getByTestId("row-0"));
    act(() => {
      window.dispatchEvent(new MouseEvent("pointermove"));
    });
    pointAt(document.body);
    act(() => {
      window.dispatchEvent(new MouseEvent("pointerup"));
    });

    expect(onMove).toHaveBeenCalledWith(2, 0);
  });

  it("moves nothing when dropped where it started", () => {
    const onMove = vi.fn();
    render(<List onMove={onMove} />);

    fireEvent.pointerDown(screen.getByTestId("row-1"), { button: 0 });
    fireEvent.pointerUp(screen.getByTestId("row-1"));

    expect(onMove).not.toHaveBeenCalled();
    expect(screen.getByTestId("list").dataset.drag).toBe("none");
  });

  it("puts the drag down on pointercancel", () => {
    const onMove = vi.fn();
    render(<List onMove={onMove} />);

    fireEvent.pointerDown(screen.getByTestId("row-0"), { button: 0 });
    fireEvent.pointerMove(screen.getByTestId("row-2"));
    act(() => {
      window.dispatchEvent(new MouseEvent("pointercancel"));
    });

    expect(onMove).not.toHaveBeenCalled();
    expect(screen.getByTestId("list").dataset.drag).toBe("none");
  });

  it("leaves a press on a row's button, another button and a disabled list alone", () => {
    const onMove = vi.fn();
    const { rerender } = render(<List onMove={onMove} />);
    const list = screen.getByTestId("list");

    fireEvent.pointerDown(screen.getByText("remove 0"), { button: 0 });
    expect(list.dataset.drag).toBe("none");

    fireEvent.pointerDown(screen.getByTestId("row-0"), { button: 2 });
    expect(list.dataset.drag).toBe("none");

    rerender(<List onMove={onMove} disabled />);
    fireEvent.pointerDown(screen.getByTestId("row-0"), { button: 0 });
    expect(list.dataset.drag).toBe("none");
  });
});

describe("rowDropClass", () => {
  it("marks the dragged row and the side it would go to", () => {
    expect(rowDropClass(1, null, null)).toBeUndefined();
    expect(rowDropClass(1, 1, 3)).toBe("is-dragging");
    expect(rowDropClass(3, 1, 3)).toBe("is-drop-after");
    expect(rowDropClass(0, 2, 0)).toBe("is-drop-before");
    expect(rowDropClass(2, 1, 3)).toBeUndefined();
  });
});
