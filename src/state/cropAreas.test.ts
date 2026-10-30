import { describe, expect, it } from "vitest";
import type { Candidate, CropPdf, IpcError } from "../ipc";
import { createInitialAppState, appReducer } from "./appReducer";
import {
  CROP_DUPLICATE_IOU,
  createInitialCropState,
  cropReducer,
  rectIou,
  unexportedAreaCount,
} from "./crop";
import type {
  AppAction,
  AppState,
  CropAction,
  CropState,
  NewArea,
  RectPt,
} from "./types";

const A4 = { widthPt: 595, heightPt: 842 };
const CRASHED: IpcError = { code: "WorkerCrashed", detail: null };
const UNAVAILABLE: IpcError = { code: "LayoutUnavailable", detail: null };

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

function photo(
  page: number,
  imageIndex: number,
  rectPt: RectPt = [0, 0, 100, 100],
): NewArea {
  return { page, kind: "photo", rectPt, imageIndex };
}

function candidate(
  kind: Candidate["kind"],
  rectPt: RectPt,
  imageIndex: number | null = null,
): Candidate {
  return { kind, score: 0.9, rectPt, imageIndex };
}

function add(state: CropState, ...areas: NewArea[]): CropState {
  return reduce(
    state,
    ...areas.map((area): CropAction => ({ type: "CROP_AREA_ADDED", area })),
  );
}

describe("rectIou", () => {
  it("is 0 for rectangles that are apart", () => {
    expect(rectIou([0, 0, 10, 10], [20, 20, 30, 30])).toBe(0);
  });

  it("is 0 for rectangles that only touch", () => {
    expect(rectIou([0, 0, 10, 10], [10, 0, 20, 10])).toBe(0);
  });

  it("is 1 for the same rectangle", () => {
    expect(rectIou([5, 5, 25, 15], [5, 5, 25, 15])).toBe(1);
  });

  it("is the share of the larger one that the smaller one covers when it lies inside", () => {
    expect(rectIou([0, 0, 10, 10], [0, 0, 5, 10])).toBeCloseTo(0.5);
  });

  it("is the shared area over the area they cover together", () => {
    // 5 × 10 shared; 100 + 100 - 50 = 150 together.
    expect(rectIou([0, 0, 10, 10], [5, 0, 15, 10])).toBeCloseTo(50 / 150);
  });

  it("is 0 when both are empty", () => {
    expect(rectIou([3, 3, 3, 3], [3, 3, 3, 3])).toBe(0);
  });

  it("is 0 for an empty rectangle and a real one", () => {
    expect(rectIou([3, 3, 3, 3], [0, 0, 10, 10])).toBe(0);
  });
});

describe("adding areas", () => {
  it("compares from an IoU of 0.9", () => {
    expect(CROP_DUPLICATE_IOU).toBe(0.9);
  });

  it("numbers the areas from 1, checked, with no overlay mark yet", () => {
    const state = add(
      open(),
      photo(1, 0),
      figure(2),
      figure(1, [200, 0, 300, 100]),
    );
    expect(state.areas.map((a) => [a.key, a.page, a.checked, a.onTop])).toEqual(
      [
        [1, 1, true, null],
        [2, 2, true, null],
        [3, 1, true, null],
      ],
    );
    expect(state.nextAreaKey).toBe(4);
  });

  it("selects the area a press adds", () => {
    const state = add(open(), figure(1), figure(2));
    expect(state.selectedKey).toBe(2);
  });

  it("does not add an area that repeats one on the page, and selects the one there", () => {
    const state = reduce(add(open(), figure(1), figure(2)), {
      type: "CROP_AREA_SELECTED",
      key: 2,
    });
    const next = add(state, figure(1));
    expect(next.areas).toHaveLength(2);
    expect(next.selectedKey).toBe(1);
    expect(next.nextAreaKey).toBe(3);
  });

  it("does not add an area at an IoU of exactly 0.9", () => {
    // 90 × 100 against 100 × 100: 9000 / 10000.
    const state = add(open(), figure(1, [0, 0, 100, 100]));
    expect(rectIou([0, 0, 100, 100], [0, 0, 90, 100])).toBeCloseTo(0.9);
    expect(add(state, figure(1, [0, 0, 90, 100])).areas).toHaveLength(1);
  });

  it("adds an area under an IoU of 0.9", () => {
    const state = add(open(), figure(1, [0, 0, 100, 100]));
    expect(add(state, figure(1, [0, 0, 89, 100])).areas).toHaveLength(2);
  });

  it("adds the same rectangle on another page", () => {
    expect(add(open(), figure(1), figure(2)).areas).toHaveLength(2);
  });

  it("adds a batch without selecting, and skips what repeats, within the batch too", () => {
    const state = reduce(open(), {
      type: "CROP_AREAS_ADDED",
      areas: [figure(1), figure(1), figure(1, [200, 0, 300, 100])],
    });
    expect(state.areas.map((a) => a.rectPt[0])).toEqual([0, 200]);
    expect(state.selectedKey).toBeNull();
  });

  it("does not count an area an export has written", () => {
    const state = { ...add(open(), figure(1), figure(2)), exportedKeys: [1] };
    expect(unexportedAreaCount(state)).toBe(1);
  });

  it("counts the unchecked areas among those not exported", () => {
    const state = reduce(add(open(), figure(1), figure(2)), {
      type: "CROP_AREA_CHECKED",
      key: 1,
      checked: false,
    });
    expect(unexportedAreaCount(state)).toBe(2);
  });
});

describe("selecting, checking and removing areas", () => {
  it("selects one area at a time and none with null", () => {
    let state = add(open(), figure(1), figure(2));
    state = reduce(state, { type: "CROP_AREA_SELECTED", key: 1 });
    expect(state.selectedKey).toBe(1);
    state = reduce(state, { type: "CROP_AREA_SELECTED", key: null });
    expect(state.selectedKey).toBeNull();
  });

  it("ignores selecting an area that is not there", () => {
    const state = add(open(), figure(1));
    expect(reduce(state, { type: "CROP_AREA_SELECTED", key: 9 })).toBe(state);
  });

  it("changes the check of one area", () => {
    const state = reduce(add(open(), figure(1), figure(2)), {
      type: "CROP_AREA_CHECKED",
      key: 2,
      checked: false,
    });
    expect(state.areas.map((a) => a.checked)).toEqual([true, false]);
  });

  it("removes an area, leaves none selected if it was, and does not reuse its key", () => {
    let state = add(open(), figure(1), figure(2));
    state = reduce(state, { type: "CROP_AREA_REMOVED", key: 2 });
    expect(state.areas.map((a) => a.key)).toEqual([1]);
    expect(state.selectedKey).toBeNull();
    state = add(state, figure(3));
    expect(state.areas.map((a) => a.key)).toEqual([1, 3]);
  });

  it("keeps the selection when another area is removed", () => {
    let state = add(open(), figure(1), figure(2));
    state = reduce(state, { type: "CROP_AREA_REMOVED", key: 1 });
    expect(state.selectedKey).toBe(2);
  });
});

describe("replacing the PDF", () => {
  function withEverything(): CropState {
    let state = add(open(1), photo(1, 0), figure(2));
    state = {
      ...state,
      exportedKeys: [1],
      detectAwaiting: [3],
      onTopRequests: { 1: "failed" },
    };
    return state;
  }

  for (const action of [
    { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
    { type: "CROP_PDF_CLOSED" },
  ] as const) {
    it(`drops the areas on ${action.type}`, () => {
      const state = reduce(withEverything(), action);
      expect(state.areas).toEqual([]);
      expect(state.nextAreaKey).toBe(1);
      expect(state.selectedKey).toBeNull();
      expect(state.exportedKeys).toEqual([]);
      expect(state.detectAwaiting).toEqual([]);
      expect(state.onTopRequests).toEqual({});
    });
  }

  it("drops the areas when the same PDF is unlocked", () => {
    const state = reduce(withEverything(), {
      type: "CROP_PDF_UNLOCKED",
      pdf: pdf(1),
    });
    expect(state.areas).toEqual([]);
    expect(state.nextAreaKey).toBe(1);
  });

  it("numbers the areas of the next PDF from 1", () => {
    const state = add(
      reduce(withEverything(), { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) }),
      figure(1),
    );
    expect(state.areas[0].key).toBe(1);
  });

  it("closes the question about replacing when the PDF is replaced", () => {
    const pending = { pdf: pdf(2), source: "dialog" as const };
    const state = reduce(
      reduce(open(), { type: "CROP_REPLACE_PENDING", pending }),
      { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
    );
    expect(state.pendingReplace).toBeNull();
  });
});

describe("the overlay mark", () => {
  function withPhoto(): CropState {
    return add(open(1), photo(1, 0));
  }

  it("is set by the answer", () => {
    const state = reduce(
      withPhoto(),
      { type: "CROP_ON_TOP_STARTED", id: 1, key: 1 },
      { type: "CROP_ON_TOP_DONE", id: 1, key: 1, onTop: true },
    );
    expect(state.areas[0].onTop).toBe(true);
    expect(state.onTopRequests).toEqual({});
  });

  it("records a request that is on its way and one that failed", () => {
    const asked = reduce(withPhoto(), {
      type: "CROP_ON_TOP_STARTED",
      id: 1,
      key: 1,
    });
    expect(asked.onTopRequests).toEqual({ 1: "pending" });
    const failed = reduce(asked, { type: "CROP_ON_TOP_FAILED", id: 1, key: 1 });
    expect(failed.onTopRequests).toEqual({ 1: "failed" });
    expect(failed.areas[0].onTop).toBeNull();
  });

  it("drops the answer for another PDF", () => {
    const state = withPhoto();
    expect(
      reduce(state, { type: "CROP_ON_TOP_DONE", id: 2, key: 1, onTop: true }),
    ).toBe(state);
    expect(reduce(state, { type: "CROP_ON_TOP_FAILED", id: 2, key: 1 })).toBe(
      state,
    );
  });

  it("drops the answer for an area that has been removed", () => {
    const state = reduce(withPhoto(), { type: "CROP_AREA_REMOVED", key: 1 });
    expect(
      reduce(state, { type: "CROP_ON_TOP_DONE", id: 1, key: 1, onTop: true }),
    ).toBe(state);
  });

  it("drops the answer for a photo that has been moved", () => {
    const state = reduce(
      withPhoto(),
      { type: "CROP_ON_TOP_STARTED", id: 1, key: 1 },
      { type: "CROP_AREA_CHANGED", key: 1, rectPt: [1, 1, 50, 50] },
    );
    expect(
      reduce(state, { type: "CROP_ON_TOP_DONE", id: 1, key: 1, onTop: true }),
    ).toBe(state);
    expect(reduce(state, { type: "CROP_ON_TOP_FAILED", id: 1, key: 1 })).toBe(
      state,
    );
  });
});

describe("all pages' candidates", () => {
  const page2 = [
    candidate("photo", [0, 0, 50, 50], 4),
    candidate("table", [0, 100, 50, 150]),
  ];

  function layoutDone(
    state: CropState,
    page: number,
    candidates: Candidate[],
  ): CropState {
    return reduce(
      state,
      { type: "CROP_LAYOUT_STARTED", id: 1, page },
      { type: "CROP_LAYOUT_DONE", id: 1, page, candidates },
    );
  }

  it("adds what the pages already analyzed brought, by page and in order, without selecting", () => {
    let state = layoutDone(open(), 3, [candidate("figure", [0, 0, 10, 10])]);
    state = layoutDone(state, 2, page2);
    state = reduce(state, { type: "CROP_DETECT_ALL_STARTED" });

    expect(state.areas.map((a) => [a.page, a.kind, a.imageIndex])).toEqual([
      [2, "photo", 4],
      [2, "table", null],
      [3, "figure", null],
    ]);
    expect(state.selectedKey).toBeNull();
    expect(state.lastJob).toBe("detectAll");
    expect(state.detectAdded).toBe(3);
  });

  it("does not count what repeats an area that is there", () => {
    let state = layoutDone(open(), 2, page2);
    state = add(state, photo(2, 4, [0, 0, 50, 50]));
    state = reduce(state, { type: "CROP_DETECT_ALL_STARTED" });
    expect(state.areas).toHaveLength(2);
    expect(state.detectAdded).toBe(1);
  });

  it("starts the count again", () => {
    let state = layoutDone(open(), 2, page2);
    state = reduce(state, { type: "CROP_DETECT_ALL_STARTED" });
    state = reduce(state, { type: "CROP_DETECT_ALL_STARTED" });
    expect(state.areas).toHaveLength(2);
    expect(state.detectAdded).toBe(0);
  });

  it("keeps the kind and image of a candidate, and no image for the other kinds", () => {
    const state = reduce(
      layoutDone(open(), 1, [
        candidate("photo", [0, 0, 50, 50], 2),
        candidate("chart", [0, 100, 50, 150], 7),
      ]),
      { type: "CROP_DETECT_ALL_STARTED" },
    );
    expect(state.areas.map((a) => a.imageIndex)).toEqual([2, null]);
  });

  describe("the pages that were still being analyzed", () => {
    function started(): CropState {
      return reduce(
        open(),
        { type: "CROP_LAYOUT_STARTED", id: 1, page: 4 },
        { type: "CROP_LAYOUT_STARTED", id: 1, page: 2 },
        { type: "CROP_DETECT_ALL_STARTED" },
      );
    }

    it("are waited for, in page order", () => {
      expect(started().detectAwaiting).toEqual([2, 4]);
    });

    it("add their candidates when the answer arrives, and are no longer waited for", () => {
      const state = reduce(started(), {
        type: "CROP_LAYOUT_DONE",
        id: 1,
        page: 4,
        candidates: [candidate("figure", [0, 0, 10, 10])],
      });
      expect(state.areas.map((a) => a.page)).toEqual([4]);
      expect(state.detectAdded).toBe(1);
      expect(state.detectAwaiting).toEqual([2]);
    });

    it("add them after the conversion has finished too", () => {
      const running = appReducer(
        {
          ...createInitialAppState("ja-JP"),
          crop: started(),
        },
        { type: "JOB_STARTED", kind: "crop", targets: [] },
      );
      const finished = appReducer(running, {
        type: "JOB_FINISHED",
        finished: {
          succeeded: 0,
          failed: 0,
          noPages: 0,
          unprocessed: 0,
          cancelled: false,
        },
      });
      const state = appReducer(finished, {
        type: "CROP_LAYOUT_DONE",
        id: 1,
        page: 2,
        candidates: [candidate("table", [0, 0, 10, 10])],
      });
      expect(state.crop.areas).toHaveLength(1);
      expect(state.job.finished).not.toBeNull();
    });

    it("stop being waited for when their analysis fails", () => {
      const state = reduce(started(), {
        type: "CROP_LAYOUT_FAILED",
        id: 1,
        page: 2,
        error: CRASHED,
      });
      expect(state.detectAwaiting).toEqual([4]);
      expect(state.areas).toEqual([]);
    });

    it("add nothing for a page that was not waited for", () => {
      const state = layoutDone(started(), 5, [
        candidate("figure", [0, 0, 10, 10]),
      ]);
      expect(state.areas).toEqual([]);
    });

    it("add nothing for an answer of another PDF", () => {
      const state = reduce(started(), {
        type: "CROP_LAYOUT_DONE",
        id: 2,
        page: 2,
        candidates: [candidate("figure", [0, 0, 10, 10])],
      });
      expect(state.areas).toEqual([]);
      expect(state.detectAwaiting).toEqual([2, 4]);
    });
  });

  describe("the answer for a page", () => {
    const answer = (
      candidates: Candidate[],
      error: IpcError | null = null,
      id = 1,
    ): CropAction => ({
      type: "CROP_LAYOUT_PAGE",
      id,
      page: 2,
      candidates,
      error,
    });

    it("adds the candidates and is the page's analysis result", () => {
      const state = reduce(open(), answer(page2));
      expect(state.areas).toHaveLength(2);
      expect(state.detectAdded).toBe(2);
      expect(state.layouts[2]).toEqual({ status: "done", value: page2 });
    });

    it("counts only what is new when a page is answered twice", () => {
      const state = reduce(open(), answer(page2), answer(page2));
      expect(state.areas).toHaveLength(2);
      expect(state.detectAdded).toBe(2);
    });

    it("makes the page fail with its error", () => {
      const state = reduce(open(), answer([], CRASHED));
      expect(state.layouts[2]).toEqual({ status: "failed", error: CRASHED });
      expect(state.layoutUnavailable).toBe(false);
      expect(state.areas).toEqual([]);
    });

    it("marks the analysis unavailable for LayoutUnavailable", () => {
      const state = reduce(open(), answer([], UNAVAILABLE));
      expect(state.layoutUnavailable).toBe(true);
    });

    it("drops the answer for another PDF", () => {
      const state = open();
      expect(reduce(state, answer(page2, null, 2))).toBe(state);
    });

    it("stops waiting for the page, so the detect_layout answer adds nothing more", () => {
      let state = reduce(
        open(),
        { type: "CROP_LAYOUT_STARTED", id: 1, page: 2 },
        { type: "CROP_DETECT_ALL_STARTED" },
        answer(page2),
      );
      expect(state.detectAwaiting).toEqual([]);
      state = reduce(state, {
        type: "CROP_LAYOUT_DONE",
        id: 1,
        page: 2,
        candidates: [candidate("figure", [300, 300, 400, 400])],
      });
      expect(state.areas).toHaveLength(2);
    });
  });
});

describe("drawing an area", () => {
  const drawn: CropAction = {
    type: "CROP_AREA_DRAWN",
    page: 2,
    rectPt: [10, 20, 110, 220],
  };

  it("adds a hand-picked, checked area and selects it", () => {
    const state = reduce(add(open(), figure(1)), drawn);
    expect(state.areas[1]).toEqual({
      key: 2,
      page: 2,
      kind: "manual",
      rectPt: [10, 20, 110, 220],
      imageIndex: null,
      checked: true,
      onTop: null,
    });
    expect(state.selectedKey).toBe(2);
    expect(state.nextAreaKey).toBe(3);
  });

  it("adds it however much it overlaps an area of the page", () => {
    const state = reduce(add(open(), figure(2, [10, 20, 110, 220])), drawn);
    expect(state.areas).toHaveLength(2);
    expect(rectIou(state.areas[0].rectPt, state.areas[1].rectPt)).toBe(1);
  });
});

describe("moving and resizing an area", () => {
  const moved: RectPt = [10, 20, 60, 70];

  function change(state: CropState, key: number, rectPt: RectPt): CropState {
    return reduce(state, { type: "CROP_AREA_CHANGED", key, rectPt });
  }

  it("replaces the rectangle and keeps the kind, the check and the selection", () => {
    const base = reduce(
      add(open(), figure(1), figure(1, [200, 0, 300, 100])),
      { type: "CROP_AREA_CHECKED", key: 1, checked: false },
      { type: "CROP_AREA_SELECTED", key: 2 },
    );
    const state = change(base, 1, moved);
    expect(state.areas[0]).toMatchObject({
      key: 1,
      kind: "figure",
      rectPt: moved,
      checked: false,
    });
    expect(state.areas[1]).toBe(base.areas[1]);
    expect(state.selectedKey).toBe(2);
  });

  it("makes a photo a hand-picked area, with no image and no overlay mark", () => {
    const base = reduce(add(open(), photo(1, 3)), {
      type: "CROP_ON_TOP_DONE",
      id: 1,
      key: 1,
      onTop: true,
    });
    expect(base.areas[0].onTop).toBe(true);
    const state = change(base, 1, moved);
    expect(state.areas[0]).toMatchObject({
      kind: "manual",
      imageIndex: null,
      onTop: null,
      rectPt: moved,
    });
  });

  it("drops what was asked of check_on_top for a photo", () => {
    const base = reduce(add(open(), photo(1, 3)), {
      type: "CROP_ON_TOP_STARTED",
      id: 1,
      key: 1,
    });
    expect(base.onTopRequests).toEqual({ 1: "pending" });
    expect(change(base, 1, moved).onTopRequests).toEqual({});
  });

  it("keeps the kind of the areas that are not photos", () => {
    for (const kind of ["figure", "chart", "table", "manual"] as const) {
      const base = add(open(), { ...figure(1), kind });
      expect(change(base, 1, moved).areas[0].kind).toBe(kind);
    }
  });

  it("takes the area off the exported ones, so the replace question counts it", () => {
    const base = {
      ...add(open(), figure(1), figure(2)),
      exportedKeys: [1, 2],
    };
    expect(unexportedAreaCount(base)).toBe(0);
    const state = change(base, 1, moved);
    expect(state.exportedKeys).toEqual([2]);
    expect(unexportedAreaCount(state)).toBe(1);
  });

  it("is not subject to the check for a repeat", () => {
    const base = add(open(), figure(1), figure(1, [200, 0, 300, 100]));
    const state = change(base, 2, [0, 0, 100, 100]);
    expect(state.areas[1].rectPt).toEqual([0, 0, 100, 100]);
  });

  it("returns the same state for the rectangle the area has", () => {
    const base = { ...add(open(), figure(1)), exportedKeys: [1] };
    expect(change(base, 1, [0, 0, 100, 100])).toBe(base);
  });

  it("returns the same state for an area that is not there", () => {
    const base = add(open(), figure(1));
    expect(change(base, 9, moved)).toBe(base);
  });
});

describe("the areas while a conversion runs", () => {
  function runningApp(): AppState {
    const state = {
      ...createInitialAppState("ja-JP"),
      crop: add(open(), figure(1)),
    };
    return appReducer(state, {
      type: "JOB_STARTED",
      kind: "pdfToImages",
      targets: [],
    });
  }

  it("takes no area added, drawn, changed, removed or checked", () => {
    const state = runningApp();
    const actions: AppAction[] = [
      { type: "CROP_AREA_ADDED", area: figure(2) },
      { type: "CROP_AREAS_ADDED", areas: [figure(2)] },
      { type: "CROP_AREA_DRAWN", page: 2, rectPt: [10, 10, 50, 50] },
      { type: "CROP_AREA_CHANGED", key: 1, rectPt: [10, 10, 50, 50] },
      { type: "CROP_AREA_REMOVED", key: 1 },
      { type: "CROP_AREA_CHECKED", key: 1, checked: false },
    ];
    for (const action of actions) {
      expect(appReducer(state, action)).toBe(state);
    }
  });

  it("takes an area selected", () => {
    const state = runningApp();
    const next = appReducer(state, { type: "CROP_AREA_SELECTED", key: 1 });
    expect(next.crop.selectedKey).toBe(1);
  });

  it("takes the areas of all pages' candidates", () => {
    const state = runningApp();
    const withLayout = appReducer(state, {
      type: "CROP_LAYOUT_PAGE",
      id: 1,
      page: 3,
      candidates: [candidate("figure", [0, 0, 10, 10])],
      error: null,
    });
    expect(withLayout.crop.areas).toHaveLength(2);

    const started = appReducer(
      {
        ...state,
        crop: reduce(
          state.crop,
          {
            type: "CROP_LAYOUT_STARTED",
            id: 1,
            page: 2,
          },
          {
            type: "CROP_LAYOUT_DONE",
            id: 1,
            page: 2,
            candidates: [candidate("table", [0, 0, 10, 10])],
          },
        ),
      },
      { type: "CROP_DETECT_ALL_STARTED" },
    );
    expect(started.crop.areas).toHaveLength(2);
  });

  it("takes them again once it has finished", () => {
    const finished = appReducer(runningApp(), {
      type: "JOB_FINISHED",
      finished: {
        succeeded: 1,
        failed: 0,
        noPages: 0,
        unprocessed: 0,
        cancelled: false,
      },
    });
    const next = appReducer(finished, {
      type: "CROP_AREA_ADDED",
      area: figure(2),
    });
    expect(next.crop.areas).toHaveLength(2);
  });
});

describe("the areas an export has written", () => {
  function exporting(lastJob: CropState["lastJob"]): AppState {
    const state = {
      ...createInitialAppState("ja-JP"),
      crop: { ...add(open(), figure(1), figure(2)), lastJob },
    };
    return appReducer(state, {
      type: "JOB_STARTED",
      kind: "crop",
      targets: [
        { id: 1, name: "a.pdf" },
        { id: 2, name: "a.pdf" },
      ],
    });
  }

  it("are those whose item succeeded", () => {
    let state = exporting("export");
    state = appReducer(state, {
      type: "JOB_ITEM",
      item: { id: 2, status: "ok", outputs: ["a.png"] },
    });
    state = appReducer(state, {
      type: "JOB_ITEM",
      item: { id: 1, status: "failed", outputs: [] },
    });
    expect(state.crop.exportedKeys).toEqual([2]);
  });

  it("are not counted twice", () => {
    let state = exporting("export");
    const item = { id: 2, status: "ok" as const, outputs: ["a.png"] };
    state = appReducer(state, { type: "JOB_ITEM", item });
    state = appReducer(state, { type: "JOB_ITEM", item });
    expect(state.crop.exportedKeys).toEqual([2]);
  });

  it("are not those of another tab's conversion", () => {
    const state = appReducer(
      appReducer(
        {
          ...createInitialAppState("ja-JP"),
          crop: { ...add(open(), figure(1)), lastJob: "export" },
        },
        {
          type: "JOB_STARTED",
          kind: "pdfToImages",
          targets: [{ id: 1, name: "x" }],
        },
      ),
      { type: "JOB_ITEM", item: { id: 1, status: "ok", outputs: [] } },
    );
    expect(state.crop.exportedKeys).toEqual([]);
  });

  it("are not those of all pages' candidates", () => {
    const state = appReducer(exporting("detectAll"), {
      type: "JOB_ITEM",
      item: { id: 1, status: "ok", outputs: [] },
    });
    expect(state.crop.exportedKeys).toEqual([]);
  });

  it("are forgotten with the area that is removed", () => {
    let state = appReducer(exporting("export"), {
      type: "JOB_ITEM",
      item: { id: 1, status: "ok", outputs: [] },
    });
    state = appReducer(state, {
      type: "JOB_FINISHED",
      finished: {
        succeeded: 1,
        failed: 0,
        noPages: 0,
        unprocessed: 1,
        cancelled: false,
      },
    });
    state = appReducer(state, { type: "CROP_AREA_REMOVED", key: 1 });
    expect(state.crop.exportedKeys).toEqual([]);
  });
});

describe("dropping the last crop result", () => {
  const finished = {
    succeeded: 1,
    failed: 0,
    noPages: 0,
    unprocessed: 0,
    cancelled: false,
  };

  function withResult(): AppState {
    const state = {
      ...createInitialAppState("ja-JP"),
      crop: add(open(), figure(1), photo(2, 0)),
    };
    return appReducer(
      appReducer(state, {
        type: "JOB_STARTED",
        kind: "crop",
        targets: [{ id: 1, name: "a.pdf" }],
      }),
      { type: "JOB_FINISHED", finished },
    );
  }

  const dropping: AppAction[] = [
    { type: "CROP_AREA_ADDED", area: figure(3) },
    { type: "CROP_AREAS_ADDED", areas: [figure(3)] },
    { type: "CROP_AREA_DRAWN", page: 3, rectPt: [10, 10, 50, 50] },
    { type: "CROP_AREA_CHANGED", key: 1, rectPt: [10, 10, 50, 50] },
    { type: "CROP_AREA_REMOVED", key: 1 },
    { type: "CROP_AREA_CHECKED", key: 1, checked: false },
    { type: "SET_CROP_FORMAT", format: "jpeg" },
    { type: "SET_CROP_COLOR_MODE", colorMode: "gray" },
    { type: "SET_CROP_DPI", dpi: 300 },
    { type: "SET_CROP_IS_CUSTOM_DPI", isCustom: true },
    { type: "SET_CROP_CUSTOM_DPI_TEXT", text: "200" },
    { type: "SET_CROP_PHOTO_NATIVE", photoNative: false },
    { type: "SET_CROP_PHOTO_ONLY", photoOnly: true },
    { type: "SET_CROP_OUTPUT_DIR", outputDir: { dirLabel: "x" } },
    { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
    { type: "CROP_PDF_CLOSED" },
  ];

  for (const action of dropping) {
    it(`is dropped by ${action.type}`, () => {
      const state = appReducer(withResult(), action);
      expect(state.job.finished).toBeNull();
    });
  }

  const keeping: AppAction[] = [
    { type: "CROP_PAGE_SELECTED", page: 2 },
    { type: "CROP_VIEW_SET", view: { kind: "fitWidth" } },
    { type: "CROP_CANDIDATES_SHOWN", shown: false },
    { type: "CROP_AREA_SELECTED", key: 1 },
    { type: "CROP_PAGE_IMAGES_STARTED", id: 1, page: 2 },
    { type: "CROP_PAGE_IMAGES_DONE", id: 1, page: 2, images: [] },
    {
      type: "CROP_PAGE_IMAGES_FAILED",
      id: 1,
      page: 2,
      error: CRASHED,
    },
    { type: "CROP_LAYOUT_STARTED", id: 1, page: 2 },
    {
      type: "CROP_LAYOUT_DONE",
      id: 1,
      page: 2,
      candidates: [candidate("table", [300, 300, 400, 400])],
    },
    { type: "CROP_LAYOUT_FAILED", id: 1, page: 2, error: CRASHED },
    {
      type: "CROP_LAYOUT_PAGE",
      id: 1,
      page: 2,
      candidates: [candidate("table", [300, 300, 400, 400])],
      error: null,
    },
    { type: "CROP_ON_TOP_STARTED", id: 1, key: 2 },
    { type: "CROP_ON_TOP_DONE", id: 1, key: 2, onTop: true },
    { type: "CROP_ON_TOP_FAILED", id: 1, key: 2 },
    { type: "CROP_PASSWORD_DIALOG", open: true },
    {
      type: "CROP_OPEN_NOTICE",
      notice: { kind: "error", error: CRASHED },
    },
    { type: "CROP_PDF_UNLOCKED", pdf: pdf(1) },
    { type: "SET_CROP_SETTINGS_OPEN", open: false },
    { type: "CROP_DETECT_ALL_STARTED" },
  ];

  for (const action of keeping) {
    it(`is kept by ${action.type}`, () => {
      // `CROP_PDF_UNLOCKED` resets the areas it belongs to, but not the result.
      const state = appReducer(withResult(), action);
      expect(state.job.finished).toEqual(finished);
    });
  }

  it("is kept when an area is changed to the rectangle it has", () => {
    const state = appReducer(withResult(), {
      type: "CROP_AREA_CHANGED",
      key: 1,
      rectPt: [0, 0, 100, 100],
    });
    expect(state.job.finished).toEqual(finished);
  });

  it("is kept when all pages' candidates add areas", () => {
    const state = appReducer(withResult(), {
      type: "CROP_LAYOUT_PAGE",
      id: 1,
      page: 4,
      candidates: [candidate("table", [300, 300, 400, 400])],
      error: null,
    });
    expect(state.crop.areas).toHaveLength(3);
    expect(state.job.finished).toEqual(finished);
  });

  it("is not dropped while a conversion runs (it has none yet)", () => {
    const running = appReducer(
      { ...createInitialAppState("ja-JP"), crop: open() },
      { type: "JOB_STARTED", kind: "crop", targets: [] },
    );
    const state = appReducer(running, { type: "SET_CROP_DPI", dpi: 300 });
    expect(state.job.phase).toBe("running");
  });
});
