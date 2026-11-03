import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type FocusEvent as ReactFocusEvent,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type Ref,
} from "react";
import { contextMenuPoint } from "../../components";
import { getTranslations } from "../../i18n";
import { useRoom } from "../shared/useRoom";
import {
  isJobActive,
  keyOfPage,
  partNumbers,
  useAppDispatch,
  useAppState,
  type OrganizePage,
  type PageKey,
} from "../../state";
import { OrganizePageMenu } from "./OrganizePageMenu";
import { PageBreak } from "./PageBreak";
import { PageCell } from "./PageCell";
import {
  ORGANIZE_PAGE_BOX_SIDE,
  cellOffset,
  dropLineLeft,
  gridColumnCount,
  gridContentHeight,
  gridRowCount,
  indexRangeOfRows,
  visibleRowRange,
} from "./organizeGrid";
import { FOCUS_LANDING, useGridReveal } from "./useGridReveal";
import { useOrganizeDrag } from "./useOrganizeDrag";
import { useOrganizeGridKeys } from "./useOrganizeGridKeys";

const NO_PARTS: ReadonlyMap<PageKey, number> = new Map();

/** The menu of a page, open at a point. */
interface PageMenu {
  x: number;
  y: number;
  key: PageKey;
}

/** What the band needs of the grid. */
export interface PageGridHandle {
  /** To call with the deletion, which leaves the focus to land in the grid. */
  focusAfterDeleting: () => void;
}

/**
 * The grid of the pages (mockup OrganizeSingle, organize §4): fixed cells
 * placed by their position, of which only the rows in view, a few more, and
 * the one the Tab key enters are drawn, so ten thousand pages cost as much as
 * a hundred. It is a listbox that holds several selected pages, with one tab
 * stop that moves with the focus.
 */
export function PageGrid({ ref }: { ref?: Ref<PageGridHandle> }) {
  const { organize, job, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language).organize;
  const { pages, selected, focused, pdfs, breaks } = organize;
  const locked = isJobActive(job);
  const isSplit = organize.output === "split";
  // The result of the save in parts, whose failed outputs are told on their badges.
  const results =
    job.kind === "organize" && job.organizeOutput === "split"
      ? job.results
      : null;

  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const room = useRoom(scrollRef);
  const columns = gridColumnCount(room.width);
  const rows = gridRowCount(pages.length, columns);

  const pdfById = useMemo(
    () => new Map(pdfs.map((pdf) => [pdf.id, pdf])),
    [pdfs],
  );
  const indexByKey = useMemo(
    () => new Map(pages.map((page, index) => [keyOfPage(page), index])),
    [pages],
  );
  // Worked out once for the pages, not for the cells in view: every scroll
  // draws the grid again.
  const parts = useMemo(
    () => (isSplit ? partNumbers(pages, breaks) : NO_PARTS),
    [isSplit, pages, breaks],
  );
  const tabStopKey =
    focused !== null && indexByKey.has(focused)
      ? focused
      : pages.length > 0
        ? keyOfPage(pages[0])
        : null;
  const tabStopIndex =
    tabStopKey === null ? -1 : (indexByKey.get(tabStopKey) ?? -1);

  const { scrollTop, setScrollTop, reveal } = useGridReveal({
    scrollRef,
    focused,
    indexByKey,
    columns,
    room,
  });
  const range = visibleRowRange(scrollTop, room.height, rows);

  const drag = useOrganizeDrag({
    scrollRef,
    contentRef,
    columns,
    count: pages.length,
    onMove: (keys, insertIndex) =>
      dispatch({ type: "ORGANIZE_PAGES_MOVED", keys, insertIndex }),
    onSelectOnly: (key) => dispatch({ type: "ORGANIZE_SELECTED_ONLY", key }),
  });

  useImperativeHandle(ref, () => ({
    focusAfterDeleting: () => reveal(FOCUS_LANDING),
  }));

  const [menu, setMenu] = useState<PageMenu | null>(null);
  // A save takes the pages; the menu does not wait for it.
  if (menu !== null && locked) {
    setMenu(null);
  }

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

  /**
   * The browser's own menu has nothing to do on the grid, so it never shows
   * there. On a page the menu works like the band: on the selection, of which
   * a page outside it becomes the only one.
   */
  function handleContextMenu(event: ReactMouseEvent<HTMLDivElement>) {
    event.preventDefault();
    const cell =
      event.target instanceof Element
        ? event.target.closest<HTMLElement>("[data-page-key]")
        : null;
    const key = cell?.dataset.pageKey;
    if (locked || drag.dragging || !cell || key === undefined) {
      return;
    }
    if (!selected.has(key)) {
      dispatch({ type: "ORGANIZE_SELECTED_ONLY", key });
    }
    setMenu({ ...contextMenuPoint(event, cell), key });
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

  const { handleKeyDown } = useOrganizeGridKeys({
    pages,
    selected,
    tabStopKey,
    indexByKey,
    columns,
    locked,
    isSplit,
    reveal,
  });

  function renderBreak(index: number) {
    const key = keyOfPage(pages[index]);
    return (
      <PageBreak
        key={`break-${key}`}
        before={pages[index - 1]}
        after={pages[index]}
        index={index}
        columns={columns}
        roomWidth={room.width}
        isBreak={breaks.has(key)}
        disabled={locked || drag.dragging}
        pdfById={pdfById}
        t={t}
      />
    );
  }

  function renderCell(index: number) {
    const page = pages[index];
    const key = keyOfPage(page);
    const { left, top } = cellOffset(index, columns);
    const part = parts.get(key) ?? null;
    return (
      <PageCell
        key={key}
        page={page}
        index={index}
        count={pages.length}
        pdf={pdfById.get(page.id)}
        selected={selected.has(key)}
        tabStop={index === tabStopIndex}
        part={part}
        partFailed={
          part !== null &&
          results !== null &&
          results[part]?.status === "failed"
        }
        left={left}
        top={top}
        t={t}
        onPointerDown={handleCellPointerDown}
      />
    );
  }

  // The places that press a break come before the cells: a badge that
  // reaches into the gap is the cell's, and a press on it is a press on the page.
  const breakNodes: React.ReactNode[] = [];
  const cells: React.ReactNode[] = [];
  if (range !== null) {
    const { start, end } = indexRangeOfRows(range, columns, pages.length);
    if (isSplit) {
      for (let index = Math.max(start, 1); index < end; index += 1) {
        breakNodes.push(renderBreak(index));
      }
    }
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
      onContextMenu={handleContextMenu}
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
        {breakNodes}
        {cells}
        {dropPoint !== null && (
          <div
            className="organize-drop-line"
            aria-hidden="true"
            style={{
              left: dropLineLeft(dropPoint.slot, room.width),
              top: cellOffset(dropPoint.row * columns, columns).top,
              height: ORGANIZE_PAGE_BOX_SIDE,
            }}
          />
        )}
      </div>
      {menu !== null && (
        <OrganizePageMenu
          x={menu.x}
          y={menu.y}
          pageKey={menu.key}
          onDelete={() => reveal(FOCUS_LANDING)}
          onClose={(returnFocus) => {
            setMenu(null);
            if (returnFocus) {
              reveal(menu.key);
            }
          }}
        />
      )}
    </div>
  );
}
