import type { PdfItem, RenderColorChoice, RenderFormatChoice } from "../ipc";
import type { PdfToImagesAction, PdfToImagesState } from "./types";

/**
 * Resolution choices matching the presets offered in the resolution dropdown (design §4.4).
 */
export const DPI_CHOICES: readonly number[] = [72, 150, 300];

/**
 * Minimum render resolution in dpi.
 * Matches `RENDER_DPI_MIN` in `crates/worker/src/lib.rs`.
 */
export const RENDER_DPI_MIN = 36;

/**
 * Maximum render resolution in dpi.
 * Matches `RENDER_DPI_MAX` in `crates/worker/src/lib.rs`.
 */
export const RENDER_DPI_MAX = 1000;

/** Default render resolution (150 dpi, design §4.4). */
export const DEFAULT_RENDER_DPI = 150;

/** Default render format choice ("png", design §6.7). */
export const DEFAULT_RENDER_FORMAT: RenderFormatChoice = "png";

/** Default render color mode ("color", design §6.7). */
export const DEFAULT_RENDER_COLOR_MODE: RenderColorChoice = "color";

/**
 * Parses a custom DPI string. Trims leading and trailing ASCII and full-width
 * whitespace, normalizes full-width digits (０-９) to 0-9, and checks that
 * the string contains only digits and represents an integer in [RENDER_DPI_MIN, RENDER_DPI_MAX].
 * Returns the parsed integer if valid, or null otherwise.
 */
export function parseCustomDpi(text: string): number | null {
  const trimmed = text.replace(/^[\s\u3000]+|[\s\u3000]+$/g, "");
  if (trimmed === "") {
    return null;
  }
  const normalized = trimmed.replace(/[\uFF10-\uFF19]/g, (ch) =>
    String.fromCharCode(ch.charCodeAt(0) - 0xfee0),
  );
  if (!/^[0-9]+$/.test(normalized)) {
    return null;
  }
  const val = Number.parseInt(normalized, 10);
  if (
    Number.isSafeInteger(val) &&
    val >= RENDER_DPI_MIN &&
    val <= RENDER_DPI_MAX
  ) {
    return val;
  }
  return null;
}

/**
 * Whether the state has custom resolution selected with an invalid value
 * (empty, malformed, or outside [RENDER_DPI_MIN, RENDER_DPI_MAX]).
 */
export function hasInvalidCustomDpi(
  state: Pick<PdfToImagesState, "isCustomDpi" | "customDpiText">,
): boolean {
  return state.isCustomDpi && parseCustomDpi(state.customDpiText) === null;
}

/** The resolution fields that "PDF to images" and the crop tab share. */
export type DpiFields = Pick<
  PdfToImagesState,
  "dpi" | "isCustomDpi" | "customDpiText"
>;

/** Picks one of the presets (or any `dpi`) and leaves the custom entry. */
export function withDpi<T extends DpiFields>(state: T, dpi: number): T {
  if (state.dpi === dpi && !state.isCustomDpi) {
    return state;
  }
  return { ...state, dpi, isCustomDpi: false, customDpiText: String(dpi) };
}

/** Turns the custom entry on or off, starting it from the resolution in use. */
export function withCustomDpiMode<T extends DpiFields>(
  state: T,
  isCustom: boolean,
): T {
  if (state.isCustomDpi === isCustom) {
    return state;
  }
  return { ...state, isCustomDpi: isCustom, customDpiText: String(state.dpi) };
}

/**
 * Takes the text typed in the custom entry; `dpi` follows it only while the
 * text is valid, so an unfinished entry keeps the last good value.
 */
export function withCustomDpiText<T extends DpiFields>(
  state: T,
  text: string,
): T {
  if (state.customDpiText === text) {
    return state;
  }
  const parsed = parseCustomDpi(text);
  if (parsed !== null) {
    return { ...state, customDpiText: text, dpi: parsed };
  }
  return { ...state, customDpiText: text };
}

export const initialPdfToImagesState: PdfToImagesState = {
  items: [],
  pageSelection: "all",
  rangeText: "",
  format: DEFAULT_RENDER_FORMAT,
  colorMode: DEFAULT_RENDER_COLOR_MODE,
  dpi: DEFAULT_RENDER_DPI,
  isCustomDpi: false,
  customDpiText: String(DEFAULT_RENDER_DPI),
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
    case "REPLACE_PDF_ITEMS": {
      if (action.items.length === 0 || state.items.length === 0) {
        return state;
      }
      const replacements = new Map<number, PdfItem>(
        action.items.map((item) => [item.id, item]),
      );
      let changed = false;
      const nextItems = state.items.map((item) => {
        const replacement = replacements.get(item.id);
        if (replacement !== undefined && replacement !== item) {
          changed = true;
          return replacement;
        }
        return item;
      });
      if (!changed) {
        return state;
      }
      return {
        ...state,
        items: nextItems,
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
    case "SET_RENDER_COLOR_MODE": {
      if (state.colorMode === action.colorMode) {
        return state;
      }
      return {
        ...state,
        colorMode: action.colorMode,
      };
    }
    case "SET_RENDER_DPI":
      return withDpi(state, action.dpi);
    case "SET_IS_CUSTOM_DPI":
      return withCustomDpiMode(state, action.isCustom);
    case "SET_CUSTOM_DPI_TEXT":
      return withCustomDpiText(state, action.text);
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
