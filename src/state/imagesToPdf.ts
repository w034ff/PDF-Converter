import type { ImagesToPdfAction, ImagesToPdfState } from "./types";

export const initialImagesToPdfState: ImagesToPdfState = {
  items: [],
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
  }
}
