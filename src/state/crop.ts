import type { CropPdf, CropSettings } from "../ipc";
import {
  DEFAULT_RENDER_COLOR_MODE,
  DEFAULT_RENDER_DPI,
  DEFAULT_RENDER_FORMAT,
  DPI_CHOICES,
  withCustomDpiMode,
  withCustomDpiText,
  withDpi,
} from "./pdfToImages";
import type { CropAction, CropPageResult, CropState } from "./types";

/**
 * The scales `−`, `＋` and Ctrl+wheel move between, in percent (design
 * §10.1). 100 is the actual size.
 */
export const CROP_ZOOM_STEPS: readonly number[] = [
  25, 33, 50, 67, 75, 90, 100, 125, 150, 200, 300, 400,
];

/**
 * The longest side of a page image `get_thumbnail` is asked for, at most, in
 * pixels. Matches `PAGE_VIEW_MAX_SIDE` in `crates/worker/src/lib.rs`, above
 * which Rust refuses `maxSide`.
 */
export const PAGE_VIEW_MAX_SIDE = 2400;

/**
 * The sizes a page image is asked for. A size between two is rounded up, so
 * zooming does not render the page again at every step (design §10.1).
 */
export const CROP_PAGE_SIDE_STEPS: readonly number[] = [
  400,
  800,
  1200,
  1600,
  2000,
  PAGE_VIEW_MAX_SIDE,
];

/**
 * The window height from which the export settings start open (design §10.1);
 * below it the list of areas would be squeezed out of view.
 */
export const CROP_SETTINGS_OPEN_MIN_HEIGHT = 720;

/** The crop state before anything is open, from the saved crop `settings` if there are any. */
export function createInitialCropState(
  settings: CropSettings | null = null,
): CropState {
  const dpi = settings?.dpi ?? DEFAULT_RENDER_DPI;
  return {
    pdf: null,
    page: 1,
    view: { kind: "fitPage" },
    showCandidates: true,
    pageImages: {},
    layouts: {},
    layoutUnavailable: false,
    passwordDialogOpen: false,
    openNotice: null,
    format: settings?.format ?? DEFAULT_RENDER_FORMAT,
    colorMode: settings?.colorMode ?? DEFAULT_RENDER_COLOR_MODE,
    dpi,
    isCustomDpi: !DPI_CHOICES.includes(dpi),
    customDpiText: String(dpi),
    photoNative: settings?.photoNative ?? true,
    photoOnly: settings?.photoOnly ?? false,
    outputDir: settings?.outputDir ?? null,
    settingsOpen: window.innerHeight >= CROP_SETTINGS_OPEN_MIN_HEIGHT,
  };
}

/** What a PDF's results and page are reset to when another PDF is open. */
function withPdf(state: CropState, pdf: CropPdf | null): CropState {
  return {
    ...state,
    pdf,
    page: 1,
    pageImages: {},
    layouts: {},
    layoutUnavailable: false,
  };
}

function isOpenPdf(state: CropState, id: number): boolean {
  return state.pdf !== null && state.pdf.id === id;
}

function withResult<T>(
  table: Partial<Record<number, CropPageResult<T>>>,
  page: number,
  result: CropPageResult<T>,
): Partial<Record<number, CropPageResult<T>>> {
  return { ...table, [page]: result };
}

export function cropReducer(state: CropState, action: CropAction): CropState {
  switch (action.type) {
    case "CROP_PDF_ACTIVATED":
      return {
        ...withPdf(state, action.pdf),
        passwordDialogOpen: action.pdf.error?.code === "PasswordProtected",
        openNotice: null,
      };
    case "CROP_PDF_UNLOCKED":
      if (!isOpenPdf(state, action.pdf.id)) {
        return state;
      }
      return { ...withPdf(state, action.pdf), passwordDialogOpen: false };
    case "CROP_PDF_CLOSED":
      return { ...withPdf(state, null), passwordDialogOpen: false };
    case "CROP_PASSWORD_DIALOG":
      return state.passwordDialogOpen === action.open
        ? state
        : { ...state, passwordDialogOpen: action.open };
    case "CROP_OPEN_NOTICE":
      return state.openNotice === null && action.notice === null
        ? state
        : { ...state, openNotice: action.notice };
    case "CROP_PAGE_SELECTED":
      if (
        state.pdf === null ||
        action.page === state.page ||
        action.page < 1 ||
        action.page > state.pdf.pageCount
      ) {
        return state;
      }
      return { ...state, page: action.page };
    case "CROP_VIEW_SET":
      return { ...state, view: action.view };
    case "CROP_CANDIDATES_SHOWN":
      return state.showCandidates === action.shown
        ? state
        : { ...state, showCandidates: action.shown };
    // An answer for another PDF than the open one is dropped: the PDF was
    // replaced while Rust was still working on it.
    case "CROP_PAGE_IMAGES_STARTED":
      return isOpenPdf(state, action.id)
        ? {
            ...state,
            pageImages: withResult(state.pageImages, action.page, {
              status: "pending",
            }),
          }
        : state;
    case "CROP_PAGE_IMAGES_DONE":
      return isOpenPdf(state, action.id)
        ? {
            ...state,
            pageImages: withResult(state.pageImages, action.page, {
              status: "done",
              value: action.images,
            }),
          }
        : state;
    case "CROP_PAGE_IMAGES_FAILED":
      return isOpenPdf(state, action.id)
        ? {
            ...state,
            pageImages: withResult(state.pageImages, action.page, {
              status: "failed",
              error: action.error,
            }),
          }
        : state;
    case "CROP_LAYOUT_STARTED":
      return isOpenPdf(state, action.id)
        ? {
            ...state,
            layouts: withResult(state.layouts, action.page, {
              status: "pending",
            }),
          }
        : state;
    case "CROP_LAYOUT_DONE":
      return isOpenPdf(state, action.id)
        ? {
            ...state,
            layouts: withResult(state.layouts, action.page, {
              status: "done",
              value: action.candidates,
            }),
          }
        : state;
    case "CROP_LAYOUT_FAILED":
      return isOpenPdf(state, action.id)
        ? {
            ...state,
            layouts: withResult(state.layouts, action.page, {
              status: "failed",
              error: action.error,
            }),
            layoutUnavailable:
              state.layoutUnavailable ||
              action.error.code === "LayoutUnavailable",
          }
        : state;
    case "SET_CROP_FORMAT":
      return state.format === action.format
        ? state
        : { ...state, format: action.format };
    case "SET_CROP_COLOR_MODE":
      return state.colorMode === action.colorMode
        ? state
        : { ...state, colorMode: action.colorMode };
    case "SET_CROP_DPI":
      return withDpi(state, action.dpi);
    case "SET_CROP_IS_CUSTOM_DPI":
      return withCustomDpiMode(state, action.isCustom);
    case "SET_CROP_CUSTOM_DPI_TEXT":
      return withCustomDpiText(state, action.text);
    case "SET_CROP_PHOTO_NATIVE":
      return state.photoNative === action.photoNative
        ? state
        : { ...state, photoNative: action.photoNative };
    case "SET_CROP_PHOTO_ONLY":
      return state.photoOnly === action.photoOnly
        ? state
        : { ...state, photoOnly: action.photoOnly };
    case "SET_CROP_OUTPUT_DIR":
      return { ...state, outputDir: action.outputDir };
    case "SET_CROP_SETTINGS_OPEN":
      return state.settingsOpen === action.open
        ? state
        : { ...state, settingsOpen: action.open };
  }
}
