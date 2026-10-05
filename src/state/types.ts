import type { Language } from "../i18n";
import type {
  CheckPageRangeResult,
  ImageItem,
  IpcError,
  OutputDirLabel,
  PdfItem,
  RenderFormatChoice,
} from "../ipc";
import type { JobAction, JobState } from "./job";

export type ActiveTab = "imagesToPdf" | "pdfToImages";

export interface LanguageState {
  language: Language;
  activeTab: ActiveTab;
}

export type LanguageAction =
  | { type: "SET_LANGUAGE"; language: Language }
  | { type: "SET_ACTIVE_TAB"; tab: ActiveTab };

export interface ImagesToPdfState {
  items: ImageItem[];
}

export type ImagesToPdfAction =
  | { type: "ADD_IMAGE_ITEMS"; items: ImageItem[] }
  | { type: "REMOVE_IMAGE_ITEM"; id: number }
  | { type: "CLEAR_IMAGE_ITEMS" };

export type PageSelection = "all" | "range";

export interface PdfToImagesState {
  items: PdfItem[];
  pageSelection: PageSelection;
  rangeText: string;
  format: RenderFormatChoice;
  dpi: number;
  outputDir: OutputDirLabel | null;
  rangeResult: CheckPageRangeResult | null;
  rangeError: IpcError | null;
  rangeChecking: boolean;
}

export type PdfToImagesAction =
  | { type: "ADD_PDF_ITEMS"; items: PdfItem[] }
  | { type: "REMOVE_PDF_ITEM"; id: number }
  | { type: "CLEAR_PDF_ITEMS" }
  | { type: "SET_PAGE_SELECTION"; selection: PageSelection }
  | { type: "SET_RANGE_TEXT"; rangeText: string }
  | { type: "SET_RENDER_FORMAT"; format: RenderFormatChoice }
  | { type: "SET_RENDER_DPI"; dpi: number }
  | { type: "SET_PDFS_OUTPUT_DIR"; outputDir: OutputDirLabel | null }
  | { type: "CHECK_RANGE_STARTED" }
  | { type: "CHECK_RANGE_SUCCESS"; result: CheckPageRangeResult }
  | { type: "CHECK_RANGE_FAILURE"; error: IpcError }
  | { type: "CHECK_RANGE_RESET" };

export interface AppState {
  language: LanguageState;
  imagesToPdf: ImagesToPdfState;
  pdfToImages: PdfToImagesState;
  job: JobState;
}

export type AppAction =
  LanguageAction | ImagesToPdfAction | PdfToImagesAction | JobAction;
