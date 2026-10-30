import { createInitialCropState, cropReducer } from "./crop";
import { createInitialLanguageState, languageReducer } from "./language";
import { imagesToPdfReducer, initialImagesToPdfState } from "./imagesToPdf";
import { initialJobState, isJobActive, jobReducer } from "./job";
import {
  DPI_CHOICES,
  pdfToImagesReducer,
  initialPdfToImagesState,
} from "./pdfToImages";
import type { Settings } from "../ipc";
import type { AppAction, AppState, CropAction } from "./types";

/**
 * The state the app starts in: the saved `settings` (design §6.7), or the
 * defaults when there are none. Lists and page ranges always start empty.
 */
export function createInitialAppState(
  initialNavLang?: string,
  settings: Settings | null = null,
): AppState {
  // Copies, so a caller that edits the state it gets (as tests do) cannot
  // change the shared initial states.
  if (settings === null) {
    return {
      language: createInitialLanguageState(initialNavLang),
      imagesToPdf: { ...initialImagesToPdfState },
      pdfToImages: { ...initialPdfToImagesState },
      crop: createInitialCropState(),
      job: { ...initialJobState },
    };
  }
  const dpi = settings.pdfToImages.dpi;
  const isCustomDpi = !DPI_CHOICES.includes(dpi);
  return {
    language: createInitialLanguageState(initialNavLang, settings.language),
    imagesToPdf: {
      ...initialImagesToPdfState,
      output: settings.imagesToPdf.output,
      pageSize: settings.imagesToPdf.pageSize,
      orientation: settings.imagesToPdf.orientation,
      pdfA: settings.imagesToPdf.pdfA,
      encrypt: settings.imagesToPdf.encrypt,
      outputDir: settings.imagesToPdf.outputDir,
    },
    pdfToImages: {
      ...initialPdfToImagesState,
      format: settings.pdfToImages.format,
      colorMode: settings.pdfToImages.colorMode,
      dpi,
      isCustomDpi,
      customDpiText: String(dpi),
      outputDir: settings.pdfToImages.outputDir,
    },
    crop: createInitialCropState(settings.crop),
    job: { ...initialJobState },
  };
}

/**
 * Drops the last conversion's result once the list or the settings of the
 * screen it ran on change: the counts, row statuses and failed pages then
 * describe items and settings that are no longer there, and the footer
 * goes back to saying what the next start would do (design §10.1). A
 * running job has neither a result nor an error yet, so it is never dropped.
 */
function dropStaleResult(
  state: AppState,
  next: AppState,
  tab: AppState["job"]["kind"],
): AppState {
  if (
    next === state ||
    state.job.kind !== tab ||
    (state.job.finished === null && state.job.error === null)
  ) {
    return next;
  }
  return { ...next, job: initialJobState };
}

/**
 * Runs a crop action. `dropsResult` is for the actions that change what the
 * last crop result describes (design §10.1).
 */
function reduceCrop(
  state: AppState,
  action: CropAction,
  dropsResult: boolean,
): AppState {
  const crop = cropReducer(state.crop, action);
  if (crop === state.crop) {
    return state;
  }
  const next = { ...state, crop };
  return dropsResult ? dropStaleResult(state, next, "crop") : next;
}

/**
 * An export's item that succeeded is exported from then on, which only the
 * crop state remembers; the job state knows nothing of areas.
 */
function withExportedItem(state: AppState, id: number): AppState {
  const { crop } = state;
  if (
    state.job.kind !== "crop" ||
    crop.lastJob !== "export" ||
    crop.exportedKeys.includes(id)
  ) {
    return state;
  }
  return {
    ...state,
    crop: { ...crop, exportedKeys: [...crop.exportedKeys, id] },
  };
}

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "SET_LANGUAGE":
    case "SET_ACTIVE_TAB":
      return {
        ...state,
        language: languageReducer(state.language, action),
      };
    case "ADD_IMAGE_ITEMS":
    case "REMOVE_IMAGE_ITEM":
    case "CLEAR_IMAGE_ITEMS":
    case "SET_IMAGES_OUTPUT_MODE":
    case "SET_IMAGES_PAGE_SIZE":
    case "SET_IMAGES_ORIENTATION":
    case "SET_IMAGES_PDF_FORMAT":
    case "SET_IMAGES_OUTPUT_DIR":
    case "MOVE_IMAGE_ITEM": {
      const imagesToPdf = imagesToPdfReducer(state.imagesToPdf, action);
      return imagesToPdf === state.imagesToPdf
        ? state
        : dropStaleResult(state, { ...state, imagesToPdf }, "imagesToPdf");
    }
    case "ADD_PDF_ITEMS":
    case "REPLACE_PDF_ITEMS":
    case "REMOVE_PDF_ITEM":
    case "CLEAR_PDF_ITEMS":
    case "SET_PAGE_SELECTION":
    case "SET_RANGE_TEXT":
    case "SET_RENDER_FORMAT":
    case "SET_RENDER_COLOR_MODE":
    case "SET_RENDER_DPI":
    case "SET_IS_CUSTOM_DPI":
    case "SET_CUSTOM_DPI_TEXT":
    case "SET_PDFS_OUTPUT_DIR": {
      const pdfToImages = pdfToImagesReducer(state.pdfToImages, action);
      return pdfToImages === state.pdfToImages
        ? state
        : dropStaleResult(state, { ...state, pdfToImages }, "pdfToImages");
    }
    // The areas are frozen while a conversion runs, so the result it ends
    // with describes the areas that were there (design §6.10). What
    // "全ページの候補" adds and selecting an area are not edits of that kind.
    case "CROP_AREA_ADDED":
    case "CROP_AREAS_ADDED":
    case "CROP_AREA_DRAWN":
    case "CROP_AREA_CHANGED":
    case "CROP_AREA_REMOVED":
    case "CROP_AREA_CHECKED":
    case "CROP_CANDIDATE_HIDDEN":
    case "CROP_CANDIDATES_RESTORED":
    case "CROP_UNDO":
    case "CROP_REDO":
      return isJobActive(state.job) ? state : reduceCrop(state, action, true);
    case "CROP_PDF_ACTIVATED":
    case "SET_CROP_FORMAT":
    case "SET_CROP_COLOR_MODE":
    case "SET_CROP_DPI":
    case "SET_CROP_IS_CUSTOM_DPI":
    case "SET_CROP_CUSTOM_DPI_TEXT":
    case "SET_CROP_PHOTO_NATIVE":
    case "SET_CROP_PHOTO_ONLY":
    case "SET_CROP_OUTPUT_DIR":
      return reduceCrop(state, action, true);
    case "CROP_PDF_UNLOCKED":
    case "CROP_PASSWORD_DIALOG":
    case "CROP_OPEN_NOTICE":
    case "CROP_PAGE_SELECTED":
    case "CROP_VIEW_SET":
    case "CROP_CANDIDATES_SHOWN":
    case "CROP_PAGE_IMAGES_STARTED":
    case "CROP_PAGE_IMAGES_DONE":
    case "CROP_PAGE_IMAGES_FAILED":
    case "CROP_LAYOUT_STARTED":
    case "CROP_LAYOUT_DONE":
    case "CROP_LAYOUT_FAILED":
    case "CROP_LAYOUT_PAGE":
    case "CROP_AREA_SELECTED":
    case "CROP_ON_TOP_STARTED":
    case "CROP_ON_TOP_DONE":
    case "CROP_ON_TOP_FAILED":
    case "CROP_EXPORT_STARTED":
    case "CROP_DETECT_ALL_STARTED":
    case "CROP_REPLACE_PENDING":
    case "SET_CROP_SETTINGS_OPEN":
    case "CROP_TOOL_SET":
      return reduceCrop(state, action, false);
    // The answers to a range check follow the user's edit; the edit itself
    // already dropped the result.
    case "CHECK_RANGE_STARTED":
    case "CHECK_RANGE_SUCCESS":
    case "CHECK_RANGE_FAILURE":
    case "CHECK_RANGE_RESET":
      return {
        ...state,
        pdfToImages: pdfToImagesReducer(state.pdfToImages, action),
      };
    case "JOB_ITEM": {
      const job = jobReducer(state.job, action);
      if (job === state.job) {
        return state;
      }
      const next = { ...state, job };
      return action.item.status === "ok"
        ? withExportedItem(next, action.item.id)
        : next;
    }
    // A conversion freezes the areas, so drawing one has nothing left to do;
    // it does not come back when the conversion ends (design §10.1).
    case "JOB_STARTED":
      return {
        ...state,
        job: jobReducer(state.job, action),
        crop:
          state.crop.tool === "draw"
            ? { ...state.crop, tool: "select" }
            : state.crop,
      };
    case "JOB_CANCEL_REQUESTED":
    case "JOB_PROGRESS":
    case "JOB_FINISHED":
    case "JOB_SAVED":
    case "JOB_FAILED":
    case "JOB_NOT_STARTED":
    case "JOB_RESET":
      return {
        ...state,
        job: jobReducer(state.job, action),
      };
  }
}
