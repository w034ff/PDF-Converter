import type { Language } from "../i18n";
import type {
  ImageItem,
  OutputDirLabel,
  OutputMode,
  PageSizeChoice,
  PdfItem,
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
  output: OutputMode;
  pageSize: PageSizeChoice;
  outputDir: OutputDirLabel | null;
}

export type ImagesToPdfAction =
  | { type: "ADD_IMAGE_ITEMS"; items: ImageItem[] }
  | { type: "REMOVE_IMAGE_ITEM"; id: number }
  | { type: "CLEAR_IMAGE_ITEMS" }
  | { type: "SET_IMAGES_OUTPUT_MODE"; output: OutputMode }
  | { type: "SET_IMAGES_PAGE_SIZE"; pageSize: PageSizeChoice }
  | { type: "SET_IMAGES_OUTPUT_DIR"; outputDir: OutputDirLabel | null }
  | { type: "MOVE_IMAGE_ITEM"; fromIndex: number; toIndex: number };

export interface PdfToImagesState {
  items: PdfItem[];
}

export type PdfToImagesAction =
  | { type: "ADD_PDF_ITEMS"; items: PdfItem[] }
  | { type: "REMOVE_PDF_ITEM"; id: number }
  | { type: "CLEAR_PDF_ITEMS" };

export interface AppState {
  language: LanguageState;
  imagesToPdf: ImagesToPdfState;
  pdfToImages: PdfToImagesState;
  job: JobState;
}

export type AppAction =
  LanguageAction | ImagesToPdfAction | PdfToImagesAction | JobAction;
