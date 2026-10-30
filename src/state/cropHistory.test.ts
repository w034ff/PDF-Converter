import { describe, expect, it } from "vitest";
import type { Candidate, CropPdf } from "../ipc";
import { appReducer, createInitialAppState } from "./appReducer";
import {
  CROP_HISTORY_LIMIT,
  canRedo,
  canUndo,
  createInitialCropState,
  cropReducer,
  unexportedAreaCount,
} from "./crop";
import type { AppState, CropAction, CropState, NewArea, RectPt } from "./types";

const A4 = { widthPt: 595, heightPt: 842 };

function pdf(id: number): CropPdf {
  return {
    id,
    name: `file-${id}.pdf`,
    pageCount: 5,
    pages: Array.from({ length: 5 }, () => A4),
    error: null,
  };
}

function reduce(state: CropState, ...actions: CropAction[]): CropState {
  return actions.reduce(cropReducer, state);
}

function open(id = 1): CropState {
  return reduce(createInitialCropState(), {
    type: "CROP_PDF_ACTIVATED",
    pdf: pdf(id),
  });
}

function figure(page: number, rectPt: RectPt = [0, 0, 100, 100]): NewArea {
  return { page, kind: "figure", rectPt, imageIndex: null };
}

function add(state: CropState, ...areas: NewArea[]): CropState {
  return reduce(
    state,
    ...areas.map((area): CropAction => ({ type: "CROP_AREA_ADDED", area })),
  );
}

function keys(state: CropState): number[] {
  return state.areas.map((area) => area.key);
}

function rectOf(state: CropState, key: number): RectPt | undefined {
  return state.areas.find((area) => area.key === key)?.rectPt;
}

const UNDO: CropAction = { type: "CROP_UNDO" };
const REDO: CropAction = { type: "CROP_REDO" };

describe("undoing and redoing", () => {
  it("takes an added area away and brings it back with its key", () => {
    const added = add(open(), figure(1), figure(2, [200, 200, 300, 300]));
    const undone = reduce(added, UNDO);
    expect(keys(undone)).toEqual([1]);
    expect(canRedo(undone)).toBe(true);

    const redone = reduce(undone, REDO);
    expect(keys(redone)).toEqual([1, 2]);
    expect(redone.selectedKey).toBe(2);
    expect(redone.page).toBe(2);
  });

  it("never gives a key twice", () => {
    const state = reduce(
      add(open(), figure(1), figure(1, [200, 0, 300, 100])),
      UNDO,
    );
    const next = add(state, figure(1, [400, 0, 500, 100]));
    expect(keys(next)).toEqual([1, 3]);
  });

  it("brings a removed area back, selects it and shows its page", () => {
    const state = reduce(
      add(open(), figure(1), figure(3, [200, 0, 300, 100])),
      { type: "CROP_PAGE_SELECTED", page: 1 },
      { type: "CROP_AREA_REMOVED", key: 2 },
      UNDO,
    );
    expect(keys(state)).toEqual([1, 2]);
    expect(state.selectedKey).toBe(2);
    expect(state.page).toBe(3);
  });

  it("puts a moved area back, a photo with its image again", () => {
    const photo: NewArea = {
      page: 1,
      kind: "photo",
      rectPt: [0, 0, 100, 100],
      imageIndex: 4,
    };
    const moved = reduce(add(open(), photo), {
      type: "CROP_AREA_CHANGED",
      key: 1,
      rectPt: [10, 10, 110, 110],
    });
    expect(moved.areas[0].kind).toBe("manual");

    const undone = reduce(moved, UNDO);
    expect(undone.areas[0]).toMatchObject({
      kind: "photo",
      imageIndex: 4,
      rectPt: [0, 0, 100, 100],
    });
  });

  it("puts a checkbox back", () => {
    const state = reduce(
      add(open(), figure(1)),
      { type: "CROP_AREA_CHECKED", key: 1, checked: false },
      UNDO,
    );
    expect(state.areas[0].checked).toBe(true);
  });

  it("is not a step for selecting, the page, the view or the settings", () => {
    const state = reduce(
      open(),
      { type: "CROP_PAGE_SELECTED", page: 2 },
      { type: "CROP_VIEW_SET", view: { kind: "fitWidth" } },
      { type: "CROP_CANDIDATES_SHOWN", shown: false },
      { type: "SET_CROP_FORMAT", format: "jpeg" },
    );
    expect(canUndo(state)).toBe(false);
    expect(reduce(state, UNDO)).toBe(state);
  });

  it("does nothing to select or press a repeat", () => {
    const added = add(open(), figure(1));
    const again = reduce(
      added,
      { type: "CROP_AREA_SELECTED", key: null },
      { type: "CROP_AREA_ADDED", area: figure(1) },
    );
    expect(again.history.past).toHaveLength(1);
  });

  it("drops what was undone once a new step is made", () => {
    const state = add(reduce(add(open(), figure(1)), UNDO), figure(2));
    expect(canRedo(state)).toBe(false);
  });

  it("keeps the last CROP_HISTORY_LIMIT steps", () => {
    let state = open();
    for (let index = 0; index <= CROP_HISTORY_LIMIT; index += 1) {
      state = add(state, figure(1, [index * 5, 0, index * 5 + 1, 1]));
    }
    for (let index = 0; index < CROP_HISTORY_LIMIT; index += 1) {
      state = reduce(state, UNDO);
    }
    expect(keys(state)).toEqual([1]);
    expect(canUndo(state)).toBe(false);
  });

  it("forgets everything when another PDF is opened", () => {
    const state = reduce(add(open(), figure(1)), {
      type: "CROP_PDF_ACTIVATED",
      pdf: pdf(2),
    });
    expect(canUndo(state)).toBe(false);
  });
});

describe("the keyboard's steps", () => {
  function move(key: number, rectPt: RectPt, byKeyboard = true): CropAction {
    return { type: "CROP_AREA_CHANGED", key, rectPt, byKeyboard };
  }

  it("make one step while they move the same area", () => {
    const state = reduce(
      add(open(), figure(1)),
      move(1, [1, 0, 101, 100]),
      move(1, [2, 0, 102, 100]),
      move(1, [3, 0, 103, 100]),
      UNDO,
    );
    expect(rectOf(state, 1)).toEqual([0, 0, 100, 100]);
  });

  it("start a step of their own after another area or a drag", () => {
    let state = add(open(), figure(1), figure(1, [200, 0, 300, 100]));
    state = reduce(
      state,
      move(1, [1, 0, 101, 100]),
      move(2, [201, 0, 301, 100]),
      move(1, [2, 0, 102, 100]),
      move(1, [50, 0, 150, 100], false),
      move(1, [51, 0, 151, 100]),
    );
    state = reduce(state, UNDO);
    expect(rectOf(state, 1)).toEqual([50, 0, 150, 100]);
    state = reduce(state, UNDO);
    expect(rectOf(state, 1)).toEqual([2, 0, 102, 100]);
    state = reduce(state, UNDO);
    expect(rectOf(state, 1)).toEqual([1, 0, 101, 100]);
    expect(rectOf(state, 2)).toEqual([201, 0, 301, 100]);
  });

  it("start again after an undo", () => {
    const state = reduce(
      add(open(), figure(1)),
      move(1, [1, 0, 101, 100]),
      UNDO,
      move(1, [5, 0, 105, 100]),
      move(1, [6, 0, 106, 100]),
      UNDO,
    );
    expect(rectOf(state, 1)).toEqual([0, 0, 100, 100]);
  });
});

describe("the areas the all-pages search adds", () => {
  const chart: Candidate = {
    kind: "chart",
    score: 0.9,
    rectPt: [10, 10, 200, 200],
    imageIndex: null,
  };

  it("go in one step, those that come page by page included", () => {
    let state = add(open(), figure(1));
    state = reduce(
      state,
      { type: "CROP_LAYOUT_STARTED", id: 1, page: 2 },
      {
        type: "CROP_LAYOUT_DONE",
        id: 1,
        page: 2,
        candidates: [chart],
      },
      { type: "CROP_DETECT_ALL_STARTED" },
      {
        type: "CROP_LAYOUT_PAGE",
        id: 1,
        page: 3,
        candidates: [chart],
        error: null,
      },
      {
        type: "CROP_LAYOUT_PAGE",
        id: 1,
        page: 4,
        candidates: [chart],
        error: null,
      },
    );
    expect(keys(state)).toEqual([1, 2, 3, 4]);

    state = reduce(state, UNDO);
    expect(keys(state)).toEqual([1]);
    expect(state.selectedKey).toBeNull();
    expect(state.page).toBe(2);
  });

  it("are not added after an undo", () => {
    const state = reduce(
      add(open(), figure(1)),
      { type: "CROP_LAYOUT_STARTED", id: 1, page: 3 },
      { type: "CROP_DETECT_ALL_STARTED" },
      UNDO,
      { type: "CROP_LAYOUT_DONE", id: 1, page: 3, candidates: [chart] },
    );
    expect(keys(state)).toEqual([]);
  });

  it("are not added after a later edit, which an undo would take away unseen", () => {
    let state = reduce(
      open(),
      { type: "CROP_LAYOUT_STARTED", id: 1, page: 3 },
      { type: "CROP_DETECT_ALL_STARTED" },
    );
    state = add(state, figure(1));
    state = reduce(state, {
      type: "CROP_LAYOUT_DONE",
      id: 1,
      page: 3,
      candidates: [chart],
    });
    expect(keys(state)).toEqual([1]);
    expect(keys(reduce(state, UNDO))).toEqual([]);
  });

  it("are passed over by an undo when they found nothing", () => {
    const state = reduce(add(open(), figure(1)), {
      type: "CROP_DETECT_ALL_STARTED",
    });
    expect(canUndo(state)).toBe(true);
    expect(keys(reduce(state, UNDO))).toEqual([]);
  });
});

describe("the exported areas", () => {
  function exported(state: CropState, ...exportedKeys: number[]): CropState {
    return { ...state, exportedKeys };
  }

  it("lose the mark when an undo moves or brings them back", () => {
    let state = exported(
      add(open(), figure(1), figure(1, [200, 0, 300, 100])),
      1,
      2,
    );
    state = reduce(state, {
      type: "CROP_AREA_CHANGED",
      key: 1,
      rectPt: [10, 0, 110, 100],
    });
    state = exported(state, 1, 2);
    state = reduce(state, UNDO);
    expect(state.exportedKeys).toEqual([2]);
    expect(unexportedAreaCount(state)).toBe(1);
  });

  it("keep it when an undo leaves them as they are", () => {
    let state = exported(add(open(), figure(1)), 1);
    state = reduce(add(state, figure(1, [200, 0, 300, 100])), UNDO);
    expect(state.exportedKeys).toEqual([1]);
  });
});

describe("undoing in the app", () => {
  function withAreas(): AppState {
    return {
      ...createInitialAppState("ja-JP"),
      crop: add(open(), figure(1), figure(1, [200, 0, 300, 100])),
    };
  }

  it("is refused while a conversion runs", () => {
    const running = appReducer(withAreas(), {
      type: "JOB_STARTED",
      kind: "pdfToImages",
      targets: [],
    });
    expect(appReducer(running, UNDO)).toBe(running);
    expect(appReducer(running, REDO)).toBe(running);
  });

  it("drops the crop result, as an edit does", () => {
    let state = appReducer(withAreas(), {
      type: "JOB_STARTED",
      kind: "crop",
      targets: [],
    });
    state = appReducer(state, {
      type: "JOB_FINISHED",
      finished: {
        succeeded: 2,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
    });
    expect(state.job.finished).not.toBeNull();
    state = appReducer(state, UNDO);
    expect(state.job.finished).toBeNull();
  });
});

describe("putting candidates aside", () => {
  const chart: Candidate = {
    kind: "chart",
    score: 0.9,
    rectPt: [10, 10, 200, 200],
    imageIndex: null,
  };
  const table: Candidate = {
    ...chart,
    kind: "table",
    rectPt: [300, 300, 400, 400],
  };

  it("is a step, which an undo takes back", () => {
    const hidden = reduce(open(), {
      type: "CROP_CANDIDATE_HIDDEN",
      page: 2,
      index: 1,
    });
    expect(hidden.hiddenCandidates).toEqual({ 2: [1] });
    expect(reduce(hidden, UNDO).hiddenCandidates).toEqual({});
    expect(reduce(hidden, UNDO, REDO).hiddenCandidates).toEqual({ 2: [1] });
  });

  it("is not done twice", () => {
    const action: CropAction = {
      type: "CROP_CANDIDATE_HIDDEN",
      page: 2,
      index: 1,
    };
    const once = reduce(open(), action);
    expect(reduce(once, action)).toBe(once);
  });

  it("keeps them out of the areas the all-pages search adds", () => {
    const state = reduce(
      open(),
      { type: "CROP_CANDIDATE_HIDDEN", page: 3, index: 0 },
      { type: "CROP_DETECT_ALL_STARTED" },
      {
        type: "CROP_LAYOUT_PAGE",
        id: 1,
        page: 3,
        candidates: [chart, table],
        error: null,
      },
    );
    expect(state.areas.map((area) => area.kind)).toEqual(["table"]);
  });

  it("come back, for that page only, as a step an undo takes back", () => {
    const hidden = reduce(
      open(),
      { type: "CROP_CANDIDATE_HIDDEN", page: 2, index: 1 },
      { type: "CROP_CANDIDATE_HIDDEN", page: 2, index: 0 },
      { type: "CROP_CANDIDATE_HIDDEN", page: 3, index: 0 },
    );
    const restored = reduce(hidden, {
      type: "CROP_CANDIDATES_RESTORED",
      page: 2,
    });
    expect(restored.hiddenCandidates).toEqual({ 3: [0] });
    expect(reduce(restored, UNDO).hiddenCandidates).toEqual({
      2: [1, 0],
      3: [0],
    });
  });

  it("do not come back where none was hidden", () => {
    const state = reduce(open(), {
      type: "CROP_CANDIDATE_HIDDEN",
      page: 3,
      index: 0,
    });
    expect(reduce(state, { type: "CROP_CANDIDATES_RESTORED", page: 2 })).toBe(
      state,
    );
  });

  it("are forgotten with another PDF", () => {
    const state = reduce(
      open(),
      { type: "CROP_CANDIDATE_HIDDEN", page: 3, index: 0 },
      { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
    );
    expect(state.hiddenCandidates).toEqual({});
  });
});

describe("the tool", () => {
  it("starts on select and is no step of the history", () => {
    const state = reduce(open(), { type: "CROP_TOOL_SET", tool: "draw" });
    expect(createInitialCropState().tool).toBe("select");
    expect(state.tool).toBe("draw");
    expect(canUndo(state)).toBe(false);
  });

  for (const action of [
    { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
    { type: "CROP_PDF_UNLOCKED", pdf: pdf(1) },
  ] as const) {
    it(`goes back to select on ${action.type}`, () => {
      const state = reduce(
        open(),
        { type: "CROP_TOOL_SET", tool: "draw" },
        action,
      );
      expect(state.tool).toBe("select");
    });
  }
});
