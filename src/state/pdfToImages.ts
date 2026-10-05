import type { PdfToImagesAction, PdfToImagesState } from "./types";

export const initialPdfToImagesState: PdfToImagesState = {
  items: [],
};

export function pdfToImagesReducer(
  state: PdfToImagesState,
  action: PdfToImagesAction,
): PdfToImagesState {
  switch (action.type) {
    case "ADD_PDF_ITEMS": {
      if (action.items.length === 0) {
        return state;
      }
      return {
        ...state,
        items: [...state.items, ...action.items],
      };
    }
    case "REMOVE_PDF_ITEM": {
      const nextItems = state.items.filter((item) => item.id !== action.id);
      if (nextItems.length === state.items.length) {
        return state;
      }
      return {
        ...state,
        items: nextItems,
      };
    }
    case "CLEAR_PDF_ITEMS": {
      if (state.items.length === 0) {
        return state;
      }
      return {
        ...state,
        items: [],
      };
    }
  }
}
