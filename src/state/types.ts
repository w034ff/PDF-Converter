import type { Language } from "../i18n";
import type { ImageItem, PdfItem } from "../ipc";

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
}

export type AppAction = LanguageAction | ImagesToPdfAction | PdfToImagesAction;
