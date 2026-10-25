import type { Language } from "../i18n";
import type {
  Candidate,
  CandidateKind,
  CheckPageRangeResult,
  CropPdf,
  ImageItem,
  IpcError,
  OutputDirLabel,
  OutputMode,
  PageImage,
  PageOrientationChoice,
  PageSizeChoice,
  PdfItem,
  RenderColorChoice,
  RenderFormatChoice,
} from "../ipc";
import type { JobAction, JobState } from "./job";

export type ActiveTab = "imagesToPdf" | "pdfToImages" | "crop";

export interface LanguageState {
  /** The language the screen is shown in. */
  language: Language;
  /**
   * The language the user picked, saved as is: `null` until they pick one,
   * so the screen keeps following the OS language (design §6.7, FR-07).
   */
  preference: Language | null;
  activeTab: ActiveTab;
}

export type LanguageAction =
  | { type: "SET_LANGUAGE"; language: Language }
  | { type: "SET_ACTIVE_TAB"; tab: ActiveTab };

export interface ImagesToPdfState {
  items: ImageItem[];
  output: OutputMode;
  pageSize: PageSizeChoice;
  orientation: PageOrientationChoice;
  pdfA: boolean;
  /** Whether the PDFs get a password; the password itself is never kept (design §6.9). */
  encrypt: boolean;
  outputDir: OutputDirLabel | null;
}

export type ImagesToPdfAction =
  | { type: "ADD_IMAGE_ITEMS"; items: ImageItem[] }
  | { type: "REMOVE_IMAGE_ITEM"; id: number }
  | { type: "CLEAR_IMAGE_ITEMS" }
  | { type: "SET_IMAGES_OUTPUT_MODE"; output: OutputMode }
  | { type: "SET_IMAGES_PAGE_SIZE"; pageSize: PageSizeChoice }
  | { type: "SET_IMAGES_ORIENTATION"; orientation: PageOrientationChoice }
  | { type: "SET_IMAGES_PDF_A"; pdfA: boolean }
  | { type: "SET_IMAGES_ENCRYPT"; encrypt: boolean }
  | { type: "SET_IMAGES_OUTPUT_DIR"; outputDir: OutputDirLabel | null }
  | { type: "MOVE_IMAGE_ITEM"; fromIndex: number; toIndex: number };

export type PageSelection = "all" | "range";

export interface PdfToImagesState {
  items: PdfItem[];
  pageSelection: PageSelection;
  rangeText: string;
  format: RenderFormatChoice;
  colorMode: RenderColorChoice;
  dpi: number;
  isCustomDpi: boolean;
  customDpiText: string;
  outputDir: OutputDirLabel | null;
  rangeResult: CheckPageRangeResult | null;
  rangeError: IpcError | null;
  rangeChecking: boolean;
}

export type PdfToImagesAction =
  | { type: "ADD_PDF_ITEMS"; items: PdfItem[] }
  | { type: "REPLACE_PDF_ITEMS"; items: PdfItem[] }
  | { type: "REMOVE_PDF_ITEM"; id: number }
  | { type: "CLEAR_PDF_ITEMS" }
  | { type: "SET_PAGE_SELECTION"; selection: PageSelection }
  | { type: "SET_RANGE_TEXT"; rangeText: string }
  | { type: "SET_RENDER_FORMAT"; format: RenderFormatChoice }
  | { type: "SET_RENDER_COLOR_MODE"; colorMode: RenderColorChoice }
  | { type: "SET_RENDER_DPI"; dpi: number }
  | { type: "SET_IS_CUSTOM_DPI"; isCustom: boolean }
  | { type: "SET_CUSTOM_DPI_TEXT"; text: string }
  | { type: "SET_PDFS_OUTPUT_DIR"; outputDir: OutputDirLabel | null }
  | { type: "CHECK_RANGE_STARTED" }
  | { type: "CHECK_RANGE_SUCCESS"; result: CheckPageRangeResult }
  | { type: "CHECK_RANGE_FAILURE"; error: IpcError }
  | { type: "CHECK_RANGE_RESET" };

/**
 * How big the open page is shown (design §10.1): fitted to the width or to
 * the whole page, which follow the size of the view, or a fixed scale where
 * 100 % is the actual size.
 */
export type CropViewSize =
  | { kind: "fitWidth" }
  | { kind: "fitPage" }
  | { kind: "zoom"; percent: number };

/**
 * What the screen has asked of Rust for one page, and what came back. A page
 * that is not in the table has not been asked about yet.
 */
export type CropPageResult<T> =
  | { status: "pending" }
  | { status: "done"; value: T }
  | { status: "failed"; error: IpcError };

/** Why the last attempt to open a PDF opened nothing (design §10.1). */
export type CropOpenNotice =
  { kind: "error"; error: IpcError } | { kind: "notOnePdf" };

/** `[x0, y0, x1, y1]` in points, from the top left of the page as shown (design §4.6). */
export type RectPt = [number, number, number, number];

/** What an area is: a candidate's kind, or `manual` for one picked by hand. */
export type AreaKind = CandidateKind | "manual";

/** An area to write, as the screen has it before it gets its `key`. */
export interface NewArea {
  page: number;
  kind: AreaKind;
  rectPt: RectPt;
  /** The embedded image behind a `photo`; `null` for the other kinds. */
  imageIndex: number | null;
}

/** An area of a page chosen for export (design §6.10). */
export interface CropArea extends NewArea {
  /** Numbers the areas from 1 in the order they were added; Rust reports a result under it. */
  key: number;
  /** Whether "書き出す" writes it. */
  checked: boolean;
  /**
   * Whether something is drawn over a `photo`: `null` until Rust has said
   * (or if it could not), and always `null` for the other kinds.
   */
  onTop: boolean | null;
}

/** What a press and a drag on the page do (design §10.1). */
export type CropTool = "select" | "draw";

/** What a step of the history puts back (design §6.10). */
export interface CropSnapshot {
  areas: CropArea[];
  hiddenCandidates: HiddenCandidates;
}

/** The candidates put aside as wrong, by page: their indices in `layouts`. */
export type HiddenCandidates = Partial<Record<number, number[]>>;

/** What "元に戻す" and "やり直す" go back to (design §6.10). */
export interface CropHistory {
  /** What there was before each step, the latest last. */
  past: CropSnapshot[];
  /** What there was before each undo, the latest undone last. */
  future: CropSnapshot[];
  /**
   * The area the keyboard moves or resizes as one step until another step
   * comes between; `null` when the next change is a step of its own.
   */
  keyedKey: number | null;
}

/** Which conversion of the crop tab started last (design §10.1). */
export type CropJobKind = "export" | "detectAll";

/** What was asked of `check_on_top` for an area that has no answer yet. */
export type OnTopRequest = "pending" | "failed";

/** A pending PDF that waits for the user to say whether to drop the areas (design §6.10). */
export interface CropPendingReplace {
  pdf: CropPdf;
  /** Whether the file dialog or a drop brought it; the focus goes back accordingly. */
  source: "dialog" | "drop";
}

export interface CropState {
  /** The open PDF; its `error` is `PasswordProtected` until it is unlocked. */
  pdf: CropPdf | null;
  /** The page shown, from 1. */
  page: number;
  view: CropViewSize;
  showCandidates: boolean;
  /** The embedded images of the pages asked about, by page. */
  pageImages: Partial<Record<number, CropPageResult<PageImage[]>>>;
  /** The layout candidates of the pages asked about, by page. */
  layouts: Partial<Record<number, CropPageResult<Candidate[]>>>;
  /** The candidates put aside as wrong; they are not drawn nor added (design §10.1). */
  hiddenCandidates: HiddenCandidates;
  /** Set once an analysis fails with `LayoutUnavailable`: Rust answers the same for every page. */
  layoutUnavailable: boolean;
  /** The areas, in the order of their `key`. */
  areas: CropArea[];
  /** The `key` the next area gets. */
  nextAreaKey: number;
  /** The `key` of the area that is selected, if any. */
  selectedKey: number | null;
  /** The `key`s of the areas an export has written since the PDF was opened. */
  exportedKeys: number[];
  /** What was asked of `check_on_top` for the photo areas that have no answer. */
  onTopRequests: Partial<Record<number, OnTopRequest>>;
  /** Which conversion of this tab started last; `null` before any. */
  lastJob: CropJobKind | null;
  /** How many areas the last "全ページの候補" added, for its summary. */
  detectAdded: number;
  /**
   * The pages whose `detect_layout` was still unanswered when "全ページの候補"
   * started: Rust may count them as analyzed and send no `layout-page`, so
   * their candidates are added when that answer comes.
   */
  detectAwaiting: number[];
  /** The PDF waiting for the user to confirm it replaces the areas; `null` when none. */
  pendingReplace: CropPendingReplace | null;
  passwordDialogOpen: boolean;
  openNotice: CropOpenNotice | null;
  format: RenderFormatChoice;
  colorMode: RenderColorChoice;
  dpi: number;
  isCustomDpi: boolean;
  customDpiText: string;
  /** Whether a photo range is written in the pixels of its embedded image. */
  photoNative: boolean;
  /** Whether a photo range leaves out everything but the photo. */
  photoOnly: boolean;
  outputDir: OutputDirLabel | null;
  /** Whether the export settings are open; only the screen keeps it (design §10.1). */
  settingsOpen: boolean;
  history: CropHistory;
  /** Only the screen keeps it; a conversion makes it "select" (design §10.1). */
  tool: CropTool;
}

export type CropAction =
  | { type: "CROP_PDF_ACTIVATED"; pdf: CropPdf }
  | { type: "CROP_PDF_UNLOCKED"; pdf: CropPdf }
  | { type: "CROP_PDF_CLOSED" }
  | { type: "CROP_PASSWORD_DIALOG"; open: boolean }
  | { type: "CROP_OPEN_NOTICE"; notice: CropOpenNotice | null }
  | { type: "CROP_PAGE_SELECTED"; page: number }
  | { type: "CROP_VIEW_SET"; view: CropViewSize }
  | { type: "CROP_CANDIDATES_SHOWN"; shown: boolean }
  | { type: "CROP_PAGE_IMAGES_STARTED"; id: number; page: number }
  | {
      type: "CROP_PAGE_IMAGES_DONE";
      id: number;
      page: number;
      images: PageImage[];
    }
  | {
      type: "CROP_PAGE_IMAGES_FAILED";
      id: number;
      page: number;
      error: IpcError;
    }
  | { type: "CROP_LAYOUT_STARTED"; id: number; page: number }
  | {
      type: "CROP_LAYOUT_DONE";
      id: number;
      page: number;
      candidates: Candidate[];
    }
  | { type: "CROP_LAYOUT_FAILED"; id: number; page: number; error: IpcError }
  | {
      type: "CROP_LAYOUT_PAGE";
      id: number;
      page: number;
      candidates: Candidate[];
      error: IpcError | null;
    }
  /** Adds one area and selects it, or selects the one it overlaps. */
  | { type: "CROP_AREA_ADDED"; area: NewArea }
  /** Adds several areas without selecting any. */
  | { type: "CROP_AREAS_ADDED"; areas: NewArea[] }
  /** Adds an area drawn by hand and selects it; no overlap makes it a repeat. */
  | { type: "CROP_AREA_DRAWN"; page: number; rectPt: RectPt }
  /**
   * Replaces the rectangle of an area that was moved or resized. Changes the
   * keyboard makes to one area in a row undo as one (`byKeyboard`).
   */
  | {
      type: "CROP_AREA_CHANGED";
      key: number;
      rectPt: RectPt;
      byKeyboard?: boolean;
    }
  | { type: "CROP_AREA_SELECTED"; key: number | null }
  | { type: "CROP_AREA_REMOVED"; key: number }
  | { type: "CROP_AREA_CHECKED"; key: number; checked: boolean }
  /** Puts the candidate at `index` of the page's `layouts` aside as wrong. */
  | { type: "CROP_CANDIDATE_HIDDEN"; page: number; index: number }
  /** Brings back every candidate of the page that was put aside. */
  | { type: "CROP_CANDIDATES_RESTORED"; page: number }
  | { type: "CROP_TOOL_SET"; tool: CropTool }
  | { type: "CROP_UNDO" }
  | { type: "CROP_REDO" }
  | { type: "CROP_ON_TOP_STARTED"; id: number; key: number }
  | { type: "CROP_ON_TOP_DONE"; id: number; key: number; onTop: boolean }
  | { type: "CROP_ON_TOP_FAILED"; id: number; key: number }
  | { type: "CROP_EXPORT_STARTED" }
  | { type: "CROP_DETECT_ALL_STARTED" }
  | { type: "CROP_REPLACE_PENDING"; pending: CropPendingReplace | null }
  | { type: "SET_CROP_FORMAT"; format: RenderFormatChoice }
  | { type: "SET_CROP_COLOR_MODE"; colorMode: RenderColorChoice }
  | { type: "SET_CROP_DPI"; dpi: number }
  | { type: "SET_CROP_IS_CUSTOM_DPI"; isCustom: boolean }
  | { type: "SET_CROP_CUSTOM_DPI_TEXT"; text: string }
  | { type: "SET_CROP_PHOTO_NATIVE"; photoNative: boolean }
  | { type: "SET_CROP_PHOTO_ONLY"; photoOnly: boolean }
  | { type: "SET_CROP_OUTPUT_DIR"; outputDir: OutputDirLabel | null }
  | { type: "SET_CROP_SETTINGS_OPEN"; open: boolean };

export interface AppState {
  language: LanguageState;
  imagesToPdf: ImagesToPdfState;
  pdfToImages: PdfToImagesState;
  crop: CropState;
  job: JobState;
}

export type AppAction =
  | LanguageAction
  | ImagesToPdfAction
  | PdfToImagesAction
  | CropAction
  | JobAction;
