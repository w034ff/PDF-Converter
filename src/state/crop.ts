import type { Candidate, CropPdf, CropSettings } from "../ipc";
import {
  DEFAULT_RENDER_COLOR_MODE,
  DEFAULT_RENDER_DPI,
  DEFAULT_RENDER_FORMAT,
  DPI_CHOICES,
  withCustomDpiMode,
  withCustomDpiText,
  withDpi,
} from "./pdfToImages";
import type {
  CropAction,
  CropArea,
  CropPageResult,
  CropState,
  NewArea,
  RectPt,
} from "./types";

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
 * The most pixels one rendered range may have. Matches
 * `pdfconv_worker::MAX_RENDER_PIXELS` in `crates/worker/src/lib.rs`, above
 * which a rectangle is refused and a photo is scaled down.
 */
export const MAX_RENDER_PIXELS = 100_000_000;

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
 * From this IoU, two areas of a page count as the same one and the second is
 * not added (design §6.10).
 */
export const CROP_DUPLICATE_IOU = 0.9;

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
    areas: [],
    nextAreaKey: 1,
    selectedKey: null,
    exportedKeys: [],
    onTopRequests: {},
    lastJob: null,
    detectAdded: 0,
    detectAwaiting: [],
    pendingReplace: null,
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
    settingsOpen: false,
  };
}

/** What a PDF's results, areas and page are reset to when another PDF is open. */
function withPdf(state: CropState, pdf: CropPdf | null): CropState {
  return {
    ...state,
    pdf,
    page: 1,
    pageImages: {},
    layouts: {},
    layoutUnavailable: false,
    areas: [],
    nextAreaKey: 1,
    selectedKey: null,
    exportedKeys: [],
    onTopRequests: {},
    detectAwaiting: [],
  };
}

function rectArea(rect: RectPt): number {
  return Math.max(rect[2] - rect[0], 0) * Math.max(rect[3] - rect[1], 0);
}

/**
 * Intersection over union of two rectangles, as `iou` in
 * `crates/core/src/region.rs`: 0 when they share nothing and when both are
 * empty.
 */
export function rectIou(a: RectPt, b: RectPt): number {
  const width = Math.min(a[2], b[2]) - Math.max(a[0], b[0]);
  const height = Math.min(a[3], b[3]) - Math.max(a[1], b[1]);
  const shared = Math.max(width, 0) * Math.max(height, 0);
  const union = rectArea(a) + rectArea(b) - shared;
  return union > 0 ? shared / union : 0;
}

/** The area on the page of `area` that it repeats, if there is one. */
export function findDuplicate(
  areas: readonly CropArea[],
  area: NewArea,
): CropArea | undefined {
  return areas.find(
    (other) =>
      other.page === area.page &&
      rectIou(other.rectPt, area.rectPt) >= CROP_DUPLICATE_IOU,
  );
}

/** `news` appended in order, but for those that repeat an area already there. */
function appendAreas(
  state: CropState,
  news: readonly NewArea[],
): { state: CropState; added: number } {
  const areas = [...state.areas];
  let nextAreaKey = state.nextAreaKey;
  for (const area of news) {
    if (findDuplicate(areas, area) === undefined) {
      areas.push({ ...area, key: nextAreaKey, checked: true, onTop: null });
      nextAreaKey += 1;
    }
  }
  const added = areas.length - state.areas.length;
  return added === 0
    ? { state, added }
    : { state: { ...state, areas, nextAreaKey }, added };
}

/** The areas the candidates of `page` make, in their order. */
export function candidateAreas(
  page: number,
  candidates: readonly Candidate[],
): NewArea[] {
  return candidates.map((candidate) => ({
    page,
    kind: candidate.kind,
    rectPt: candidate.rectPt,
    imageIndex: candidate.kind === "photo" ? candidate.imageIndex : null,
  }));
}

/** Adds a page's candidates, and counts how many were new for the summary. */
function addDetected(
  state: CropState,
  page: number,
  candidates: readonly Candidate[],
): CropState {
  const { state: next, added } = appendAreas(
    state,
    candidateAreas(page, candidates),
  );
  return added === 0
    ? state
    : { ...next, detectAdded: next.detectAdded + added };
}

function withoutPage(pages: readonly number[], page: number): number[] {
  return pages.filter((other) => other !== page);
}

/**
 * Handles the candidates a page's analysis brought, if "全ページの候補" was
 * waiting for them, and stops waiting for that page.
 */
function takeAwaited(
  state: CropState,
  page: number,
  candidates: readonly Candidate[] | null,
): CropState {
  if (!state.detectAwaiting.includes(page)) {
    return state;
  }
  const next = {
    ...state,
    detectAwaiting: withoutPage(state.detectAwaiting, page),
  };
  return candidates === null ? next : addDetected(next, page, candidates);
}

function pagesWith(
  layouts: CropState["layouts"],
  status: "pending" | "done",
): number[] {
  return Object.entries(layouts)
    .filter(([, result]) => result?.status === status)
    .map(([page]) => Number(page))
    .sort((a, b) => a - b);
}

function withArea(
  state: CropState,
  key: number,
  change: (area: CropArea) => CropArea,
): CropState {
  return state.areas.some((area) => area.key === key)
    ? {
        ...state,
        areas: state.areas.map((area) =>
          area.key === key ? change(area) : area,
        ),
      }
    : state;
}

function sameRect(a: RectPt, b: RectPt): boolean {
  return a.every((value, index) => value === b[index]);
}

/**
 * Moves or resizes an area. A photo no longer lies on the pixel grid of its
 * embedded image, so it becomes a hand-picked area and forgets the image and
 * what was drawn over it; what an export wrote no longer matches the area, so
 * it counts as not exported (design §6.10).
 */
function changeArea(state: CropState, key: number, rectPt: RectPt): CropState {
  const area = state.areas.find((other) => other.key === key);
  if (area === undefined || sameRect(area.rectPt, rectPt)) {
    return state;
  }
  const onTopRequests = { ...state.onTopRequests };
  delete onTopRequests[key];
  return {
    ...state,
    areas: state.areas.map((other) =>
      other.key === key
        ? other.kind === "photo"
          ? {
              ...other,
              kind: "manual",
              rectPt,
              imageIndex: null,
              onTop: null,
            }
          : { ...other, rectPt }
        : other,
    ),
    exportedKeys: state.exportedKeys.filter((other) => other !== key),
    onTopRequests,
  };
}

function isOpenPdf(state: CropState, id: number): boolean {
  return state.pdf !== null && state.pdf.id === id;
}

/** How many areas were never exported, the unchecked ones among them (design §6.10). */
export function unexportedAreaCount(state: CropState): number {
  return state.areas.filter((area) => !state.exportedKeys.includes(area.key))
    .length;
}

function hasArea(state: CropState, key: number): boolean {
  return state.areas.some((area) => area.key === key);
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
        pendingReplace: null,
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
        ? takeAwaited(
            {
              ...state,
              layouts: withResult(state.layouts, action.page, {
                status: "done",
                value: action.candidates,
              }),
            },
            action.page,
            action.candidates,
          )
        : state;
    case "CROP_LAYOUT_FAILED":
      return isOpenPdf(state, action.id)
        ? takeAwaited(
            {
              ...state,
              layouts: withResult(state.layouts, action.page, {
                status: "failed",
                error: action.error,
              }),
              layoutUnavailable:
                state.layoutUnavailable ||
                action.error.code === "LayoutUnavailable",
            },
            action.page,
            null,
          )
        : state;
    // The answer of "全ページの候補" for one page: the page's analysis result
    // too, so showing the page asks Rust nothing.
    case "CROP_LAYOUT_PAGE":
      if (!isOpenPdf(state, action.id)) {
        return state;
      }
      return action.error === null
        ? takeAwaited(
            addDetected(
              {
                ...state,
                layouts: withResult(state.layouts, action.page, {
                  status: "done",
                  value: action.candidates,
                }),
              },
              action.page,
              action.candidates,
            ),
            action.page,
            null,
          )
        : takeAwaited(
            {
              ...state,
              layouts: withResult(state.layouts, action.page, {
                status: "failed",
                error: action.error,
              }),
              layoutUnavailable:
                state.layoutUnavailable ||
                action.error.code === "LayoutUnavailable",
            },
            action.page,
            null,
          );
    case "CROP_AREA_ADDED": {
      const duplicate = findDuplicate(state.areas, action.area);
      if (duplicate !== undefined) {
        return state.selectedKey === duplicate.key
          ? state
          : { ...state, selectedKey: duplicate.key };
      }
      const { state: next } = appendAreas(state, [action.area]);
      return { ...next, selectedKey: next.nextAreaKey - 1 };
    }
    case "CROP_AREAS_ADDED":
      return appendAreas(state, action.areas).state;
    // The user put the area there, so no overlap makes it a repeat.
    case "CROP_AREA_DRAWN": {
      const key = state.nextAreaKey;
      return {
        ...state,
        areas: [
          ...state.areas,
          {
            key,
            page: action.page,
            kind: "manual",
            rectPt: action.rectPt,
            imageIndex: null,
            checked: true,
            onTop: null,
          },
        ],
        nextAreaKey: key + 1,
        selectedKey: key,
      };
    }
    case "CROP_AREA_CHANGED":
      return changeArea(state, action.key, action.rectPt);
    case "CROP_AREA_SELECTED":
      if (
        state.selectedKey === action.key ||
        (action.key !== null &&
          !state.areas.some((area) => area.key === action.key))
      ) {
        return state;
      }
      return { ...state, selectedKey: action.key };
    case "CROP_AREA_REMOVED": {
      if (!state.areas.some((area) => area.key === action.key)) {
        return state;
      }
      const onTopRequests = { ...state.onTopRequests };
      delete onTopRequests[action.key];
      return {
        ...state,
        areas: state.areas.filter((area) => area.key !== action.key),
        selectedKey:
          state.selectedKey === action.key ? null : state.selectedKey,
        exportedKeys: state.exportedKeys.filter((key) => key !== action.key),
        onTopRequests,
      };
    }
    case "CROP_AREA_CHECKED":
      return withArea(state, action.key, (area) =>
        area.checked === action.checked
          ? area
          : { ...area, checked: action.checked },
      );
    case "CROP_ON_TOP_STARTED":
      return isOpenPdf(state, action.id) && hasArea(state, action.key)
        ? {
            ...state,
            onTopRequests: { ...state.onTopRequests, [action.key]: "pending" },
          }
        : state;
    case "CROP_ON_TOP_DONE": {
      if (!isOpenPdf(state, action.id) || !hasArea(state, action.key)) {
        return state;
      }
      const onTopRequests = { ...state.onTopRequests };
      delete onTopRequests[action.key];
      return {
        ...withArea(state, action.key, (area) => ({
          ...area,
          onTop: action.onTop,
        })),
        onTopRequests,
      };
    }
    case "CROP_ON_TOP_FAILED":
      return isOpenPdf(state, action.id) && hasArea(state, action.key)
        ? {
            ...state,
            onTopRequests: { ...state.onTopRequests, [action.key]: "failed" },
          }
        : state;
    case "CROP_EXPORT_STARTED":
      return state.lastJob === "export"
        ? state
        : { ...state, lastJob: "export" };
    case "CROP_DETECT_ALL_STARTED": {
      const started: CropState = {
        ...state,
        lastJob: "detectAll",
        detectAdded: 0,
        detectAwaiting: pagesWith(state.layouts, "pending"),
      };
      return pagesWith(state.layouts, "done").reduce((next, page) => {
        const result = next.layouts[page];
        return result?.status === "done"
          ? addDetected(next, page, result.value)
          : next;
      }, started);
    }
    case "CROP_REPLACE_PENDING":
      return state.pendingReplace === null && action.pending === null
        ? state
        : { ...state, pendingReplace: action.pending };
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
