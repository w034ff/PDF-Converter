import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from "react";
import type { PageKey } from "../../state";
import type { Room } from "../shared/useRoom";
import {
  rowOfIndex,
  scrollTopToCenterRow,
  scrollTopToShowRow,
} from "./organizeGrid";

/** Stands for the page the focus lands on after a deletion in `revealRef`. */
export const FOCUS_LANDING = Symbol("focus-landing");

/** What `reveal` is asked to show: a page, or the one the focus lands on. */
export type RevealTarget = PageKey | typeof FOCUS_LANDING;

export interface GridRevealOptions {
  /** The scrolling element of the grid. */
  scrollRef: RefObject<HTMLElement | null>;
  /** The page with the grid's focus, which `FOCUS_LANDING` stands for. */
  focused: PageKey | null;
  indexByKey: ReadonlyMap<PageKey, number>;
  columns: number;
  room: Room;
}

export interface GridReveal {
  /** How far the grid is scrolled, for the rows to draw. */
  scrollTop: number;
  /** To call when the user scrolls; it is also set by the reveals. */
  setScrollTop: (scrollTop: number) => void;
  /** Shows a page and gives it the focus once the state has changed. */
  reveal: (target: RevealTarget) => void;
}

/**
 * Where the grid is scrolled to, and what moves it by itself: a page the
 * keyboard moved or focused is shown and given the focus, and the page last
 * worked on is shown when the tab comes back.
 */
export function useGridReveal({
  scrollRef,
  focused,
  indexByKey,
  columns,
  room,
}: GridRevealOptions): GridReveal {
  const [scrollTop, setScrollTop] = useState(0);
  const scrollTo = useCallback((scroller: HTMLElement, next: number) => {
    if (next !== scroller.scrollTop) {
      scroller.scrollTop = next;
      setScrollTop(next);
    }
  }, []);

  // The page the keyboard moved or focused, shown and given the focus once the
  // state has changed. After a deletion it is whichever page the focus
  // landed on, which only the state knows.
  const revealRef = useRef<RevealTarget | null>(null);
  // A key at the edge of the grid changes no state, and what it asks for must
  // not wait for the next change that does (a press far away, say).
  const [revealTurn, setRevealTurn] = useState(0);
  function reveal(target: RevealTarget) {
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
    scrollTo(
      scroller,
      scrollTopToShowRow(
        rowOfIndex(index, columns),
        scroller.scrollTop,
        room.height,
      ),
    );
    const cell = scroller.querySelector<HTMLElement>(
      `[data-page-key="${CSS.escape(key)}"]`,
    );
    cell?.focus({ preventScroll: true });
  }, [
    scrollRef,
    scrollTo,
    revealTurn,
    focused,
    indexByKey,
    columns,
    room.height,
  ]);

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
    scrollTo(
      scroller,
      scrollTopToCenterRow(rowOfIndex(index, columns), room.height),
    );
  }, [
    scrollRef,
    scrollTo,
    focused,
    indexByKey,
    columns,
    room.width,
    room.height,
  ]);

  return { scrollTop, setScrollTop, reveal };
}
