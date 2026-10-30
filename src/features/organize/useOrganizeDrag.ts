import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import type { PageKey } from "../../state";
import { DRAG_THRESHOLD_PX } from "../shared/pointer";
import {
  dragScrollStep,
  insertPointAt,
  type InsertPoint,
} from "./organizeGrid";

export interface OrganizeDragOptions {
  /** The scrolling element; a drag near its top or bottom edge scrolls it. */
  scrollRef: RefObject<HTMLElement | null>;
  /** The element the cells are placed in, whose top left is where cell 0 starts. */
  contentRef: RefObject<HTMLElement | null>;
  columns: number;
  count: number;
  /** Puts the pages of `keys` at `insertIndex` of the grid as it was when the drag began. */
  onMove: (keys: readonly PageKey[], insertIndex: number) => void;
  /** A press on a selected page that did not move: it alone becomes the selection. */
  onSelectOnly: (key: PageKey) => void;
}

export interface PagePress {
  /** The page pressed. */
  key: PageKey;
  /** The pages a drag would move: the selection, or the pressed page alone. */
  keys: readonly PageKey[];
  /** Whether the release without a move should select the page alone. */
  selectOnRelease: boolean;
  clientX: number;
  clientY: number;
}

interface Press extends PagePress {
  passed: boolean;
  point: InsertPoint | null;
  frame: number | null;
}

function sameInsertPoint(
  a: InsertPoint | null,
  b: InsertPoint | null,
): boolean {
  return (
    a === b ||
    (a !== null &&
      b !== null &&
      a.index === b.index &&
      a.row === b.row &&
      a.slot === b.slot)
  );
}

/**
 * Drags pages with the pointer (organize §4). The press is a click until the
 * pointer has moved `DRAG_THRESHOLD_PX`; then the pages of the press follow
 * the pointer as a line between cells, the grid scrolls when the pointer is
 * near its edge, and the grid changes once, on release. Esc, `pointercancel`
 * and a window that loses the focus put it down. While pages are dragged no
 * key reaches anything else.
 *
 * The place the pages go is worked out from the pointer's position and the
 * number of columns, not from the DOM, since the cells out of view are not
 * there.
 */
export function useOrganizeDrag(options: OrganizeDragOptions) {
  const [dropPoint, setDropPoint] = useState<InsertPoint | null>(null);
  const [dragging, setDragging] = useState(false);
  const pressRef = useRef<Press | null>(null);
  const stopRef = useRef<(() => void) | null>(null);
  const latest = useRef(options);
  useEffect(() => {
    latest.current = options;
  });

  const refreshPoint = useCallback((press: Press) => {
    const { contentRef, columns, count } = latest.current;
    const rect = contentRef.current?.getBoundingClientRect();
    if (rect === undefined) {
      return;
    }
    const point = insertPointAt(
      press.clientX - rect.left,
      press.clientY - rect.top,
      columns,
      count,
    );
    if (!sameInsertPoint(press.point, point)) {
      press.point = point;
      setDropPoint(point);
    }
  }, []);

  const start = useCallback(
    (press: PagePress) => {
      stopRef.current?.();
      const current: Press = {
        ...press,
        passed: false,
        point: null,
        frame: null,
      };
      pressRef.current = current;
      const startX = press.clientX;
      const startY = press.clientY;

      function scrollFrame() {
        const scroller = latest.current.scrollRef.current;
        if (scroller !== null) {
          const rect = scroller.getBoundingClientRect();
          const step = dragScrollStep(current.clientY, rect.top, rect.bottom);
          if (step !== 0) {
            scroller.scrollTop += step;
          }
          // Every frame: the wheel scrolls the grid under a pointer that does not move.
          refreshPoint(current);
        }
        current.frame = requestAnimationFrame(scrollFrame);
      }

      function end(commit: boolean) {
        window.removeEventListener("pointermove", handleMove);
        window.removeEventListener("pointerup", handleUp);
        window.removeEventListener("pointercancel", handleCancel);
        window.removeEventListener("keydown", handleKey, true);
        window.removeEventListener("blur", handleCancel);
        if (current.frame !== null) {
          cancelAnimationFrame(current.frame);
        }
        pressRef.current = null;
        stopRef.current = null;
        setDropPoint(null);
        setDragging(false);
        if (!commit) {
          return;
        }
        if (current.passed) {
          if (current.point !== null) {
            latest.current.onMove(current.keys, current.point.index);
          }
        } else if (current.selectOnRelease) {
          latest.current.onSelectOnly(current.key);
        }
      }

      function handleMove(event: PointerEvent) {
        current.clientX = event.clientX;
        current.clientY = event.clientY;
        if (!current.passed) {
          if (
            Math.hypot(event.clientX - startX, event.clientY - startY) <
            DRAG_THRESHOLD_PX
          ) {
            return;
          }
          current.passed = true;
          setDragging(true);
          current.frame = requestAnimationFrame(scrollFrame);
        }
        refreshPoint(current);
      }

      function handleUp() {
        end(true);
      }

      function handleCancel() {
        end(false);
      }

      function handleKey(event: KeyboardEvent) {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          end(false);
        } else if (current.passed) {
          event.stopPropagation();
        }
      }

      window.addEventListener("pointermove", handleMove);
      window.addEventListener("pointerup", handleUp);
      window.addEventListener("pointercancel", handleCancel);
      window.addEventListener("keydown", handleKey, true);
      window.addEventListener("blur", handleCancel);
      stopRef.current = () => end(false);
    },
    [refreshPoint],
  );

  // A drag does not outlive the grid.
  useEffect(() => () => stopRef.current?.(), []);

  return { start, dropPoint, dragging };
}
