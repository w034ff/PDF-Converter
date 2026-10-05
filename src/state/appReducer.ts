import { createInitialLanguageState, languageReducer } from "./language";
import { imagesToPdfReducer, initialImagesToPdfState } from "./imagesToPdf";
import { pdfToImagesReducer, initialPdfToImagesState } from "./pdfToImages";
import type { AppAction, AppState } from "./types";

export function createInitialAppState(initialNavLang?: string): AppState {
  return {
    language: createInitialLanguageState(initialNavLang),
    imagesToPdf: initialImagesToPdfState,
    pdfToImages: initialPdfToImagesState,
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
      return {
        ...state,
        pdfToImages: pdfToImagesReducer(state.pdfToImages, action),
      };
  }
}
