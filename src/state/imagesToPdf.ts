import type { ImagesToPdfAction, ImagesToPdfState } from "./types";

export const initialImagesToPdfState: ImagesToPdfState = {
  items: [],
  output: "merge",
  pageSize: "fit",
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
