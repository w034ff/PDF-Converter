import type { PageSizeChoice } from "../ipc";
import type {
  ImagesPdfFormat,
  ImagesToPdfAction,
  ImagesToPdfState,
} from "./types";

export interface PaperSizeOption {
  readonly value: Exclude<PageSizeChoice, "fit">;
  readonly label: string;
}

export const PAPER_SIZE_OPTIONS = [
  { value: "a3", label: "A3 (297 × 420 mm)" },
  { value: "a4", label: "A4 (210 × 297 mm)" },
  { value: "a5", label: "A5 (148 × 210 mm)" },
  { value: "jisB4", label: "B4 (257 × 364 mm)" },
  { value: "jisB5", label: "B5 (182 × 257 mm)" },
  { value: "letter", label: "Letter (8.5 × 11 in)" },
  { value: "legal", label: "Legal (8.5 × 14 in)" },
] satisfies readonly PaperSizeOption[];

export function isPaperSize(
  value: unknown,
): value is Exclude<PageSizeChoice, "fit"> {
  return PAPER_SIZE_OPTIONS.some((option) => option.value === value);
}

export function isPageSizeChoice(value: unknown): value is PageSizeChoice {
  return value === "fit" || isPaperSize(value);
}

export function imagesPdfFormat(
  state: Pick<ImagesToPdfState, "pdfA" | "encrypt">,
): ImagesPdfFormat {
  if (state.pdfA) {
    return "pdfA";
  }
  return state.encrypt ? "encrypt" : "standard";
}

export const initialImagesToPdfState: ImagesToPdfState = {
  items: [],
  output: "merge",
  pageSize: "fit",
  orientation: "auto",
  pdfA: false,
  encrypt: false,
  outputDir: null,
};

export function imagesToPdfReducer(
  state: ImagesToPdfState,
  action: ImagesToPdfAction,
): ImagesToPdfState {
  switch (action.type) {
    case "ADD_IMAGE_ITEMS": {
      if (action.items.length === 0) {
        return state;
      }
      return {
        ...state,
        items: [...state.items, ...action.items],
      };
    }
    case "REMOVE_IMAGE_ITEM": {
      const nextItems = state.items.filter((item) => item.id !== action.id);
      if (nextItems.length === state.items.length) {
        return state;
      }
      return {
        ...state,
        items: nextItems,
      };
    }
    case "CLEAR_IMAGE_ITEMS": {
      if (state.items.length === 0) {
        return state;
      }
      return {
        ...state,
        items: [],
      };
    }
    case "SET_IMAGES_OUTPUT_MODE": {
      if (state.output === action.output) {
        return state;
      }
      return {
        ...state,
        output: action.output,
      };
    }
    case "SET_IMAGES_PAGE_SIZE": {
      if (state.pageSize === action.pageSize) {
        return state;
      }
      return {
        ...state,
        pageSize: action.pageSize,
      };
    }
    case "SET_IMAGES_ORIENTATION": {
      if (state.orientation === action.orientation) {
        return state;
      }
      return {
        ...state,
        orientation: action.orientation,
      };
    }
    case "SET_IMAGES_PDF_FORMAT": {
      if (imagesPdfFormat(state) === action.format) {
        return state;
      }
      return {
        ...state,
        pdfA: action.format === "pdfA",
        encrypt: action.format === "encrypt",
      };
    }
    case "SET_IMAGES_OUTPUT_DIR": {
      return {
        ...state,
        outputDir: action.outputDir,
      };
    }
    case "MOVE_IMAGE_ITEM": {
      const { fromIndex, toIndex } = action;
      if (
        fromIndex < 0 ||
        fromIndex >= state.items.length ||
        toIndex < 0 ||
        toIndex >= state.items.length ||
        fromIndex === toIndex
      ) {
        return state;
      }
      const nextItems = [...state.items];
      const [moved] = nextItems.splice(fromIndex, 1);
      if (!moved) {
        return state;
      }
      nextItems.splice(toIndex, 0, moved);
      return {
        ...state,
        items: nextItems,
      };
    }
  }
}
