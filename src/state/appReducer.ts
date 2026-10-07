import { createInitialLanguageState, languageReducer } from "./language";
import { imagesToPdfReducer, initialImagesToPdfState } from "./imagesToPdf";
import { initialJobState, jobReducer } from "./job";
import { pdfToImagesReducer, initialPdfToImagesState } from "./pdfToImages";
import type { Settings } from "../ipc";
import type { AppAction, AppState } from "./types";

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
      job: { ...initialJobState },
    };
  }
  return {
    language: createInitialLanguageState(initialNavLang, settings.language),
    imagesToPdf: {
      ...initialImagesToPdfState,
      output: settings.imagesToPdf.output,
      pageSize: settings.imagesToPdf.pageSize,
      outputDir: settings.imagesToPdf.outputDir,
    },
    pdfToImages: {
      ...initialPdfToImagesState,
      format: settings.pdfToImages.format,
      dpi: settings.pdfToImages.dpi,
      outputDir: settings.pdfToImages.outputDir,
    },
    job: { ...initialJobState },
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
    case "SET_IMAGES_OUTPUT_DIR":
    case "MOVE_IMAGE_ITEM":
      return {
        ...state,
        imagesToPdf: imagesToPdfReducer(state.imagesToPdf, action),
      };
    case "ADD_PDF_ITEMS":
    case "REMOVE_PDF_ITEM":
    case "CLEAR_PDF_ITEMS":
    case "SET_PAGE_SELECTION":
    case "SET_RANGE_TEXT":
    case "SET_RENDER_FORMAT":
    case "SET_RENDER_DPI":
    case "SET_PDFS_OUTPUT_DIR":
    case "CHECK_RANGE_STARTED":
    case "CHECK_RANGE_SUCCESS":
    case "CHECK_RANGE_FAILURE":
    case "CHECK_RANGE_RESET":
      return {
        ...state,
        pdfToImages: pdfToImagesReducer(state.pdfToImages, action),
      };
    case "JOB_STARTED":
    case "JOB_CANCEL_REQUESTED":
    case "JOB_PROGRESS":
    case "JOB_ITEM":
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
