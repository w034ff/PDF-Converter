import { afterEach, describe, expect, it, vi } from "vitest";
import type { CropPdf, IpcError, Settings } from "../ipc";
import {
  CROP_PAGE_SIDE_STEPS,
  CROP_ZOOM_STEPS,
  PAGE_VIEW_MAX_SIDE,
  createInitialCropState,
  cropReducer,
} from "./crop";
import { createInitialAppState } from "./appReducer";
import type { CropAction, CropState } from "./types";

const A4 = { widthPt: 595, heightPt: 842 };

function pdf(id: number, overrides: Partial<CropPdf> = {}): CropPdf {
  return {
    id,
    name: `file-${id}.pdf`,
    pageCount: 5,
    pages: Array.from({ length: 5 }, () => A4),
    error: null,
    ...overrides,
  };
}

const PASSWORD: IpcError = { code: "PasswordProtected", detail: null };
const CRASHED: IpcError = { code: "WorkerCrashed", detail: null };

function reduce(state: CropState, ...actions: CropAction[]): CropState {
  return actions.reduce(cropReducer, state);
}

function open(id = 1): CropState {
  return reduce(createInitialCropState(), {
    type: "CROP_PDF_ACTIVATED",
    pdf: pdf(id),
  });
}

describe("the constants", () => {
  it("end the page sizes at the largest the worker serves", () => {
    expect(PAGE_VIEW_MAX_SIDE).toBe(2400);
    expect(CROP_PAGE_SIDE_STEPS.at(-1)).toBe(PAGE_VIEW_MAX_SIDE);
    expect([...CROP_PAGE_SIDE_STEPS].sort((a, b) => a - b)).toEqual(
      CROP_PAGE_SIDE_STEPS,
    );
  });

  it("have the zoom steps of the design, in order", () => {
    expect(CROP_ZOOM_STEPS).toEqual([
      25, 33, 50, 67, 75, 90, 100, 125, 150, 200, 300, 400,
    ]);
  });
});

describe("createInitialCropState", () => {
  const originalHeight = window.innerHeight;

  afterEach(() => {
    vi.stubGlobal("innerHeight", originalHeight);
  });

  it("starts with nothing open, fitted to the page, candidates shown", () => {
    const state = createInitialCropState();
    expect(state.pdf).toBeNull();
    expect(state.page).toBe(1);
    expect(state.view).toEqual({ kind: "fitPage" });
    expect(state.showCandidates).toBe(true);
    expect(state.pageImages).toEqual({});
    expect(state.layouts).toEqual({});
    expect(state.passwordDialogOpen).toBe(false);
    expect(state.openNotice).toBeNull();
  });

  it("starts from the defaults of the design without saved settings", () => {
    const state = createInitialCropState();
    expect(state).toMatchObject({
      format: "png",
      colorMode: "color",
      dpi: 150,
      isCustomDpi: false,
      customDpiText: "150",
      photoNative: true,
      photoOnly: false,
      outputDir: null,
    });
  });

  it("takes the saved settings", () => {
    const state = createInitialCropState({
      format: "webp",
      colorMode: "gray",
      dpi: 300,
      photoNative: false,
      photoOnly: true,
      outputDir: { dirLabel: "figures" },
    });
    expect(state).toMatchObject({
      format: "webp",
      colorMode: "gray",
      dpi: 300,
      isCustomDpi: false,
      photoNative: false,
      photoOnly: true,
      outputDir: { dirLabel: "figures" },
    });
  });

  it("starts a saved resolution that is not a preset as a custom entry", () => {
    const state = createInitialCropState({
      format: "png",
      colorMode: "color",
      dpi: 220,
      photoNative: true,
      photoOnly: false,
      outputDir: null,
    });
    expect(state).toMatchObject({
      dpi: 220,
      isCustomDpi: true,
      customDpiText: "220",
    });
  });

  it.each([400, 720, 1200])(
    "starts with the export settings closed at a window height of %i",
    (height) => {
      vi.stubGlobal("innerHeight", height);
      expect(createInitialCropState().settingsOpen).toBe(false);
    },
  );

  it("keeps the export settings as they are when another PDF is open", () => {
    const opened = reduce(open(1), {
      type: "SET_CROP_SETTINGS_OPEN",
      open: true,
    });
    for (const action of [
      { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
      { type: "CROP_PDF_UNLOCKED", pdf: pdf(2) },
    ] satisfies CropAction[]) {
      expect(reduce(opened, action).settingsOpen).toBe(true);
    }
    const closed = reduce(opened, {
      type: "SET_CROP_SETTINGS_OPEN",
      open: false,
    });
    expect(
      reduce(closed, { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) }).settingsOpen,
    ).toBe(false);
  });

  it("is part of the app state, from the saved settings", () => {
    const settings: Settings = {
      language: null,
      imagesToPdf: {
        output: "merge",
        pageSize: "fit",
        orientation: "auto",
        pdfA: false,
        encrypt: false,
        outputDir: null,
      },
      pdfToImages: {
        format: "png",
        colorMode: "color",
        dpi: 150,
        outputDir: null,
      },
      crop: {
        format: "jpeg",
        colorMode: "gray",
        dpi: 72,
        photoNative: false,
        photoOnly: true,
        outputDir: { dirLabel: "out" },
      },
      organize: { output: "single", encrypt: false, outputDir: null },
    };
    expect(createInitialAppState("ja", settings).crop).toMatchObject({
      format: "jpeg",
      colorMode: "gray",
      dpi: 72,
      photoNative: false,
      photoOnly: true,
      outputDir: { dirLabel: "out" },
    });
    expect(createInitialAppState("ja").crop.format).toBe("png");
  });
});

describe("opening and replacing a PDF", () => {
  it("shows the first page of the activated PDF", () => {
    const state = open(1);
    expect(state.pdf?.id).toBe(1);
    expect(state.page).toBe(1);
    expect(state.passwordDialogOpen).toBe(false);
  });

  it("opens the password dialog for a PDF that needs a password", () => {
    const state = reduce(createInitialCropState(), {
      type: "CROP_PDF_ACTIVATED",
      pdf: pdf(1, { pageCount: 0, pages: [], error: PASSWORD }),
    });
    expect(state.passwordDialogOpen).toBe(true);
  });

  it("drops the results and the page when another PDF is activated", () => {
    const state = reduce(
      open(1),
      { type: "CROP_PAGE_SELECTED", page: 3 },
      { type: "CROP_PAGE_IMAGES_DONE", id: 1, page: 3, images: [] },
      {
        type: "CROP_LAYOUT_FAILED",
        id: 1,
        page: 3,
        error: { code: "LayoutUnavailable", detail: null },
      },
      { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
    );
    expect(state.pdf?.id).toBe(2);
    expect(state.page).toBe(1);
    expect(state.pageImages).toEqual({});
    expect(state.layouts).toEqual({});
    expect(state.layoutUnavailable).toBe(false);
  });

  it("clears the notice about a file that did not open once a PDF opens", () => {
    const state = reduce(
      createInitialCropState(),
      { type: "CROP_OPEN_NOTICE", notice: { kind: "notOnePdf" } },
      { type: "CROP_PDF_ACTIVATED", pdf: pdf(1) },
    );
    expect(state.openNotice).toBeNull();
  });

  it("keeps what the user chose about the view when another PDF opens", () => {
    const state = reduce(
      open(1),
      { type: "CROP_VIEW_SET", view: { kind: "zoom", percent: 150 } },
      { type: "CROP_CANDIDATES_SHOWN", shown: false },
      { type: "CROP_PDF_ACTIVATED", pdf: pdf(2) },
    );
    expect(state.view).toEqual({ kind: "zoom", percent: 150 });
    expect(state.showCandidates).toBe(false);
  });

  it("replaces the PDF once it is unlocked, closing the dialog", () => {
    const locked = reduce(createInitialCropState(), {
      type: "CROP_PDF_ACTIVATED",
      pdf: pdf(1, { pageCount: 0, pages: [], error: PASSWORD }),
    });
    const state = reduce(locked, {
      type: "CROP_PDF_UNLOCKED",
      pdf: pdf(1),
    });
    expect(state.pdf?.error).toBeNull();
    expect(state.pdf?.pageCount).toBe(5);
    expect(state.passwordDialogOpen).toBe(false);
  });

  it("ignores an unlock for a PDF that is no longer open", () => {
    const state = open(2);
    expect(reduce(state, { type: "CROP_PDF_UNLOCKED", pdf: pdf(1) })).toBe(
      state,
    );
  });

  it("opens and closes the password dialog", () => {
    const state = reduce(open(1), { type: "CROP_PASSWORD_DIALOG", open: true });
    expect(state.passwordDialogOpen).toBe(true);
    expect(
      reduce(state, { type: "CROP_PASSWORD_DIALOG", open: false })
        .passwordDialogOpen,
    ).toBe(false);
  });
});

describe("the page shown", () => {
  it("selects a page of the open PDF", () => {
    expect(reduce(open(), { type: "CROP_PAGE_SELECTED", page: 4 }).page).toBe(
      4,
    );
  });

  it("ignores a page the PDF does not have, and a page when none is open", () => {
    const state = open();
    expect(reduce(state, { type: "CROP_PAGE_SELECTED", page: 0 })).toBe(state);
    expect(reduce(state, { type: "CROP_PAGE_SELECTED", page: 6 })).toBe(state);
    const empty = createInitialCropState();
    expect(reduce(empty, { type: "CROP_PAGE_SELECTED", page: 2 })).toBe(empty);
  });
});

describe("what the pages answered", () => {
  it("keeps pending, done and failed apart, per page", () => {
    const state = reduce(
      open(1),
      { type: "CROP_PAGE_IMAGES_STARTED", id: 1, page: 1 },
      { type: "CROP_PAGE_IMAGES_DONE", id: 1, page: 2, images: [] },
      { type: "CROP_PAGE_IMAGES_FAILED", id: 1, page: 3, error: CRASHED },
      { type: "CROP_LAYOUT_STARTED", id: 1, page: 1 },
      { type: "CROP_LAYOUT_DONE", id: 1, page: 2, candidates: [] },
      { type: "CROP_LAYOUT_FAILED", id: 1, page: 3, error: CRASHED },
    );
    expect(state.pageImages[1]).toEqual({ status: "pending" });
    expect(state.pageImages[2]).toEqual({ status: "done", value: [] });
    expect(state.pageImages[3]).toEqual({ status: "failed", error: CRASHED });
    expect(state.pageImages[4]).toBeUndefined();
    expect(state.layouts[1]).toEqual({ status: "pending" });
    expect(state.layouts[2]).toEqual({ status: "done", value: [] });
    expect(state.layouts[3]).toEqual({ status: "failed", error: CRASHED });
  });

  it("drops an answer for a PDF that is not the open one", () => {
    const state = open(2);
    const answers: CropAction[] = [
      { type: "CROP_PAGE_IMAGES_STARTED", id: 1, page: 1 },
      { type: "CROP_PAGE_IMAGES_DONE", id: 1, page: 1, images: [] },
      { type: "CROP_PAGE_IMAGES_FAILED", id: 1, page: 1, error: CRASHED },
      { type: "CROP_LAYOUT_STARTED", id: 1, page: 1 },
      { type: "CROP_LAYOUT_DONE", id: 1, page: 1, candidates: [] },
      { type: "CROP_LAYOUT_FAILED", id: 1, page: 1, error: CRASHED },
    ];
    for (const answer of answers) {
      expect(reduce(state, answer), answer.type).toBe(state);
    }
  });

  it("drops an answer when no PDF is open", () => {
    const state = createInitialCropState();
    expect(
      reduce(state, {
        type: "CROP_LAYOUT_DONE",
        id: 1,
        page: 1,
        candidates: [],
      }),
    ).toBe(state);
  });

  it("remembers that the layout analysis is unavailable only for that error", () => {
    const crashed = reduce(open(1), {
      type: "CROP_LAYOUT_FAILED",
      id: 1,
      page: 1,
      error: CRASHED,
    });
    expect(crashed.layoutUnavailable).toBe(false);

    const unavailable = reduce(crashed, {
      type: "CROP_LAYOUT_FAILED",
      id: 1,
      page: 2,
      error: { code: "LayoutUnavailable", detail: null },
    });
    expect(unavailable.layoutUnavailable).toBe(true);
    // A later failure of another kind does not undo it.
    expect(
      reduce(unavailable, {
        type: "CROP_LAYOUT_FAILED",
        id: 1,
        page: 3,
        error: CRASHED,
      }).layoutUnavailable,
    ).toBe(true);
  });
});

describe("the view", () => {
  it("sets the size and the candidates", () => {
    const state = reduce(
      open(),
      { type: "CROP_VIEW_SET", view: { kind: "fitWidth" } },
      { type: "CROP_CANDIDATES_SHOWN", shown: false },
    );
    expect(state.view).toEqual({ kind: "fitWidth" });
    expect(state.showCandidates).toBe(false);
  });
});

describe("the export settings", () => {
  it("sets each option", () => {
    const state = reduce(
      createInitialCropState(),
      { type: "SET_CROP_FORMAT", format: "webp" },
      { type: "SET_CROP_COLOR_MODE", colorMode: "gray" },
      { type: "SET_CROP_DPI", dpi: 300 },
      { type: "SET_CROP_PHOTO_NATIVE", photoNative: false },
      { type: "SET_CROP_PHOTO_ONLY", photoOnly: true },
      { type: "SET_CROP_OUTPUT_DIR", outputDir: { dirLabel: "figs" } },
      { type: "SET_CROP_SETTINGS_OPEN", open: false },
    );
    expect(state).toMatchObject({
      format: "webp",
      colorMode: "gray",
      dpi: 300,
      photoNative: false,
      photoOnly: true,
      outputDir: { dirLabel: "figs" },
      settingsOpen: false,
    });
  });

  it("follows a valid custom resolution and keeps the last good one for a bad entry", () => {
    let state = reduce(createInitialCropState(), {
      type: "SET_CROP_IS_CUSTOM_DPI",
      isCustom: true,
    });
    expect(state).toMatchObject({ isCustomDpi: true, customDpiText: "150" });

    state = reduce(state, { type: "SET_CROP_CUSTOM_DPI_TEXT", text: "220" });
    expect(state.dpi).toBe(220);

    state = reduce(state, { type: "SET_CROP_CUSTOM_DPI_TEXT", text: "5000" });
    expect(state).toMatchObject({ dpi: 220, customDpiText: "5000" });

    state = reduce(state, { type: "SET_CROP_DPI", dpi: 72 });
    expect(state).toMatchObject({
      dpi: 72,
      isCustomDpi: false,
      customDpiText: "72",
    });
  });
});
