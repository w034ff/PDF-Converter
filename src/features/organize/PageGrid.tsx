import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent as ReactFocusEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { getTranslations } from "../../i18n";
import {
  isJobActive,
  keyOfPage,
  useAppDispatch,
  useAppState,
  type OrganizePage,
  type PageKey,
} from "../../state";
import { PageCell } from "./PageCell";
import {
  ORGANIZE_PAGE_BOX_SIDE,
  cellOffset,
  dropLineLeft,
  focusIndexAfter,
  gridColumnCount,
  gridContentHeight,
  gridRowCount,
  indexRangeOfRows,
  isFocusKey,
  keyInsertIndex,
  rowOfIndex,
  scrollTopToCenterRow,
  scrollTopToShowRow,
  visibleRowRange,
} from "./organizeGrid";
import { useOrganizeDrag } from "./useOrganizeDrag";

/** Stands for the page the focus lands on after a deletion in `revealRef`. */
const FOCUS_LANDING = Symbol("focus-landing");

/** The size of the scrolling area's content box; zero until it is measured. */
interface Room {
  width: number;
  height: number;
}

function useRoom(scrollRef: React.RefObject<HTMLDivElement | null>): Room {
  const [room, setRoom] = useState<Room>({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (element === null) {
      return;
    }
    setRoom({ width: element.clientWidth, height: element.clientHeight });
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries.at(-1);
      if (entry !== undefined) {
        const { width, height } = entry.contentRect;
        setRoom({ width, height });
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [scrollRef]);
  return room;
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

/**
 * The grid of the pages (mockup OrganizeSingle, organize §4): fixed cells
 * placed by their position, of which only the rows in view, a few more, and
 * the one the Tab key enters are drawn, so ten thousand pages cost as much as
 * a hundred. It is a listbox that holds several selected pages, with one tab
 * stop that moves with the focus.
 */
export function PageGrid() {
  const { organize, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language).organize;
  const { pages, selected, focused, pdfs } = organize;
  const locked = isJobActive(job);

  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const room = useRoom(scrollRef);
  const [scrollTop, setScrollTop] = useState(0);
  const columns = gridColumnCount(room.width);
  const rows = gridRowCount(pages.length, columns);
  const range = visibleRowRange(scrollTop, room.height, rows);

  const pdfById = useMemo(
    () => new Map(pdfs.map((pdf) => [pdf.id, pdf])),
    [pdfs],
  );
  const indexByKey = useMemo(
    () => new Map(pages.map((page, index) => [keyOfPage(page), index])),
    [pages],
  );
  const tabStopKey =
    focused !== null && indexByKey.has(focused)
      ? focused
      : pages.length > 0
        ? keyOfPage(pages[0])
        : null;
  const tabStopIndex =
    tabStopKey === null ? -1 : (indexByKey.get(tabStopKey) ?? -1);

  const drag = useOrganizeDrag({
    scrollRef,
    contentRef,
    columns,
    count: pages.length,
    onMove: (keys, insertIndex) =>
      dispatch({ type: "ORGANIZE_PAGES_MOVED", keys, insertIndex }),
    onSelectOnly: (key) => dispatch({ type: "ORGANIZE_SELECTED_ONLY", key }),
  });

  // The page the keyboard moved or focused, shown and given the focus once the
  // state has changed. After a deletion it is whichever page the focus
  // landed on, which only the state knows.
  const revealRef = useRef<PageKey | typeof FOCUS_LANDING | null>(null);
  // A key at the edge of the grid changes no state, and what it asks for must
  // not wait for the next change that does (a press far away, say).
  const [revealTurn, setRevealTurn] = useState(0);
  function reveal(target: PageKey | typeof FOCUS_LANDING) {
    revealRef.current = target;
    setRevealTurn((turn) => turn + 1);
  }
  useLayoutEffect(() => {
    const wanted = revealRef.current;
    const scroller = scrollRef.current;
    if (wanted === null || scroller === null) {
      return;
    }
    revealRef.current = null;
    const key = wanted === FOCUS_LANDING ? focused : wanted;
    if (key === null) {
      return;
    }
    const index = indexByKey.get(key);
    if (index === undefined) {
      return;
    }
    const next = scrollTopToShowRow(
      rowOfIndex(index, columns),
      scroller.scrollTop,
      room.height,
    );
    if (next !== scroller.scrollTop) {
      scroller.scrollTop = next;
      setScrollTop(next);
    }
    const cell = scroller.querySelector<HTMLElement>(
      `[data-page-key="${CSS.escape(key)}"]`,
    );
    cell?.focus({ preventScroll: true });
  }, [revealTurn, focused, indexByKey, columns, room.height]);

  // The grid is drawn anew when its tab comes back, at the top. The page last
  // worked on is shown once the room is known; the focus stays where the tab
  // change left it.
  const restoredRef = useRef(false);
  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    if (restoredRef.current || room.width === 0 || scroller === null) {
      return;
    }
    restoredRef.current = true;
    const index = focused === null ? undefined : indexByKey.get(focused);
    if (index === undefined) {
      return;
    }
    const next = scrollTopToCenterRow(rowOfIndex(index, columns), room.height);
    if (next !== scroller.scrollTop) {
      scroller.scrollTop = next;
      setScrollTop(next);
    }
  }, [focused, indexByKey, columns, room.width, room.height]);

  // The latest selection and lock, for a handler that must not change with them:
  // a new handler would redraw every cell on each press.
  const latest = useRef({ selected, locked });
  useEffect(() => {
    latest.current = { selected, locked };
  });
  const startDrag = drag.start;

  const handleCellPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>, page: OrganizePage) => {
      if (event.button !== 0) {
        return;
      }
      const key = keyOfPage(page);
      if (event.shiftKey) {
        dispatch({ type: "ORGANIZE_SELECTION_EXTENDED", key });
        return;
      }
      if (event.ctrlKey || event.metaKey) {
        dispatch({ type: "ORGANIZE_SELECTION_TOGGLED", key });
        return;
      }
      const { selected: current, locked: isLocked } = latest.current;
      const isSelected = current.has(key);
      if (!isSelected) {
        dispatch({ type: "ORGANIZE_SELECTED_ONLY", key });
      }
      if (isLocked) {
        if (isSelected) {
          dispatch({ type: "ORGANIZE_SELECTED_ONLY", key });
        }
        return;
      }
      startDrag({
        key,
        // A selected page drags the whole selection, and is the selection
        // alone only if the press ends without a move: pressing it must not
        // take the others away before they can be dragged.
        keys: isSelected ? [...current] : [key],
        selectOnRelease: isSelected,
        clientX: event.clientX,
        clientY: event.clientY,
      });
    },
    [dispatch, startDrag],
  );

  function handleBackgroundPointerDown(
    event: ReactPointerEvent<HTMLDivElement>,
  ) {
    const scroller = scrollRef.current;
    if (
      event.button !== 0 ||
      scroller === null ||
      (event.target !== scroller && event.target !== contentRef.current)
    ) {
      return;
    }
    // The scroll bar is part of the element but not an empty place.
    const rect = scroller.getBoundingClientRect();
    if (
      event.clientX - rect.left >= scroller.clientWidth ||
      event.clientY - rect.top >= scroller.clientHeight
    ) {
      return;
    }
    dispatch({ type: "ORGANIZE_SELECTION_CLEARED" });
  }

  // The focus the pointer or the Tab key gives a cell is the grid's focus too.
  function handleFocus(event: ReactFocusEvent<HTMLDivElement>) {
    const key =
      event.target instanceof HTMLElement
        ? event.target.dataset.pageKey
        : undefined;
    if (key !== undefined) {
      dispatch({ type: "ORGANIZE_FOCUSED", key });
    }
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
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
    if (event.ctrlKey && !event.altKey && key.toLowerCase() === "a") {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_SELECTED_ALL" });
      return;
    }
    if (event.ctrlKey || event.altKey || event.metaKey) {
      return;
    }
    if (key === " ") {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_SELECTION_TOGGLED", key: current });
    } else if (key === "Escape") {
      event.preventDefault();
      dispatch({ type: "ORGANIZE_SELECTION_CLEARED" });
    } else if (!locked && (key === "r" || key === "R")) {
      event.preventDefault();
      dispatch({
        type: "ORGANIZE_PAGES_ROTATED",
        keys: operatedKeys(selected, current),
        direction: "right",
      });
    } else if (!locked && (key === "l" || key === "L")) {
      event.preventDefault();
      dispatch({
        type: "ORGANIZE_PAGES_ROTATED",
        keys: operatedKeys(selected, current),
        direction: "left",
      });
    } else if (!locked && (key === "Delete" || key === "Backspace")) {
      event.preventDefault();
      // The cell that has the focus is the one going, and the focus must not be lost with it.
      reveal(FOCUS_LANDING);
      dispatch({
        type: "ORGANIZE_PAGES_DELETED",
        keys: operatedKeys(selected, current),
      });
    }
  }

  function renderCell(index: number) {
    const page = pages[index];
    const key = keyOfPage(page);
    const { left, top } = cellOffset(index, columns);
    return (
      <PageCell
        key={key}
        page={page}
        index={index}
        count={pages.length}
        pdf={pdfById.get(page.id)}
        selected={selected.has(key)}
        tabStop={index === tabStopIndex}
        left={left}
        top={top}
        t={t}
        onPointerDown={handleCellPointerDown}
      />
    );
  }

  const cells: React.ReactNode[] = [];
  if (range !== null) {
    const { start, end } = indexRangeOfRows(range, columns, pages.length);
    for (let index = start; index < end; index += 1) {
      cells.push(renderCell(index));
    }
    // The page Tab enters stays drawn out of view, or the grid could not be entered.
    if (tabStopIndex >= 0 && (tabStopIndex < start || tabStopIndex >= end)) {
      cells.push(renderCell(tabStopIndex));
    }
  }

  const { dropPoint } = drag;

  return (
    <div
      ref={scrollRef}
      className="organize-grid-scroll"
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      onPointerDown={handleBackgroundPointerDown}
    >
      <div
        ref={contentRef}
        role="listbox"
        aria-multiselectable="true"
        aria-label={t.grid.aria}
        className={`organize-grid${drag.dragging ? " is-dragging" : ""}`}
        style={{ height: gridContentHeight(rows) }}
        onKeyDown={handleKeyDown}
        onFocus={handleFocus}
      >
        {cells}
        {dropPoint !== null && (
          <div
            className="organize-drop-line"
            aria-hidden="true"
            style={{
              left: dropLineLeft(dropPoint.slot),
              top: cellOffset(dropPoint.row * columns, columns).top,
              height: ORGANIZE_PAGE_BOX_SIDE,
            }}
          />
        )}
      </div>
    </div>
  );
}
