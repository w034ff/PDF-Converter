import { createInitialLanguageState, languageReducer } from "./language";
import { imagesToPdfReducer, initialImagesToPdfState } from "./imagesToPdf";
import { initialJobState, jobReducer } from "./job";
import { pdfToImagesReducer, initialPdfToImagesState } from "./pdfToImages";
import type { AppAction, AppState } from "./types";

export function createInitialAppState(initialNavLang?: string): AppState {
  return {
    language: createInitialLanguageState(initialNavLang),
    imagesToPdf: initialImagesToPdfState,
    pdfToImages: initialPdfToImagesState,
    job: initialJobState,
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
    case "SET_OUTPUT_DIR":
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
    case "JOB_RESET":
      return {
        ...state,
        job: jobReducer(state.job, action),
      };
  }
}
