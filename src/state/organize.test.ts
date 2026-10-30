import { describe, expect, it } from "vitest";
import type { IpcError, OrganizePdf } from "../ipc";
import {
  ORGANIZE_HISTORY_LIMIT,
  ORGANIZE_SOURCE_COLORS,
  createInitialOrganizeState,
  keyOfPage,
  moveSelected,
  organizeReducer,
  pageKey,
  turnRotation,
  type OrganizeAction,
  type OrganizePage,
  type OrganizeState,
  type PageKey,
} from "./organize";

const A4 = { widthPt: 595, heightPt: 842 };
const PASSWORD: IpcError = { code: "PasswordProtected", detail: null };
const BROKEN: IpcError = { code: "PdfOpenFailed", detail: null };

function pdf(
  id: number,
  pageCount: number,
  overrides: Partial<OrganizePdf> = {},
) {
  return {
    id,
    name: `file-${id}.pdf`,
    pageCount,
    pages: Array.from({ length: pageCount }, () => A4),
    fileBytes: 1000 * pageCount,
    encrypted: false,
    error: null,
    ...overrides,
  } satisfies OrganizePdf;
}

function waiting(id: number): OrganizePdf {
  return pdf(id, 0, { error: PASSWORD, encrypted: true });
}

function reduce(
  state: OrganizeState,
  ...actions: OrganizeAction[]
): OrganizeState {
  return actions.reduce(organizeReducer, state);
}

/** A state with the PDFs added in order. */
function stateWith(...pdfs: OrganizePdf[]): OrganizeState {
  return reduce(createInitialOrganizeState(), {
    type: "ORGANIZE_PDFS_ADDED",
    items: pdfs,
  });
}

function k(id: number, page: number): PageKey {
  return pageKey(id, page);
}

/** The grid as `id:page` (and `@90` for a turn), for readable expectations. */
function grid(state: OrganizeState): string[] {
  return state.pages.map(
    (page) =>
      `${keyOfPage(page)}${page.rotation === 0 ? "" : `@${page.rotation}`}`,
  );
}

const UNDO: OrganizeAction = { type: "ORGANIZE_UNDO" };
const REDO: OrganizeAction = { type: "ORGANIZE_REDO" };

function select(...keys: PageKey[]): OrganizeAction[] {
  return keys.map((key, index) =>
    index === 0
      ? { type: "ORGANIZE_SELECTED_ONLY", key }
      : { type: "ORGANIZE_SELECTION_TOGGLED", key },
  );
}

describe("adding PDFs", () => {
  it("puts their pages in the grid in the order they were added", () => {
    const state = stateWith(pdf(1, 2), pdf(2, 3));

    expect(grid(state)).toEqual(["1:1", "1:2", "2:1", "2:2", "2:3"]);
    expect(state.pages.every((page) => page.rotation === 0)).toBe(true);
  });

  it("adds the pages of a later addition at the end, after the page operations", () => {
    let state = stateWith(pdf(1, 3));
    state = reduce(
      state,
      ...select(k(1, 1)),
      { type: "ORGANIZE_PAGES_ROTATED", keys: [k(1, 1)], direction: "right" },
      { type: "ORGANIZE_PDFS_ADDED", items: [pdf(2, 1)] },
    );

    expect(grid(state)).toEqual(["1:1@90", "1:2", "1:3", "2:1"]);
  });

  it("gives a PDF with an error no pages, and keeps it in the list", () => {
    const state = stateWith(
      pdf(1, 2),
      waiting(2),
      pdf(3, 0, { error: BROKEN }),
    );

    expect(grid(state)).toEqual(["1:1", "1:2"]);
    expect(state.pdfs.map((entry) => entry.id)).toEqual([1, 2, 3]);
  });

  it("numbers the colors in the order of adding and repeats them", () => {
    const items = Array.from({ length: ORGANIZE_SOURCE_COLORS + 2 }, (_, i) =>
      pdf(i + 1, 1),
    );
    const state = stateWith(...items);

    expect(state.pdfs.map((entry) => entry.colorIndex)).toEqual([
      0, 1, 2, 3, 4, 5, 0, 1,
    ]);
  });

  it("does not change the color of a PDF that stays when another is removed", () => {
    let state = stateWith(pdf(1, 1), pdf(2, 1), pdf(3, 1));
    state = reduce(
      state,
      { type: "ORGANIZE_PDF_REMOVED", id: 1 },
      { type: "ORGANIZE_PDFS_ADDED", items: [pdf(4, 1)] },
    );

    expect(state.pdfs.map((entry) => [entry.id, entry.colorIndex])).toEqual([
      [2, 1],
      [3, 2],
      [4, 3],
    ]);
  });

  it("ignores a PDF that is in the list already", () => {
    const state = stateWith(pdf(1, 2));

    expect(
      reduce(state, { type: "ORGANIZE_PDFS_ADDED", items: [pdf(1, 2)] }),
    ).toBe(state);
  });

  it("adds the pages to every step of the history, so undoing keeps them", () => {
    let state = stateWith(pdf(1, 3));
    state = reduce(state, ...select(k(1, 2)), {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [k(1, 2)],
    });
    state = reduce(state, { type: "ORGANIZE_PDFS_ADDED", items: [pdf(2, 2)] });
    expect(grid(state)).toEqual(["1:1", "1:3", "2:1", "2:2"]);

    state = reduce(state, UNDO);
    expect(grid(state)).toEqual(["1:1", "1:2", "1:3", "2:1", "2:2"]);

    state = reduce(state, REDO);
    expect(grid(state)).toEqual(["1:1", "1:3", "2:1", "2:2"]);
  });

  it("adds the pages to the future as well", () => {
    let state = stateWith(pdf(1, 2));
    state = reduce(
      state,
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 1)] },
      UNDO,
      { type: "ORGANIZE_PDFS_ADDED", items: [pdf(2, 1)] },
    );
    expect(grid(state)).toEqual(["1:1", "1:2", "2:1"]);

    state = reduce(state, REDO);

    expect(grid(state)).toEqual(["1:2", "2:1"]);
  });
});

describe("removing a PDF", () => {
  it("takes its pages out of the grid, the selection, the anchor and the focus", () => {
    let state = stateWith(pdf(1, 2), pdf(2, 2));
    state = reduce(state, ...select(k(1, 1), k(2, 1)), {
      type: "ORGANIZE_FOCUSED",
      key: k(1, 2),
    });
    state = reduce(state, { type: "ORGANIZE_SELECTION_TOGGLED", key: k(1, 2) });

    state = reduce(state, { type: "ORGANIZE_PDF_REMOVED", id: 1 });

    expect(grid(state)).toEqual(["2:1", "2:2"]);
    expect([...state.selected]).toEqual([k(2, 1)]);
    expect(state.anchor).toBeNull();
    expect(state.focused).toBeNull();
    expect(state.pdfs.map((entry) => entry.id)).toEqual([2]);
  });

  it("takes its pages out of every step of the past and the future", () => {
    let state = stateWith(pdf(1, 2), pdf(2, 2));
    state = reduce(
      state,
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(2, 1)] },
      { type: "ORGANIZE_PAGES_ROTATED", keys: [k(1, 1)], direction: "right" },
      UNDO,
      UNDO,
    );
    expect(grid(state)).toEqual(["1:1", "1:2", "2:1", "2:2"]);

    state = reduce(state, { type: "ORGANIZE_PDF_REMOVED", id: 1 });
    expect(grid(state)).toEqual(["2:1", "2:2"]);

    state = reduce(state, REDO);
    expect(grid(state)).toEqual(["2:2"]);
    state = reduce(state, REDO);
    expect(grid(state)).toEqual(["2:2"]);
    state = reduce(state, UNDO, UNDO);
    expect(grid(state)).toEqual(["2:1", "2:2"]);
  });

  it("takes its pages out of the steps that can still be undone", () => {
    let state = stateWith(pdf(1, 2), pdf(2, 2));
    state = reduce(
      state,
      { type: "ORGANIZE_PAGES_ROTATED", keys: [k(1, 1)], direction: "right" },
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(2, 1)] },
      { type: "ORGANIZE_PDF_REMOVED", id: 1 },
    );
    expect(grid(state)).toEqual(["2:2"]);

    state = reduce(state, UNDO);
    expect(grid(state)).toEqual(["2:1", "2:2"]);
    state = reduce(state, UNDO);
    expect(grid(state)).toEqual(["2:1", "2:2"]);
    expect(state.history.past.flat().some((page) => page.id === 1)).toBe(false);
  });

  it("leaves no step that changes nothing to undo or redo", () => {
    let state = stateWith(pdf(1, 2), pdf(2, 2));
    state = reduce(
      state,
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(2, 1)] },
      { type: "ORGANIZE_PAGES_ROTATED", keys: [k(1, 1)], direction: "right" },
      { type: "ORGANIZE_PDF_REMOVED", id: 1 },
    );

    state = reduce(state, UNDO);
    expect(grid(state)).toEqual(["2:1", "2:2"]);
    expect(state.history.past).toEqual([]);

    state = reduce(state, REDO);
    expect(grid(state)).toEqual(["2:2"]);
  });

  it("starts over when the last PDF goes, keeping the settings and the notice", () => {
    let state = reduce(
      stateWith(pdf(1, 2)),
      { type: "SET_ORGANIZE_ENCRYPT", encrypt: true },
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 1)] },
      { type: "ORGANIZE_PDF_REMOVED", id: 1 },
    );

    expect(state.edited).toBe(false);
    expect(state.addedCount).toBe(0);
    expect(state.history.past).toEqual([]);
    expect(state.encrypt).toBe(true);

    state = reduce(state, { type: "ORGANIZE_PDFS_ADDED", items: [pdf(2, 1)] });
    expect(state.edited).toBe(false);
  });

  it("ignores an ID that is not in the list", () => {
    const state = stateWith(pdf(1, 2));

    expect(reduce(state, { type: "ORGANIZE_PDF_REMOVED", id: 9 })).toBe(state);
  });

  it("is the same for a PDF a save replaced", () => {
    const state = stateWith(pdf(1, 2), pdf(2, 1));

    expect(
      grid(reduce(state, { type: "ORGANIZE_PDF_REPLACED", id: 2 })),
    ).toEqual(["1:1", "1:2"]);
  });
});

describe("opening PDFs with a password", () => {
  it("replaces every PDF the answer lists and adds the pages of those that opened", () => {
    let state = stateWith(pdf(1, 1), waiting(2), waiting(3), waiting(4));
    state = reduce(state, {
      type: "ORGANIZE_PDFS_UNLOCKED",
      items: [
        pdf(3, 2, { encrypted: true }),
        pdf(2, 1, { encrypted: true }),
        waiting(4),
      ],
    });

    // The target comes first in the answer and its pages first in the grid.
    expect(grid(state)).toEqual(["1:1", "3:1", "3:2", "2:1"]);
    expect(state.pdfs.map((entry) => [entry.id, entry.pageCount])).toEqual([
      [1, 1],
      [2, 1],
      [3, 2],
      [4, 0],
    ]);
    expect(state.pdfs.find((entry) => entry.id === 3)?.error).toBeNull();
    expect(state.pdfs.find((entry) => entry.id === 4)?.error).toEqual(PASSWORD);
  });

  it("keeps the color of the PDF", () => {
    let state = stateWith(pdf(1, 1), waiting(2));
    state = reduce(state, {
      type: "ORGANIZE_PDFS_UNLOCKED",
      items: [pdf(2, 1, { encrypted: true })],
    });

    expect(state.pdfs.find((entry) => entry.id === 2)?.colorIndex).toBe(1);
  });

  it("adds the pages to the history, as an addition does", () => {
    let state = stateWith(pdf(1, 2), waiting(2));
    state = reduce(
      state,
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 1)] },
      {
        type: "ORGANIZE_PDFS_UNLOCKED",
        items: [pdf(2, 1, { encrypted: true })],
      },
      UNDO,
    );

    expect(grid(state)).toEqual(["1:1", "1:2", "2:1"]);
  });

  it("does not add the pages of a PDF that had them already", () => {
    const state = stateWith(pdf(1, 2));

    const next = reduce(state, {
      type: "ORGANIZE_PDFS_UNLOCKED",
      items: [pdf(1, 2)],
    });

    expect(grid(next)).toEqual(["1:1", "1:2"]);
  });
});

describe("selecting", () => {
  const base = () => stateWith(pdf(1, 6));

  it("a press selects the page alone and makes it the anchor and the focus", () => {
    const state = reduce(base(), ...select(k(1, 2), k(1, 3)), {
      type: "ORGANIZE_SELECTED_ONLY",
      key: k(1, 5),
    });

    expect([...state.selected]).toEqual([k(1, 5)]);
    expect(state.anchor).toBe(k(1, 5));
    expect(state.focused).toBe(k(1, 5));
  });

  it("Ctrl puts a page in and out and makes it the anchor", () => {
    let state = reduce(base(), ...select(k(1, 2), k(1, 4)));
    expect([...state.selected]).toEqual([k(1, 2), k(1, 4)]);
    expect(state.anchor).toBe(k(1, 4));

    state = reduce(state, { type: "ORGANIZE_SELECTION_TOGGLED", key: k(1, 2) });

    expect([...state.selected]).toEqual([k(1, 4)]);
    expect(state.anchor).toBe(k(1, 2));
  });

  it("Shift selects from the anchor to the page and nothing else, either way round", () => {
    let state = reduce(
      base(),
      ...select(k(1, 2)),
      { type: "ORGANIZE_SELECTION_TOGGLED", key: k(1, 6) },
      { type: "ORGANIZE_SELECTED_ONLY", key: k(1, 3) },
      { type: "ORGANIZE_SELECTION_EXTENDED", key: k(1, 5) },
    );
    expect([...state.selected]).toEqual([k(1, 3), k(1, 4), k(1, 5)]);
    expect(state.anchor).toBe(k(1, 3));

    state = reduce(state, {
      type: "ORGANIZE_SELECTION_EXTENDED",
      key: k(1, 1),
    });

    expect([...state.selected].sort()).toEqual([k(1, 1), k(1, 2), k(1, 3)]);
    expect(state.anchor).toBe(k(1, 3));
  });

  it("Shift with no anchor selects the page alone", () => {
    const state = reduce(base(), {
      type: "ORGANIZE_SELECTION_EXTENDED",
      key: k(1, 4),
    });

    expect([...state.selected]).toEqual([k(1, 4)]);
    expect(state.anchor).toBe(k(1, 4));
  });

  it("selects all, and clears the selection and the anchor", () => {
    let state = reduce(base(), ...select(k(1, 2)), {
      type: "ORGANIZE_SELECTED_ALL",
    });
    expect(state.selected.size).toBe(6);

    state = reduce(state, { type: "ORGANIZE_SELECTION_CLEARED" });

    expect(state.selected.size).toBe(0);
    expect(state.anchor).toBeNull();
  });

  it("moving the focus changes no selection, and with Shift selects from the anchor", () => {
    let state = reduce(base(), ...select(k(1, 2)));
    state = reduce(state, { type: "ORGANIZE_FOCUSED", key: k(1, 5) });
    expect([...state.selected]).toEqual([k(1, 2)]);
    expect(state.focused).toBe(k(1, 5));

    state = reduce(state, {
      type: "ORGANIZE_FOCUSED",
      key: k(1, 4),
      extend: true,
    });

    expect([...state.selected]).toEqual([k(1, 2), k(1, 3), k(1, 4)]);
    expect(state.anchor).toBe(k(1, 2));
  });

  it("Shift with an arrow and no anchor starts at the page the focus was on", () => {
    let state = reduce(base(), { type: "ORGANIZE_FOCUSED", key: k(1, 3) });
    state = reduce(state, {
      type: "ORGANIZE_FOCUSED",
      key: k(1, 4),
      extend: true,
    });

    expect([...state.selected]).toEqual([k(1, 3), k(1, 4)]);
  });

  it("ignores a page that is not in the grid", () => {
    const state = base();

    expect(
      reduce(state, { type: "ORGANIZE_SELECTED_ONLY", key: k(9, 9) }),
    ).toBe(state);
    expect(
      reduce(state, { type: "ORGANIZE_SELECTION_TOGGLED", key: k(9, 9) }),
    ).toBe(state);
  });

  it("leaves the history alone", () => {
    const state = reduce(base(), ...select(k(1, 1), k(1, 2)));

    expect(state.history.past).toEqual([]);
    expect(state.edited).toBe(false);
  });
});

describe("moveSelected", () => {
  function pagesOf(...numbers: number[]): OrganizePage[] {
    return numbers.map((page) => ({ id: 1, page, rotation: 0 }));
  }
  function order(pages: OrganizePage[]): number[] {
    return pages.map((page) => page.page);
  }
  function keysOf(...numbers: number[]): ReadonlySet<PageKey> {
    return new Set(numbers.map((page) => pageKey(1, page)));
  }

  it("puts the selected pages together, in their order, at the position", () => {
    const pages = pagesOf(1, 2, 3, 4, 5, 6);

    expect(order(moveSelected(pages, keysOf(2, 5), 0))).toEqual([
      2, 5, 1, 3, 4, 6,
    ]);
    expect(order(moveSelected(pages, keysOf(2, 5), 6))).toEqual([
      1, 3, 4, 6, 2, 5,
    ]);
    // Position 3 is in front of page 4; page 3 stays before it, so the block lands after it.
    expect(order(moveSelected(pages, keysOf(2, 5), 3))).toEqual([
      1, 3, 2, 5, 4, 6,
    ]);
  });

  it("counts the position in the grid as it was before the move", () => {
    const pages = pagesOf(1, 2, 3, 4, 5, 6);

    // The block 1 and 2 goes in front of page 5, which is position 4 before the move.
    expect(order(moveSelected(pages, keysOf(1, 2), 4))).toEqual([
      3, 4, 1, 2, 5, 6,
    ]);
    // Position 3 is in front of page 4, whichever of the selected pages lie before it.
    expect(order(moveSelected(pages, keysOf(2, 3), 3))).toEqual([
      1, 2, 3, 4, 5, 6,
    ]);
    expect(order(moveSelected(pages, keysOf(1, 3, 5), 3))).toEqual([
      2, 1, 3, 5, 4, 6,
    ]);
  });

  it("clamps a position past the ends", () => {
    const pages = pagesOf(1, 2, 3);

    expect(order(moveSelected(pages, keysOf(2), -5))).toEqual([2, 1, 3]);
    expect(order(moveSelected(pages, keysOf(2), 99))).toEqual([1, 3, 2]);
  });

  it("returns the grid as it is when nothing moves", () => {
    const pages = pagesOf(1, 2, 3, 4);

    expect(moveSelected(pages, keysOf(2, 3), 1)).toEqual(pages);
    expect(moveSelected(pages, keysOf(2, 3), 2)).toEqual(pages);
    expect(moveSelected(pages, keysOf(2, 3), 4)).not.toEqual(pages);
    expect(moveSelected(pages, new Set(), 2)).toEqual(pages);
    expect(moveSelected(pages, keysOf(1, 2, 3, 4), 2)).toEqual(pages);
  });

  it("keeps the page objects, so a move changes nothing else", () => {
    const pages = pagesOf(1, 2, 3);
    const moved = moveSelected(pages, keysOf(3), 0);

    expect(moved[0]).toBe(pages[2]);
  });
});

describe("page operations and the history", () => {
  it("a move, a turn and a deletion each take one step", () => {
    let state = stateWith(pdf(1, 4));
    state = reduce(state, {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 4)],
      insertIndex: 0,
    });
    expect(grid(state)).toEqual(["1:4", "1:1", "1:2", "1:3"]);
    state = reduce(state, {
      type: "ORGANIZE_PAGES_ROTATED",
      keys: [k(1, 4)],
      direction: "left",
    });
    expect(grid(state)).toEqual(["1:4@270", "1:1", "1:2", "1:3"]);
    state = reduce(state, { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 2)] });
    expect(grid(state)).toEqual(["1:4@270", "1:1", "1:3"]);
    expect(state.history.past).toHaveLength(3);
    expect(state.edited).toBe(true);

    state = reduce(state, UNDO);
    expect(grid(state)).toEqual(["1:4@270", "1:1", "1:2", "1:3"]);
    state = reduce(state, UNDO);
    expect(grid(state)).toEqual(["1:4", "1:1", "1:2", "1:3"]);
    state = reduce(state, UNDO);
    expect(grid(state)).toEqual(["1:1", "1:2", "1:3", "1:4"]);
    state = reduce(state, REDO, REDO, REDO);
    expect(grid(state)).toEqual(["1:4@270", "1:1", "1:3"]);
  });

  it("an operation that changes nothing is no step", () => {
    const state = stateWith(pdf(1, 3));

    const moved = reduce(state, {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 1)],
      insertIndex: 1,
    });
    const rotatedNothing = reduce(state, {
      type: "ORGANIZE_PAGES_ROTATED",
      keys: [k(9, 9)],
      direction: "right",
    });
    const deletedNothing = reduce(state, {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [],
    });

    for (const next of [moved, rotatedNothing, deletedNothing]) {
      expect(next.history.past).toEqual([]);
      expect(next.edited).toBe(false);
    }
  });

  it("an operation clears what could be redone", () => {
    let state = stateWith(pdf(1, 3));
    state = reduce(
      state,
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 1)] },
      UNDO,
    );
    expect(state.history.future).toHaveLength(1);

    state = reduce(state, {
      type: "ORGANIZE_PAGES_ROTATED",
      keys: [k(1, 2)],
      direction: "right",
    });

    expect(state.history.future).toEqual([]);
    expect(reduce(state, REDO)).toBe(state);
  });

  it("keeps ORGANIZE_HISTORY_LIMIT steps and drops the oldest", () => {
    let state = stateWith(pdf(1, 1));
    for (let turn = 0; turn < ORGANIZE_HISTORY_LIMIT + 5; turn += 1) {
      state = reduce(state, {
        type: "ORGANIZE_PAGES_ROTATED",
        keys: [k(1, 1)],
        direction: "right",
      });
    }
    expect(state.history.past).toHaveLength(ORGANIZE_HISTORY_LIMIT);

    for (let step = 0; step < ORGANIZE_HISTORY_LIMIT + 5; step += 1) {
      state = reduce(state, UNDO);
    }

    // 105 turns are 105 × 90°; the earliest 5 cannot be undone: 5 × 90° remain.
    expect(state.pages[0].rotation).toBe(turnRotation(5 * 90, 0));
    expect(state.history.past).toEqual([]);
    expect(state.edited).toBe(true);
  });

  it("is not edited again once everything is undone, if nothing was dropped", () => {
    let state = stateWith(pdf(1, 2));
    state = reduce(state, { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 1)] });
    expect(state.edited).toBe(true);

    state = reduce(state, UNDO);
    expect(state.edited).toBe(false);

    state = reduce(state, REDO);
    expect(state.edited).toBe(true);
  });

  it("undoing with nothing to undo changes nothing", () => {
    const state = stateWith(pdf(1, 2));

    expect(reduce(state, UNDO)).toBe(state);
    expect(reduce(state, REDO)).toBe(state);
  });

  it("a move leaves the selection and the focus on the same pages", () => {
    let state = stateWith(pdf(1, 5));
    state = reduce(state, ...select(k(1, 2), k(1, 3)));

    state = reduce(state, {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 2), k(1, 3)],
      insertIndex: 5,
    });

    expect(grid(state)).toEqual(["1:1", "1:4", "1:5", "1:2", "1:3"]);
    expect([...state.selected]).toEqual([k(1, 2), k(1, 3)]);
    expect(state.focused).toBe(k(1, 3));
  });

  it("a deletion empties the selection and moves the focus to the next page", () => {
    let state = stateWith(pdf(1, 5));
    state = reduce(state, ...select(k(1, 2), k(1, 3)));

    state = reduce(state, {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [k(1, 2), k(1, 3)],
    });

    expect(grid(state)).toEqual(["1:1", "1:4", "1:5"]);
    expect(state.selected.size).toBe(0);
    expect(state.anchor).toBeNull();
    expect(state.focused).toBe(k(1, 4));
  });

  it("a deletion at the end moves the focus to the page before", () => {
    let state = stateWith(pdf(1, 4));

    state = reduce(state, {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [k(1, 3), k(1, 4)],
    });

    expect(state.focused).toBe(k(1, 2));
  });

  it("deleting every page leaves nothing to focus", () => {
    let state = stateWith(pdf(1, 2));

    state = reduce(state, {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [k(1, 1), k(1, 2)],
    });

    expect(state.pages).toEqual([]);
    expect(state.focused).toBeNull();
  });

  it("undoing takes the pages that are gone from the selection and gives the focus a page", () => {
    let state = stateWith(pdf(1, 3), pdf(2, 1));
    state = reduce(state, { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 3)] });
    state = reduce(state, { type: "ORGANIZE_PDFS_ADDED", items: [pdf(3, 1)] });
    state = reduce(state, ...select(k(3, 1)));
    // Removing the PDF drops its pages everywhere, including the selection.
    state = reduce(state, { type: "ORGANIZE_PDF_REMOVED", id: 3 });

    state = reduce(state, UNDO);

    expect([...state.selected]).toEqual([]);
    expect(state.focused).toBe(k(1, 1));
  });

  it("a turn is kept within 0 to 270", () => {
    expect(turnRotation(0, 90)).toBe(90);
    expect(turnRotation(270, 90)).toBe(0);
    expect(turnRotation(0, -90)).toBe(270);
    expect(turnRotation(90, -90)).toBe(0);
    expect(turnRotation(180, 270)).toBe(90);
  });

  it("turns left and right opposite ways, in a step each", () => {
    let state = stateWith(pdf(1, 2));
    state = reduce(
      state,
      { type: "ORGANIZE_PAGES_ROTATED", keys: [k(1, 1)], direction: "left" },
      { type: "ORGANIZE_PAGES_ROTATED", keys: [k(1, 2)], direction: "right" },
    );

    expect(grid(state)).toEqual(["1:1@270", "1:2@90"]);
    expect(state.history.past).toHaveLength(2);
  });
});

describe("clearing and the settings", () => {
  it("empties the list, the grid and the history but not the settings", () => {
    let state = reduce(
      createInitialOrganizeState({
        output: "split",
        encrypt: true,
        outputDir: null,
      }),
      { type: "ORGANIZE_PDFS_ADDED", items: [pdf(1, 2)] },
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 1)] },
    );

    state = reduce(state, { type: "ORGANIZE_CLEARED" });

    expect(state.pdfs).toEqual([]);
    expect(state.pages).toEqual([]);
    expect(state.history).toEqual({ past: [], future: [], trimmed: false });
    expect(state.edited).toBe(false);
    expect(state.encrypt).toBe(true);
    expect(state.output).toBe("split");
  });

  it("numbers the colors from the first again after it", () => {
    const state = reduce(
      stateWith(pdf(1, 1), pdf(2, 1)),
      { type: "ORGANIZE_CLEARED" },
      { type: "ORGANIZE_PDFS_ADDED", items: [pdf(3, 1)] },
    );

    expect(state.pdfs[0].colorIndex).toBe(0);
  });

  it("starts from the saved settings and keeps the output it was given", () => {
    const state = createInitialOrganizeState({
      output: "split",
      encrypt: true,
      outputDir: { dirLabel: "out" },
    });

    expect(state.output).toBe("split");
    expect(state.encrypt).toBe(true);
    expect(createInitialOrganizeState().output).toBe("single");
  });

  it("sets whether the PDF gets a password, as no step", () => {
    const state = reduce(stateWith(pdf(1, 1)), {
      type: "SET_ORGANIZE_ENCRYPT",
      encrypt: true,
    });

    expect(state.encrypt).toBe(true);
    expect(state.history.past).toEqual([]);
  });
});
