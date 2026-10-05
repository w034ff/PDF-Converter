import type { RenderFormatChoice } from "../ipc";
import type { PdfToImagesAction, PdfToImagesState } from "./types";

/**
 * Resolution choices matching Rust `DPI_CHOICES` in `crates/worker/src/lib.rs`.
 */
export const DPI_CHOICES: readonly number[] = [72, 150, 300];

/** Default render resolution (150 dpi, design §4.4). */
export const DEFAULT_RENDER_DPI = 150;

/** Default render format choice ("png", design §6.7). */
export const DEFAULT_RENDER_FORMAT: RenderFormatChoice = "png";

export const initialPdfToImagesState: PdfToImagesState = {
  items: [],
  pageSelection: "all",
  rangeText: "",
  format: DEFAULT_RENDER_FORMAT,
  dpi: DEFAULT_RENDER_DPI,
  outputDir: null,
  rangeResult: null,
  rangeError: null,
  rangeChecking: false,
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
    case "SET_PAGE_SELECTION": {
      if (state.pageSelection === action.selection) {
        return state;
      }
      return {
        ...state,
        pageSelection: action.selection,
      };
    }
    case "SET_RANGE_TEXT": {
      if (state.rangeText === action.rangeText) {
        return state;
      }
      return {
        ...state,
        rangeText: action.rangeText,
      };
    }
    case "SET_RENDER_FORMAT": {
      if (state.format === action.format) {
        return state;
      }
      return {
        ...state,
        format: action.format,
      };
    }
    case "SET_RENDER_DPI": {
      if (state.dpi === action.dpi) {
        return state;
      }
      return {
        ...state,
        dpi: action.dpi,
      };
    }
    case "SET_PDFS_OUTPUT_DIR": {
      return {
        ...state,
        outputDir: action.outputDir,
      };
    }
    case "CHECK_RANGE_STARTED": {
      return {
        ...state,
        rangeChecking: true,
      };
    }
    case "CHECK_RANGE_SUCCESS": {
      return {
        ...state,
        rangeChecking: false,
        rangeResult: action.result,
        rangeError: null,
      };
    }
    case "CHECK_RANGE_FAILURE": {
      return {
        ...state,
        rangeChecking: false,
        rangeResult: null,
        rangeError: action.error,
      };
    }
    case "CHECK_RANGE_RESET": {
      return {
        ...state,
        rangeChecking: false,
        rangeResult: null,
        rangeError: null,
      };
    }
  }
}
