import { describe, expect, it } from "vitest";
import type { OrganizePdf } from "../ipc";
import {
  ORGANIZE_HISTORY_LIMIT,
  createInitialOrganizeState,
  keyOfPage,
  organizeReducer,
  pageKey,
  partNumbers,
  splitIntoParts,
  type OrganizeAction,
  type OrganizeState,
  type PageKey,
} from "./organize";
import { expectBreaksInvariant, reduceChecked } from "./organizeTestSupport";

const A4 = { widthPt: 595, heightPt: 842 };

function pdf(id: number, pageCount: number): OrganizePdf {
  return {
    id,
    name: `file-${id}.pdf`,
    pageCount,
    pages: Array.from({ length: pageCount }, () => A4),
    fileBytes: 1000 * pageCount,
    encrypted: false,
    error: null,
  };
}

function k(id: number, page: number): PageKey {
  return pageKey(id, page);
}

const UNDO: OrganizeAction = { type: "ORGANIZE_UNDO" };
const REDO: OrganizeAction = { type: "ORGANIZE_REDO" };

function stateWith(...pdfs: OrganizePdf[]): OrganizeState {
  return reduceChecked(createInitialOrganizeState(), {
    type: "ORGANIZE_PDFS_ADDED",
    items: pdfs,
  });
}

function toggles(...keys: PageKey[]): OrganizeAction[] {
  return keys.map((key) => ({ type: "ORGANIZE_BREAK_TOGGLED", key }));
}

/** The pages that hold a break, in the order of the grid. */
function breaks(state: OrganizeState): PageKey[] {
  return state.pages.map(keyOfPage).filter((key) => state.breaks.has(key));
}

function grid(state: OrganizeState): PageKey[] {
  return state.pages.map(keyOfPage);
}

const MOVE_TO_END = (key: PageKey, count: number): OrganizeAction => ({
  type: "ORGANIZE_PAGES_MOVED",
  keys: [key],
  insertIndex: count,
});

describe("putting a break in and out", () => {
  it("toggles the break before a page, as a step of the history", () => {
    const state = reduceChecked(stateWith(pdf(1, 4)), ...toggles(k(1, 3)));

    expect(breaks(state)).toEqual([k(1, 3)]);
    expect(state.edited).toBe(true);
    expect(state.history.past).toHaveLength(1);

    const off = reduceChecked(state, ...toggles(k(1, 3)));
    expect(breaks(off)).toEqual([]);
    expect(off.history.past).toHaveLength(2);
  });

  it("does nothing for the first page or a page that is not in the grid", () => {
    const state = stateWith(pdf(1, 3));

    expect(reduceChecked(state, ...toggles(k(1, 1)))).toBe(state);
    expect(reduceChecked(state, ...toggles(k(9, 1)))).toBe(state);
  });

  it("is undone and redone with the pages", () => {
    let state = reduceChecked(
      stateWith(pdf(1, 4)),
      ...toggles(k(1, 2), k(1, 4)),
    );

    state = reduceChecked(state, UNDO);
    expect(breaks(state)).toEqual([k(1, 2)]);
    state = reduceChecked(state, UNDO);
    expect(breaks(state)).toEqual([]);
    expect(state.edited).toBe(false);
    state = reduceChecked(state, REDO, REDO);
    expect(breaks(state)).toEqual([k(1, 2), k(1, 4)]);
  });

  it("is one step that differs from its neighbours only by a break", () => {
    // Undo must not skip a step that changes nothing but a break.
    const state = reduceChecked(stateWith(pdf(1, 3)), ...toggles(k(1, 2)));

    expect(reduceChecked(state, UNDO).history.future).toHaveLength(1);
    expect(breaks(reduceChecked(state, UNDO))).toEqual([]);
  });

  it("stops at the limit of the history, as the page operations do", () => {
    let state = stateWith(pdf(1, 3));
    for (let turn = 0; turn < ORGANIZE_HISTORY_LIMIT + 5; turn += 1) {
      state = reduceChecked(state, ...toggles(k(1, 2)));
    }

    expect(state.history.past).toHaveLength(ORGANIZE_HISTORY_LIMIT);
    expect(state.history.trimmed).toBe(true);
  });
});

describe("the shortcuts", () => {
  it("puts a break before every page but the first, in place of the ones set", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 3), pdf(2, 1)),
      ...toggles(k(1, 3)),
      { type: "ORGANIZE_BREAKS_EACH_PAGE" },
    );

    expect(breaks(state)).toEqual([k(1, 2), k(1, 3), k(2, 1)]);
  });

  it("puts a break where a page comes from another PDF than the one before it", () => {
    // 1:1 2:1 1:2 1:3 2:2: the PDF changes before 2:1, 1:2 and 2:2.
    let state = stateWith(pdf(1, 3), pdf(2, 2));
    state = reduceChecked(
      state,
      { type: "ORGANIZE_PAGES_MOVED", keys: [k(2, 1)], insertIndex: 1 },
      { type: "ORGANIZE_PAGES_MOVED", keys: [k(2, 2)], insertIndex: 4 },
      ...toggles(k(1, 3)),
      { type: "ORGANIZE_BREAKS_EACH_PDF" },
    );

    expect(grid(state)).toEqual([k(1, 1), k(2, 1), k(1, 2), k(1, 3), k(2, 2)]);
    expect(breaks(state)).toEqual([k(2, 1), k(1, 2), k(2, 2)]);
  });

  it("takes every break off", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 3)),
      ...toggles(k(1, 2), k(1, 3)),
      { type: "ORGANIZE_BREAKS_CLEARED" },
    );

    expect(breaks(state)).toEqual([]);
    expect(reduceChecked(state, UNDO).breaks.size).toBe(2);
  });

  it("changes nothing, and adds no step, when the breaks are those it would put", () => {
    const eachPage = reduceChecked(stateWith(pdf(1, 3)), {
      type: "ORGANIZE_BREAKS_EACH_PAGE",
    });
    expect(reduceChecked(eachPage, { type: "ORGANIZE_BREAKS_EACH_PAGE" })).toBe(
      eachPage,
    );

    const none = stateWith(pdf(1, 3));
    expect(reduceChecked(none, { type: "ORGANIZE_BREAKS_CLEARED" })).toBe(none);
    // One PDF has no place where the PDF changes.
    expect(reduceChecked(none, { type: "ORGANIZE_BREAKS_EACH_PDF" })).toBe(
      none,
    );
    expect(none.history.past).toEqual([]);
  });

  it("does nothing without pages", () => {
    const empty = createInitialOrganizeState();

    expect(reduceChecked(empty, { type: "ORGANIZE_BREAKS_EACH_PAGE" })).toBe(
      empty,
    );
  });
});

describe("a break when pages are moved", () => {
  it("is handed to the page that stays after a moved page, which holds none", () => {
    // 1:1 1:2 |1:3 1:4: moving 1:3 to the front hands its break to 1:4.
    const state = reduceChecked(stateWith(pdf(1, 4)), ...toggles(k(1, 3)), {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 3)],
      insertIndex: 0,
    });

    expect(grid(state)).toEqual([k(1, 3), k(1, 1), k(1, 2), k(1, 4)]);
    expect(breaks(state)).toEqual([k(1, 4)]);
  });

  it("drops the break of a moved page when no page stays after it", () => {
    const state = reduceChecked(stateWith(pdf(1, 3)), ...toggles(k(1, 3)), {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 3)],
      insertIndex: 0,
    });

    expect(breaks(state)).toEqual([]);
  });

  it("does not put a break on the page that is first after the move", () => {
    // 1:1 |1:2 1:3: moving 1:1 behind 1:2 makes 1:2 first, and the first holds none.
    const state = reduceChecked(stateWith(pdf(1, 3)), ...toggles(k(1, 2)), {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 1)],
      insertIndex: 2,
    });

    expect(grid(state)).toEqual([k(1, 2), k(1, 1), k(1, 3)]);
    expect(breaks(state)).toEqual([]);
  });

  it("moves the pages the selection holds apart, each handing over its break", () => {
    // 1:1 |1:2 1:3 |1:4 1:5: 1:2 and 1:4 move to the end.
    const state = reduceChecked(
      stateWith(pdf(1, 5)),
      ...toggles(k(1, 2), k(1, 4)),
      {
        type: "ORGANIZE_PAGES_MOVED",
        keys: [k(1, 2), k(1, 4)],
        insertIndex: 5,
      },
    );

    expect(grid(state)).toEqual([k(1, 1), k(1, 3), k(1, 5), k(1, 2), k(1, 4)]);
    expect(breaks(state)).toEqual([k(1, 3), k(1, 5)]);
  });

  it("merges the breaks of pages that hold one in a row into the one that stays", () => {
    // 1:1 |1:2 |1:3 1:4: moving both hands one break to 1:4.
    const state = reduceChecked(
      stateWith(pdf(1, 4)),
      ...toggles(k(1, 2), k(1, 3)),
      {
        type: "ORGANIZE_PAGES_MOVED",
        keys: [k(1, 2), k(1, 3)],
        insertIndex: 4,
      },
    );

    expect(grid(state)).toEqual([k(1, 1), k(1, 4), k(1, 2), k(1, 3)]);
    expect(breaks(state)).toEqual([k(1, 4)]);
  });

  it("keeps a break the page that stays holds, and adds none to the moved pages", () => {
    // 1:1 |1:2 |1:3 1:4: 1:2 moves to the end; 1:3 keeps its own.
    const state = reduceChecked(
      stateWith(pdf(1, 4)),
      ...toggles(k(1, 2), k(1, 3)),
      MOVE_TO_END(k(1, 2), 4),
    );

    expect(breaks(state)).toEqual([k(1, 3)]);
    expect(state.breaks.has(k(1, 2))).toBe(false);
  });

  it("leaves the breaks alone when the move changes nothing", () => {
    const before = reduceChecked(stateWith(pdf(1, 3)), ...toggles(k(1, 2)));
    const after = reduceChecked(before, {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 3)],
      insertIndex: 3,
    });

    expect(after).toBe(before);
  });

  it("undoes the move together with the breaks it handed over", () => {
    let state = reduceChecked(stateWith(pdf(1, 4)), ...toggles(k(1, 3)), {
      type: "ORGANIZE_PAGES_MOVED",
      keys: [k(1, 3)],
      insertIndex: 0,
    });

    state = reduceChecked(state, UNDO);
    expect(grid(state)).toEqual([k(1, 1), k(1, 2), k(1, 3), k(1, 4)]);
    expect(breaks(state)).toEqual([k(1, 3)]);
    state = reduceChecked(state, REDO);
    expect(breaks(state)).toEqual([k(1, 4)]);
  });
});

describe("a break when pages are deleted", () => {
  it("is handed to the page after the deleted one", () => {
    const state = reduceChecked(stateWith(pdf(1, 4)), ...toggles(k(1, 2)), {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [k(1, 2)],
    });

    expect(grid(state)).toEqual([k(1, 1), k(1, 3), k(1, 4)]);
    expect(breaks(state)).toEqual([k(1, 3)]);
  });

  it("is dropped when the deleted page was the last", () => {
    const state = reduceChecked(stateWith(pdf(1, 3)), ...toggles(k(1, 3)), {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [k(1, 3)],
    });

    expect(breaks(state)).toEqual([]);
  });

  it("is not put on the page that becomes the first", () => {
    const state = reduceChecked(stateWith(pdf(1, 3)), ...toggles(k(1, 2)), {
      type: "ORGANIZE_PAGES_DELETED",
      keys: [k(1, 1)],
    });

    expect(grid(state)).toEqual([k(1, 2), k(1, 3)]);
    expect(breaks(state)).toEqual([]);
  });

  it("hands the breaks of deleted pages that are apart on, each to its own page", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 6)),
      ...toggles(k(1, 2), k(1, 5)),
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 2), k(1, 5)] },
    );

    expect(grid(state)).toEqual([k(1, 1), k(1, 3), k(1, 4), k(1, 6)]);
    expect(breaks(state)).toEqual([k(1, 3), k(1, 6)]);
  });

  it("is brought back by undo", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 4)),
      ...toggles(k(1, 2)),
      { type: "ORGANIZE_PAGES_DELETED", keys: [k(1, 2)] },
      UNDO,
    );

    expect(breaks(state)).toEqual([k(1, 2)]);
  });
});

describe("rotating, adding and unlocking", () => {
  it("leave the breaks as they are", () => {
    const set = reduceChecked(stateWith(pdf(1, 3)), ...toggles(k(1, 2)));
    const rotated = reduceChecked(set, {
      type: "ORGANIZE_PAGES_ROTATED",
      keys: [k(1, 2)],
      direction: "right",
    });
    expect(breaks(rotated)).toEqual([k(1, 2)]);

    const added = reduceChecked(rotated, {
      type: "ORGANIZE_PDFS_ADDED",
      items: [pdf(2, 2)],
    });
    expect(breaks(added)).toEqual([k(1, 2)]);
    // The added pages hold none in the steps behind it either.
    expect(added.history.past.every((step) => step.breaks.size <= 1)).toBe(
      true,
    );

    const waiting = {
      ...pdf(3, 0),
      error: { code: "PasswordProtected" as const, detail: null },
    };
    const withWaiting = reduceChecked(added, {
      type: "ORGANIZE_PDFS_ADDED",
      items: [waiting],
    });
    const unlocked = reduceChecked(withWaiting, {
      type: "ORGANIZE_PDFS_UNLOCKED",
      items: [pdf(3, 2)],
    });
    expect(breaks(unlocked)).toEqual([k(1, 2)]);
    expect(unlocked.pages).toHaveLength(7);
  });
});

describe("a break when a PDF is taken out", () => {
  it("is handed to the page that follows the PDF's last page, in the grid and in each step", () => {
    // 1:1 1:2 |2:1 2:2 3:1 with a break before 3:1, then PDF 2 goes.
    let state = stateWith(pdf(1, 2), pdf(2, 2), pdf(3, 1));
    state = reduceChecked(state, ...toggles(k(2, 1), k(3, 1)), {
      type: "ORGANIZE_PAGES_ROTATED",
      keys: [k(1, 1)],
      direction: "left",
    });
    const before = state;

    state = reduceChecked(state, { type: "ORGANIZE_PDF_REMOVED", id: 2 });

    expect(grid(state)).toEqual([k(1, 1), k(1, 2), k(3, 1)]);
    expect(breaks(state)).toEqual([k(3, 1)]);
    // The steps behind it are told the same, so undo does not bring breaks of a PDF that is gone.
    for (const step of [...state.history.past, ...state.history.future]) {
      expect([...step.breaks].every((key) => !key.startsWith("2:"))).toBe(true);
    }
    expect(before.history.past.length).toBe(state.history.past.length);
    const undone = reduceChecked(state, UNDO);
    expect(breaks(undone)).toEqual([k(3, 1)]);
  });

  it("is dropped when the PDF's pages were the last, and not put on a first page", () => {
    let state = stateWith(pdf(1, 2), pdf(2, 2));
    state = reduceChecked(state, ...toggles(k(2, 1)), {
      type: "ORGANIZE_PDF_REMOVED",
      id: 2,
    });
    expect(breaks(state)).toEqual([]);

    // The first PDF goes: the break its successor's first page holds is the first page's now.
    state = stateWith(pdf(1, 2), pdf(2, 2));
    state = reduceChecked(state, ...toggles(k(2, 1)), {
      type: "ORGANIZE_PDF_REMOVED",
      id: 1,
    });
    expect(grid(state)).toEqual([k(2, 1), k(2, 2)]);
    expect(breaks(state)).toEqual([]);
  });

  it("does the same to the steps that would be redone", () => {
    let state = stateWith(pdf(1, 2), pdf(2, 2), pdf(3, 1));
    state = reduceChecked(
      state,
      ...toggles(k(2, 2)),
      { type: "ORGANIZE_BREAKS_CLEARED" },
      UNDO,
    );
    expect(breaks(state)).toEqual([k(2, 2)]);

    state = reduceChecked(state, { type: "ORGANIZE_PDF_REMOVED", id: 2 });
    expect(breaks(state)).toEqual([k(3, 1)]);
    for (const step of state.history.future) {
      expect([...step.breaks].every((key) => !key.startsWith("2:"))).toBe(true);
    }
  });

  it("keeps the breaks, and the settings, when the last PDF goes and another comes", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 2)),
      { type: "SET_ORGANIZE_OUTPUT", output: "split" },
      { type: "SET_ORGANIZE_OUTPUT_DIR", outputDir: { dirLabel: "out" } },
      ...toggles(k(1, 2)),
      { type: "ORGANIZE_PDF_REMOVED", id: 1 },
    );

    expect(state.breaks.size).toBe(0);
    expect(state.output).toBe("split");
    expect(state.outputDir).toEqual({ dirLabel: "out" });
  });
});

describe("undo and redo and the breaks", () => {
  it("do not skip a step that differs only by a break", () => {
    let state = stateWith(pdf(1, 3));
    state = reduceChecked(state, ...toggles(k(1, 2)), ...toggles(k(1, 3)), {
      type: "ORGANIZE_BREAKS_CLEARED",
    });

    state = reduceChecked(state, UNDO);
    expect(breaks(state)).toEqual([k(1, 2), k(1, 3)]);
    state = reduceChecked(state, UNDO);
    expect(breaks(state)).toEqual([k(1, 2)]);
    state = reduceChecked(state, REDO, REDO);
    expect(breaks(state)).toEqual([]);
  });
});

describe("the settings of the save in parts", () => {
  it("keeps the breaks when the way of saving changes", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 3)),
      ...toggles(k(1, 2)),
      { type: "SET_ORGANIZE_OUTPUT", output: "split" },
      { type: "SET_ORGANIZE_OUTPUT", output: "single" },
    );

    expect(state.output).toBe("single");
    expect(breaks(state)).toEqual([k(1, 2)]);
    expect(
      organizeReducer(state, { type: "SET_ORGANIZE_OUTPUT", output: "single" }),
    ).toBe(state);
  });

  it("starts from the saved folder, and clears keep it", () => {
    const initial = createInitialOrganizeState({
      output: "split",
      encrypt: false,
      outputDir: { dirLabel: "saved" },
    });
    expect(initial.output).toBe("split");
    expect(initial.outputDir).toEqual({ dirLabel: "saved" });

    const cleared = reduceChecked(
      reduceChecked(initial, {
        type: "ORGANIZE_PDFS_ADDED",
        items: [pdf(1, 2)],
      }),
      { type: "ORGANIZE_CLEARED" },
    );
    expect(cleared.outputDir).toEqual({ dirLabel: "saved" });
    expect(cleared.breaks.size).toBe(0);
  });
});

describe("splitIntoParts", () => {
  it("makes one output of pages without breaks, and none of no pages", () => {
    const state = stateWith(pdf(1, 3));

    expect(splitIntoParts(state.pages, state.breaks)).toEqual([state.pages]);
    expect(splitIntoParts([], new Set())).toEqual([]);
  });

  it("makes one output of each page for a break before each", () => {
    const state = reduceChecked(stateWith(pdf(1, 3)), {
      type: "ORGANIZE_BREAKS_EACH_PAGE",
    });

    expect(
      splitIntoParts(state.pages, state.breaks).map((part) => part.length),
    ).toEqual([1, 1, 1]);
  });

  it("starts a new output at each break, in the order of the grid and turned as the pages are", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 5)),
      ...toggles(k(1, 3), k(1, 5)),
      { type: "ORGANIZE_PAGES_ROTATED", keys: [k(1, 4)], direction: "right" },
    );

    const parts = splitIntoParts(state.pages, state.breaks);
    expect(parts.map((part) => part.map(keyOfPage))).toEqual([
      [k(1, 1), k(1, 2)],
      [k(1, 3), k(1, 4)],
      [k(1, 5)],
    ]);
    expect(parts[1][1].rotation).toBe(90);
  });

  it("starts an output at the first page even when it is given a break", () => {
    const state = stateWith(pdf(1, 2));

    expect(splitIntoParts(state.pages, new Set([k(1, 1)]))).toHaveLength(1);
  });

  it("numbers the pages that begin an output as the outputs are", () => {
    const state = reduceChecked(
      stateWith(pdf(1, 5)),
      ...toggles(k(1, 3), k(1, 5)),
    );

    expect([...partNumbers(state.pages, state.breaks)]).toEqual([
      [k(1, 1), 1],
      [k(1, 3), 2],
      [k(1, 5), 3],
    ]);
  });
});

describe("the invariant of the breaks", () => {
  it("holds through any run of the actions that change the pages or the breaks", () => {
    // A fixed pseudo-random run, so a failure is the same every time.
    let seed = 12345;
    const next = (limit: number) => {
      seed = (seed * 1103515245 + 12345) % 2147483648;
      return seed % limit;
    };
    let state = stateWith(pdf(1, 6), pdf(2, 4), pdf(3, 5));
    const keysOf = (count: number): PageKey[] =>
      Array.from({ length: count }, () => {
        const page = state.pages[next(Math.max(state.pages.length, 1))];
        return page === undefined ? k(9, 9) : keyOfPage(page);
      });

    for (let step = 0; step < 400; step += 1) {
      const action: OrganizeAction = (() => {
        switch (next(11)) {
          case 0:
            return { type: "ORGANIZE_BREAK_TOGGLED", key: keysOf(1)[0] };
          case 1:
            return { type: "ORGANIZE_BREAKS_EACH_PAGE" };
          case 2:
            return { type: "ORGANIZE_BREAKS_EACH_PDF" };
          case 3:
            return {
              type: "ORGANIZE_PAGES_MOVED",
              keys: keysOf(1 + next(3)),
              insertIndex: next(state.pages.length + 1),
            };
          case 4:
            return {
              type: "ORGANIZE_PAGES_DELETED",
              keys: keysOf(1 + next(3)),
            };
          case 5:
            return UNDO;
          case 6:
            return REDO;
          case 7:
            return { type: "ORGANIZE_PDF_REMOVED", id: 1 + next(3) };
          case 8:
            return {
              type: "ORGANIZE_PDFS_ADDED",
              items: [pdf(1 + next(3), 2 + next(3))],
            };
          case 9:
            return { type: "ORGANIZE_BREAKS_CLEARED" };
          default:
            return {
              type: "ORGANIZE_PAGES_ROTATED",
              keys: keysOf(2),
              direction: "right",
            };
        }
      })();
      state = reduceChecked(state, action);
    }

    expectBreaksInvariant(state);
  });
});
