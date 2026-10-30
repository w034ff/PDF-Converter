import type {
  IpcError,
  OrganizeOutput,
  OrganizePdf,
  OrganizeSettings,
  OutputDirLabel,
  Skipped,
} from "../ipc";

/**
 * The most pages one output has, as `MAX_PDF_PAGES` in `crates/worker/src/lib.rs`:
 * the app opens no PDF with more (design organize §2). The grid itself has no limit.
 */
export const MAX_PDF_PAGES = 10_000;

/** How many colors tell the PDFs apart; they repeat in the order PDFs were added (organize §4). */
export const ORGANIZE_SOURCE_COLORS = 6;

/** How many steps of page operations are kept for undo (organize §4). */
export const ORGANIZE_HISTORY_LIMIT = 100;

/** The turn of one rotation button, in degrees. */
export const ORGANIZE_QUARTER_TURN = 90;

const FULL_TURN = 360;

/** An organize PDF in the list, with the number of its color. */
export interface OrganizePdfEntry extends OrganizePdf {
  /** The place among the added PDFs, from 0, modulo `ORGANIZE_SOURCE_COLORS`. */
  colorIndex: number;
}

/** A page of the grid; the same fields as `OrganizePageInput`, which is what is saved. */
export interface OrganizePage {
  id: number;
  /** From 1, as in the PDF. */
  page: number;
  /** Clockwise degrees the page is turned by: 0, 90, 180 or 270. */
  rotation: number;
}

/** What identifies a page: a page is in the grid once (organize §4). */
export type PageKey = string;

export function pageKey(id: number, page: number): PageKey {
  return `${id}:${page}`;
}

export function keyOfPage(page: OrganizePage): PageKey {
  return pageKey(page.id, page.page);
}

/** What the last addition left out or failed on, for the band above the grid. */
export interface OrganizeNotice {
  skipped: Skipped | null;
  error: IpcError | null;
}

/** The pages and the breaks between them, which undo and redo put back together (organize §4). */
export interface OrganizeStep {
  pages: OrganizePage[];
  breaks: ReadonlySet<PageKey>;
}

/** What undo and redo go back to: the grids before and after the page operations. */
export interface OrganizeHistory {
  /** The grid before each operation, the latest last. */
  past: OrganizeStep[];
  /** The grid before each undo, the latest undone last. */
  future: OrganizeStep[];
  /** Whether the oldest operations were dropped for `ORGANIZE_HISTORY_LIMIT`. */
  trimmed: boolean;
}

export interface OrganizeState {
  /** In the order they were added. */
  pdfs: OrganizePdfEntry[];
  /** How many PDFs were added since the list was empty, which numbers the next color. */
  addedCount: number;
  /** The grid, in order. */
  pages: OrganizePage[];
  /**
   * The pages that start a new output when the pages are saved in parts: a
   * break belongs to the page after it (organize §4). Every key is a page of
   * `pages` but not the first.
   */
  breaks: ReadonlySet<PageKey>;
  /** None of the next four is in the history. */
  selected: ReadonlySet<PageKey>;
  /** Where Shift extends the selection from. */
  anchor: PageKey | null;
  /** The page the Tab key enters the grid at; `null` is the first page. */
  focused: PageKey | null;
  history: OrganizeHistory;
  /** Whether `pages` holds page operations, which "すべて外す" would lose. */
  edited: boolean;
  /** What the last addition left out or failed on. */
  notice: OrganizeNotice | null;
  encrypt: boolean;
  output: OrganizeOutput;
  /** The folder the save in parts writes to; `null` until one is chosen. */
  outputDir: OutputDirLabel | null;
}

export type OrganizeAction =
  | { type: "ORGANIZE_PDFS_ADDED"; items: OrganizePdf[] }
  /** Takes a PDF out of the list and its pages out of the grid and every step of the history. */
  | { type: "ORGANIZE_PDF_REMOVED"; id: number }
  /** The same as removal, for a PDF that a save wrote over: it is not a change the result tells of. */
  | { type: "ORGANIZE_PDF_REPLACED"; id: number }
  /** Takes the PDFs a password opened (or tried), and adds the pages of those that opened. */
  | { type: "ORGANIZE_PDFS_UNLOCKED"; items: OrganizePdf[] }
  /** Empties the list, the grid and the history; the settings stay. */
  | { type: "ORGANIZE_CLEARED" }
  | { type: "ORGANIZE_NOTICE"; notice: OrganizeNotice | null }
  /** A press: only this page is selected, and the next Shift starts here. */
  | { type: "ORGANIZE_SELECTED_ONLY"; key: PageKey }
  /** Ctrl and a press, or Space: puts the page in or out of the selection. */
  | { type: "ORGANIZE_SELECTION_TOGGLED"; key: PageKey }
  /** Shift and a press: selects from the anchor to the page, and nothing else. */
  | { type: "ORGANIZE_SELECTION_EXTENDED"; key: PageKey }
  | { type: "ORGANIZE_SELECTED_ALL" }
  | { type: "ORGANIZE_SELECTION_CLEARED" }
  /**
   * The focus moves to a page. With `extend` (Shift and an arrow) the pages
   * from the anchor to it are selected as well.
   */
  | { type: "ORGANIZE_FOCUSED"; key: PageKey; extend?: boolean }
  /** Moves the pages of `keys`, as one block in their order, to `insertIndex` of the grid before the move. */
  | {
      type: "ORGANIZE_PAGES_MOVED";
      keys: readonly PageKey[];
      insertIndex: number;
    }
  | {
      type: "ORGANIZE_PAGES_ROTATED";
      keys: readonly PageKey[];
      direction: "left" | "right";
    }
  | { type: "ORGANIZE_PAGES_DELETED"; keys: readonly PageKey[] }
  /** Puts the break before the page in or out; the first page has none. */
  | { type: "ORGANIZE_BREAK_TOGGLED"; key: PageKey }
  /** Replaces the breaks by one before every page but the first. */
  | { type: "ORGANIZE_BREAKS_EACH_PAGE" }
  /** Replaces the breaks by one where a page comes from another PDF than the one before it. */
  | { type: "ORGANIZE_BREAKS_EACH_PDF" }
  | { type: "ORGANIZE_BREAKS_CLEARED" }
  | { type: "ORGANIZE_UNDO" }
  | { type: "ORGANIZE_REDO" }
  | { type: "SET_ORGANIZE_ENCRYPT"; encrypt: boolean }
  | { type: "SET_ORGANIZE_OUTPUT"; output: OrganizeOutput }
  | { type: "SET_ORGANIZE_OUTPUT_DIR"; outputDir: OutputDirLabel | null };

const EMPTY_HISTORY: OrganizeHistory = {
  past: [],
  future: [],
  trimmed: false,
};

const NO_KEYS: ReadonlySet<PageKey> = new Set();

/** The organize state before anything is added, from the saved organize `settings` if there are any. */
export function createInitialOrganizeState(
  settings: OrganizeSettings | null = null,
): OrganizeState {
  return {
    pdfs: [],
    addedCount: 0,
    pages: [],
    breaks: NO_KEYS,
    selected: NO_KEYS,
    anchor: null,
    focused: null,
    history: EMPTY_HISTORY,
    edited: false,
    notice: null,
    encrypt: settings?.encrypt ?? false,
    output: settings?.output ?? "single",
    outputDir: settings?.outputDir ?? null,
  };
}

/** The pages a PDF puts in the grid when it is added: all of them, not turned. A PDF with an error has none. */
function pagesOfPdf(pdf: OrganizePdf): OrganizePage[] {
  if (pdf.error !== null) {
    return [];
  }
  return Array.from({ length: pdf.pageCount }, (_, index) => ({
    id: pdf.id,
    page: index + 1,
    rotation: 0,
  }));
}

/** `rotation` turned by `degrees`, kept within 0 to 270. */
export function turnRotation(rotation: number, degrees: number): number {
  return (((rotation + degrees) % FULL_TURN) + FULL_TURN) % FULL_TURN;
}

function samePages(a: readonly OrganizePage[], b: readonly OrganizePage[]) {
  return a.length === b.length && a.every((page, index) => page === b[index]);
}

function sameBreaks(a: ReadonlySet<PageKey>, b: ReadonlySet<PageKey>) {
  return a.size === b.size && [...a].every((key) => b.has(key));
}

function sameStep(a: OrganizeStep, b: OrganizeStep) {
  return samePages(a.pages, b.pages) && sameBreaks(a.breaks, b.breaks);
}

function stepOf(state: OrganizeState): OrganizeStep {
  return { pages: state.pages, breaks: state.breaks };
}

/**
 * Whether the page at `index` of `pages` starts an output when they are saved
 * in parts: the first page does, and so does a page that holds a break.
 */
function startsPart(
  index: number,
  page: OrganizePage,
  breaks: ReadonlySet<PageKey>,
): boolean {
  return index === 0 || breaks.has(keyOfPage(page));
}

/**
 * The outputs the pages make when they are saved in parts: each begins at the
 * first page and at each page that holds a break, and goes on to the page
 * before the next. No pages make no output.
 */
export function splitIntoParts(
  pages: readonly OrganizePage[],
  breaks: ReadonlySet<PageKey>,
): OrganizePage[][] {
  const parts: OrganizePage[][] = [];
  pages.forEach((page, index) => {
    if (startsPart(index, page, breaks)) {
      parts.push([page]);
    } else {
      parts[parts.length - 1].push(page);
    }
  });
  return parts;
}

/**
 * The number of the output (from 1, as in `splitIntoParts`) each page that
 * starts one begins; the other pages are not in it.
 */
export function partNumbers(
  pages: readonly OrganizePage[],
  breaks: ReadonlySet<PageKey>,
): ReadonlyMap<PageKey, number> {
  const numbers = new Map<PageKey, number>();
  pages.forEach((page, index) => {
    if (startsPart(index, page, breaks)) {
      numbers.set(keyOfPage(page), numbers.size + 1);
    }
  });
  return numbers;
}

/**
 * The breaks that are left once the pages of `leaving` are gone from `pages`,
 * or are put somewhere else: the break of a leaving page goes to the first
 * page after it that stays, and is dropped when none does, so that taking a
 * page out never puts a boundary between two pages that were not touched
 * (organize §4). A leaving page holds no break afterwards.
 */
function breaksHandedOver(
  pages: readonly OrganizePage[],
  breaks: ReadonlySet<PageKey>,
  leaving: ReadonlySet<PageKey>,
): Set<PageKey> {
  const handed = new Set<PageKey>();
  let pending = false;
  for (const page of pages) {
    const key = keyOfPage(page);
    if (leaving.has(key)) {
      pending = pending || breaks.has(key);
    } else {
      if (pending || breaks.has(key)) {
        handed.add(key);
      }
      pending = false;
    }
  }
  return handed;
}

/** `breaks` without the first of `pages`, which starts an output with or without one. */
function breaksBelowFirst(
  pages: readonly OrganizePage[],
  breaks: ReadonlySet<PageKey>,
): ReadonlySet<PageKey> {
  if (pages.length === 0 || !breaks.has(keyOfPage(pages[0]))) {
    return breaks;
  }
  const rest = new Set(breaks);
  rest.delete(keyOfPage(pages[0]));
  return rest;
}

function indexOfKeys(pages: readonly OrganizePage[]): Map<PageKey, number> {
  return new Map(pages.map((page, index) => [keyOfPage(page), index]));
}

/**
 * Moves the pages whose keys are in `selected` to `insertIndex`, a position
 * of `pages` as it is before the move, keeping them in their order as one
 * block. The pages around it keep theirs. The same array comes back when
 * nothing moves.
 */
export function moveSelected(
  pages: readonly OrganizePage[],
  selected: ReadonlySet<PageKey>,
  insertIndex: number,
): OrganizePage[] {
  const at = Math.min(Math.max(insertIndex, 0), pages.length);
  const block: OrganizePage[] = [];
  const before: OrganizePage[] = [];
  const after: OrganizePage[] = [];
  pages.forEach((page, index) => {
    if (selected.has(keyOfPage(page))) {
      block.push(page);
    } else {
      (index < at ? before : after).push(page);
    }
  });
  const moved = [...before, ...block, ...after];
  return samePages(moved, pages) ? pages.slice() : moved;
}

/** Whether `moveSelected` leaves `pages` as it is, which is no step of the history. */
function moveChanges(
  pages: readonly OrganizePage[],
  moved: readonly OrganizePage[],
): boolean {
  return !samePages(pages, moved);
}

/** The keys among `keys` that are pages of the grid, as a set. */
function keysInGrid(
  pages: readonly OrganizePage[],
  keys: readonly PageKey[],
): ReadonlySet<PageKey> {
  const wanted = new Set(keys);
  return new Set(pages.map(keyOfPage).filter((key) => wanted.has(key)));
}

/** Pushes the grid `before` an operation onto the history; the future is gone. */
function withStep(
  state: OrganizeState,
  before: OrganizeStep,
  next: OrganizeStep,
): OrganizeState {
  const past = [...state.history.past, before];
  const overflow = past.length - ORGANIZE_HISTORY_LIMIT;
  return {
    ...state,
    pages: next.pages,
    breaks: next.breaks,
    edited: true,
    history: {
      past: overflow > 0 ? past.slice(overflow) : past,
      future: [],
      trimmed: state.history.trimmed || overflow > 0,
    },
  };
}

/**
 * Takes out of the selection, the anchor and the focus what is no page of the
 * grid any more, and puts the focus on the first page if it has none.
 */
function tidied(state: OrganizeState): OrganizeState {
  const inGrid = new Set(state.pages.map(keyOfPage));
  const selected = [...state.selected].filter((key) => inGrid.has(key));
  const focused =
    state.focused !== null && inGrid.has(state.focused)
      ? state.focused
      : state.pages.length > 0
        ? keyOfPage(state.pages[0])
        : null;
  return {
    ...state,
    selected:
      selected.length === state.selected.size
        ? state.selected
        : new Set(selected),
    anchor:
      state.anchor !== null && inGrid.has(state.anchor) ? state.anchor : null,
    focused,
  };
}

/** The step without the pages of the PDF `id`, whose breaks go to the pages that follow them. */
function withPdfPagesRemoved(step: OrganizeStep, id: number): OrganizeStep {
  const leaving = new Set(
    step.pages.filter((page) => page.id === id).map(keyOfPage),
  );
  if (leaving.size === 0) {
    return step;
  }
  const pages = step.pages.filter((page) => page.id !== id);
  return {
    pages,
    breaks: breaksBelowFirst(
      pages,
      breaksHandedOver(step.pages, step.breaks, leaving),
    ),
  };
}

function removePdf(state: OrganizeState, id: number): OrganizeState {
  if (!state.pdfs.some((pdf) => pdf.id === id)) {
    return state;
  }
  const keyIsOfPdf = (key: PageKey) => key.startsWith(`${id}:`);
  const pdfs = state.pdfs.filter((pdf) => pdf.id !== id);
  if (pdfs.length === 0) {
    // Nothing is left to undo or to lose.
    return {
      ...createInitialOrganizeState(),
      notice: state.notice,
      encrypt: state.encrypt,
      output: state.output,
      outputDir: state.outputDir,
    };
  }
  const selected = [...state.selected].filter((key) => !keyIsOfPdf(key));
  const rest = withPdfPagesRemoved(stepOf(state), id);
  return {
    ...state,
    pdfs,
    pages: rest.pages,
    breaks: rest.breaks,
    selected:
      selected.length === state.selected.size
        ? state.selected
        : new Set(selected),
    anchor:
      state.anchor !== null && keyIsOfPdf(state.anchor) ? null : state.anchor,
    focused:
      state.focused !== null && keyIsOfPdf(state.focused)
        ? null
        : state.focused,
    history: {
      ...state.history,
      past: state.history.past.map((step) => withPdfPagesRemoved(step, id)),
      future: state.history.future.map((step) => withPdfPagesRemoved(step, id)),
    },
  };
}

function withPagesAdded(
  step: OrganizeStep,
  added: OrganizePage[],
): OrganizeStep {
  return { pages: [...step.pages, ...added], breaks: step.breaks };
}

/** Adds the pages to the grid and to every step of the history, at their end (organize §4); they hold no break. */
function withPagesAppended(
  state: OrganizeState,
  added: OrganizePage[],
): OrganizeState {
  if (added.length === 0) {
    return state;
  }
  return {
    ...state,
    pages: [...state.pages, ...added],
    history: {
      ...state.history,
      past: state.history.past.map((step) => withPagesAdded(step, added)),
      future: state.history.future.map((step) => withPagesAdded(step, added)),
    },
  };
}

function addPdfs(state: OrganizeState, items: OrganizePdf[]): OrganizeState {
  const fresh = items.filter(
    (item) => !state.pdfs.some((pdf) => pdf.id === item.id),
  );
  if (fresh.length === 0) {
    return state;
  }
  const entries = fresh.map((item, index) => ({
    ...item,
    colorIndex: (state.addedCount + index) % ORGANIZE_SOURCE_COLORS,
  }));
  return withPagesAppended(
    {
      ...state,
      pdfs: [...state.pdfs, ...entries],
      addedCount: state.addedCount + fresh.length,
    },
    fresh.flatMap(pagesOfPdf),
  );
}

function unlockPdfs(state: OrganizeState, items: OrganizePdf[]): OrganizeState {
  const opened: OrganizePage[] = [];
  const replaced = new Map<number, OrganizePdfEntry>();
  for (const item of items) {
    const old = state.pdfs.find((pdf) => pdf.id === item.id);
    if (old === undefined) {
      continue;
    }
    replaced.set(item.id, { ...item, colorIndex: old.colorIndex });
    // A PDF that had pages in the grid already (an error-free one) gets none twice.
    if (old.error !== null && item.error === null) {
      opened.push(...pagesOfPdf(item));
    }
  }
  if (replaced.size === 0) {
    return state;
  }
  return withPagesAppended(
    {
      ...state,
      pdfs: state.pdfs.map((pdf) => replaced.get(pdf.id) ?? pdf),
    },
    opened,
  );
}

function selectOnly(state: OrganizeState, key: PageKey): OrganizeState {
  if (!state.pages.some((page) => keyOfPage(page) === key)) {
    return state;
  }
  return { ...state, selected: new Set([key]), anchor: key, focused: key };
}

function toggle(state: OrganizeState, key: PageKey): OrganizeState {
  if (!state.pages.some((page) => keyOfPage(page) === key)) {
    return state;
  }
  const selected = new Set(state.selected);
  if (!selected.delete(key)) {
    selected.add(key);
  }
  return { ...state, selected, anchor: key, focused: key };
}

/** The keys from `from` to `to` in the grid, both ends included, whichever comes first. */
function keysBetween(
  pages: readonly OrganizePage[],
  from: PageKey,
  to: PageKey,
): PageKey[] {
  const index = indexOfKeys(pages);
  const a = index.get(from);
  const b = index.get(to);
  if (a === undefined || b === undefined) {
    return [];
  }
  return pages.slice(Math.min(a, b), Math.max(a, b) + 1).map(keyOfPage);
}

function extend(state: OrganizeState, key: PageKey): OrganizeState {
  if (!state.pages.some((page) => keyOfPage(page) === key)) {
    return state;
  }
  const anchor = state.anchor ?? key;
  return {
    ...state,
    selected: new Set(keysBetween(state.pages, anchor, key)),
    anchor,
    focused: key,
  };
}

function focus(
  state: OrganizeState,
  key: PageKey,
  extendSelection: boolean,
): OrganizeState {
  if (!state.pages.some((page) => keyOfPage(page) === key)) {
    return state;
  }
  if (!extendSelection) {
    return state.focused === key ? state : { ...state, focused: key };
  }
  // Without an anchor the selection starts where the focus was.
  const anchor = state.anchor ?? state.focused ?? key;
  return {
    ...state,
    selected: new Set(keysBetween(state.pages, anchor, key)),
    anchor,
    focused: key,
  };
}

function movePages(
  state: OrganizeState,
  keys: readonly PageKey[],
  insertIndex: number,
): OrganizeState {
  const moving = keysInGrid(state.pages, keys);
  const moved = moveSelected(state.pages, moving, insertIndex);
  if (!moveChanges(state.pages, moved)) {
    return state;
  }
  return withStep(state, stepOf(state), {
    pages: moved,
    breaks: breaksBelowFirst(
      moved,
      breaksHandedOver(state.pages, state.breaks, moving),
    ),
  });
}

function rotatePages(
  state: OrganizeState,
  keys: readonly PageKey[],
  direction: "left" | "right",
): OrganizeState {
  const wanted = keysInGrid(state.pages, keys);
  if (wanted.size === 0) {
    return state;
  }
  const degrees =
    direction === "right" ? ORGANIZE_QUARTER_TURN : -ORGANIZE_QUARTER_TURN;
  const pages = state.pages.map((page) =>
    wanted.has(keyOfPage(page))
      ? { ...page, rotation: turnRotation(page.rotation, degrees) }
      : page,
  );
  return withStep(state, stepOf(state), { pages, breaks: state.breaks });
}

function deletePages(
  state: OrganizeState,
  keys: readonly PageKey[],
): OrganizeState {
  const doomed = keysInGrid(state.pages, keys);
  if (doomed.size === 0) {
    return state;
  }
  const indices = state.pages.flatMap((page, index) =>
    doomed.has(keyOfPage(page)) ? [index] : [],
  );
  const first = indices[0];
  const last = indices[indices.length - 1];
  const pages = state.pages.filter((page) => !doomed.has(keyOfPage(page)));
  // The page after the deleted ones, else the one before them.
  const next = state.pages
    .slice(last + 1)
    .find((page) => !doomed.has(keyOfPage(page)));
  const previous = state.pages
    .slice(0, first)
    .reverse()
    .find((page) => !doomed.has(keyOfPage(page)));
  const landing = next ?? previous;
  return {
    ...withStep(state, stepOf(state), {
      pages,
      breaks: breaksBelowFirst(
        pages,
        breaksHandedOver(state.pages, state.breaks, doomed),
      ),
    }),
    selected: NO_KEYS,
    anchor: null,
    focused: landing === undefined ? null : keyOfPage(landing),
  };
}

/**
 * The place of the latest step that differs from `step` in its pages or its
 * breaks, or -1. Removing a PDF can leave steps the same as their neighbours,
 * and a key press that shows no change would read as a key that does not work.
 */
function latestDifferent(
  steps: readonly OrganizeStep[],
  step: OrganizeStep,
): number {
  for (let index = steps.length - 1; index >= 0; index -= 1) {
    if (!sameStep(steps[index], step)) {
      return index;
    }
  }
  return -1;
}

/**
 * Makes `breaks` the breaks of the grid as one step of the history, or
 * changes nothing when they are the ones there are already.
 */
function withBreaks(
  state: OrganizeState,
  breaks: ReadonlySet<PageKey>,
): OrganizeState {
  return sameBreaks(breaks, state.breaks)
    ? state
    : withStep(state, stepOf(state), { pages: state.pages, breaks });
}

function toggleBreak(state: OrganizeState, key: PageKey): OrganizeState {
  const index = state.pages.findIndex((page) => keyOfPage(page) === key);
  if (index <= 0) {
    return state;
  }
  const breaks = new Set(state.breaks);
  if (!breaks.delete(key)) {
    breaks.add(key);
  }
  return withBreaks(state, breaks);
}

/** A break before each page that `startsNew` says begins an output, the first page aside. */
function breaksWhere(
  state: OrganizeState,
  startsNew: (page: OrganizePage, previous: OrganizePage) => boolean,
): OrganizeState {
  const breaks = new Set<PageKey>();
  state.pages.forEach((page, index) => {
    if (index > 0 && startsNew(page, state.pages[index - 1])) {
      breaks.add(keyOfPage(page));
    }
  });
  return withBreaks(state, breaks);
}

function undo(state: OrganizeState): OrganizeState {
  const { past, future, trimmed } = state.history;
  const at = latestDifferent(past, stepOf(state));
  if (at < 0) {
    return state;
  }
  const rest = past.slice(0, at);
  return tidied({
    ...state,
    pages: past[at].pages,
    breaks: past[at].breaks,
    edited: rest.length > 0 || trimmed,
    history: { past: rest, future: [...future, stepOf(state)], trimmed },
  });
}

function redo(state: OrganizeState): OrganizeState {
  const { past, future, trimmed } = state.history;
  const at = latestDifferent(future, stepOf(state));
  if (at < 0) {
    return state;
  }
  return tidied({
    ...state,
    pages: future[at].pages,
    breaks: future[at].breaks,
    edited: true,
    history: {
      past: [...past, stepOf(state)],
      future: future.slice(0, at),
      trimmed,
    },
  });
}

export function organizeReducer(
  state: OrganizeState,
  action: OrganizeAction,
): OrganizeState {
  switch (action.type) {
    case "ORGANIZE_PDFS_ADDED":
      return addPdfs(state, action.items);
    case "ORGANIZE_PDF_REMOVED":
    case "ORGANIZE_PDF_REPLACED":
      return removePdf(state, action.id);
    case "ORGANIZE_PDFS_UNLOCKED":
      return unlockPdfs(state, action.items);
    case "ORGANIZE_CLEARED":
      if (state.pdfs.length === 0 && state.notice === null) {
        return state;
      }
      return {
        ...createInitialOrganizeState(),
        encrypt: state.encrypt,
        output: state.output,
        outputDir: state.outputDir,
      };
    case "ORGANIZE_NOTICE":
      return { ...state, notice: action.notice };
    case "ORGANIZE_SELECTED_ONLY":
      return selectOnly(state, action.key);
    case "ORGANIZE_SELECTION_TOGGLED":
      return toggle(state, action.key);
    case "ORGANIZE_SELECTION_EXTENDED":
      return extend(state, action.key);
    case "ORGANIZE_SELECTED_ALL":
      if (state.pages.length === 0) {
        return state;
      }
      return {
        ...state,
        selected: new Set(state.pages.map(keyOfPage)),
        anchor: state.anchor ?? keyOfPage(state.pages[0]),
      };
    case "ORGANIZE_SELECTION_CLEARED":
      return state.selected.size === 0 && state.anchor === null
        ? state
        : { ...state, selected: NO_KEYS, anchor: null };
    case "ORGANIZE_FOCUSED":
      return focus(state, action.key, action.extend === true);
    case "ORGANIZE_PAGES_MOVED":
      return movePages(state, action.keys, action.insertIndex);
    case "ORGANIZE_PAGES_ROTATED":
      return rotatePages(state, action.keys, action.direction);
    case "ORGANIZE_PAGES_DELETED":
      return deletePages(state, action.keys);
    case "ORGANIZE_BREAK_TOGGLED":
      return toggleBreak(state, action.key);
    case "ORGANIZE_BREAKS_EACH_PAGE":
      return breaksWhere(state, () => true);
    case "ORGANIZE_BREAKS_EACH_PDF":
      return breaksWhere(state, (page, previous) => page.id !== previous.id);
    case "ORGANIZE_BREAKS_CLEARED":
      return withBreaks(state, NO_KEYS);
    case "ORGANIZE_UNDO":
      return undo(state);
    case "ORGANIZE_REDO":
      return redo(state);
    case "SET_ORGANIZE_ENCRYPT":
      return state.encrypt === action.encrypt
        ? state
        : { ...state, encrypt: action.encrypt };
    case "SET_ORGANIZE_OUTPUT":
      return state.output === action.output
        ? state
        : { ...state, output: action.output };
    case "SET_ORGANIZE_OUTPUT_DIR":
      return { ...state, outputDir: action.outputDir };
  }
}
