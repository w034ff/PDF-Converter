import type { Language } from "../i18n";
import type {
  Candidate,
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
  /** Set once an analysis fails with `LayoutUnavailable`: Rust answers the same for every page. */
  layoutUnavailable: boolean;
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
